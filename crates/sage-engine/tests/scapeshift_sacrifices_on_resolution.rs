//! Scapeshift sacrifices **on resolution**, and searches for *up to* that many.
//!
//! Issue #721 filed the card as an additional cast cost, and the branch that authored it
//! followed the issue. It is not one. Gatherer is explicit: *"You sacrifice the lands as
//! part of the resolution of Scapeshift. It isn't an additional cost. If Scapeshift is
//! countered, you won't sacrifice any lands."* Two things were wrong, and both are visible
//! from the table — the lands went away too early, and the search took exactly as many
//! cards as the lands it ate rather than up to that many.
//!
//! So the headline test here is the counterspell: the whole content of "it isn't a cost"
//! is that a countered Scapeshift leaves the board exactly as it found it. Everything else
//! is the shape of the two questions the resolution now poses — the sacrifice, whose size
//! the player picks, and the search it sizes.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, choice_candidates, pending_player_choice, permanent_choice_bounds,
    permanent_choice_candidates, valid_actions, Action, CardDatabase, CardId, CardInstance,
    CardInstanceId, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, pools stocked so payability never
/// decides a test that is about a resolution, and an explicit RNG seed so "the shuffle
/// replays" is a checkable claim (ADR 0006).
fn main_phase(seed: u64) -> GameState {
    let mut state = GameState::new_two_player_with_seed(seed);
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        ..Default::default()
    });
    id
}

/// Put `slug` into `seat`'s hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Give `seat` a library of `slugs`, in order.
fn library_of(state: &mut GameState, db: &CardDatabase, seat: PlayerId, slugs: &[&str]) {
    let cards: Vec<CardInstance> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[seat.0].library = cards;
}

/// The requirement form of casting `card` — the shape `valid_actions` advertises, and the
/// whole of Scapeshift's announcement now that it carries no payment.
fn offer(card: CardInstance) -> Action {
    Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// Pass priority twice — both seats — so the top of the stack resolves.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let once = apply_action(state, &Action::PassPriority, db);
    apply_action(&once, &Action::PassPriority, db)
}

/// Whether `id` is on the battlefield.
fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// The permanent ids the currently owed sacrifice offers, with its clamped bounds.
fn sacrifice_offer(state: &GameState, db: &CardDatabase) -> (Vec<PermanentId>, (u32, u32)) {
    let pending = pending_player_choice(state).expect("a choice is owed");
    let request = pending.question.permanents().expect("the sacrifice");
    (
        permanent_choice_candidates(state, request, db),
        permanent_choice_bounds(state, request, db),
    )
}

/// Cast Scapeshift with `lands` on the battlefield and `library` behind it, resolve it,
/// and stop at the sacrifice question.
fn scapeshift_resolving(
    db: &CardDatabase,
    seed: u64,
    lands: &[&str],
    library: &[&str],
) -> (GameState, Vec<PermanentId>) {
    let mut state = main_phase(seed);
    let placed: Vec<PermanentId> = lands
        .iter()
        .map(|slug| place(&mut state, db, slug, PlayerId(0)))
        .collect();
    let scapeshift = to_hand(&mut state, db, "scapeshift", PlayerId(0));
    library_of(&mut state, db, PlayerId(0), library);
    (
        settle(&apply_action(&state, &offer(scapeshift), db), db),
        placed,
    )
}

// ----- the headline: a countered Scapeshift sacrifices nothing ---------------

#[test]
fn issue_721_countering_scapeshift_sacrifices_nothing() {
    // The whole point of the fix, and the one thing an additional cost could never get
    // right. A cost is paid as the spell goes on the stack (CR 601.2h), so the lands would
    // already be in a graveyard by the time anybody could respond; the sacrifice belongs to
    // the resolution, and CR 701.5a removes a countered spell from the stack before any of
    // its effects happen.
    let db = db();
    let mut state = main_phase(0x5CA9_E541_0000_0001);
    let lands: Vec<PermanentId> = ["forest", "mountain", "island"]
        .iter()
        .map(|slug| place(&mut state, &db, slug, PlayerId(0)))
        .collect();
    let scapeshift = to_hand(&mut state, &db, "scapeshift", PlayerId(0));
    let cancel = to_hand(&mut state, &db, "cancel", PlayerId(1));
    library_of(&mut state, &db, PlayerId(0), &["plains", "swamp"]);

    let announced = apply_action(&state, &offer(scapeshift), &db);
    assert_eq!(announced.stack.len(), 1, "the spell is on the stack");
    assert!(
        lands.iter().all(|id| on_battlefield(&announced, *id)),
        "announcing it costs no lands at all — there is no cost to pay",
    );

    // Priority passes to the opponent, who counters it before it ever resolves.
    let spell = announced.stack[0].id;
    let passed = apply_action(&announced, &Action::PassPriority, &db);
    let cast = apply_action(
        &passed,
        &Action::CastSpell {
            card: cancel,
            mode: None,
            x: None,
            targets: vec![Target::Spell(spell)],
            payment: Vec::new(),
        },
        &db,
    );
    let countered = settle(&settle(&cast, &db), &db);

    assert!(
        countered.stack.is_empty(),
        "both spells have left the stack"
    );
    assert!(
        pending_player_choice(&countered).is_none(),
        "and it never got as far as asking anything",
    );
    assert!(
        lands.iter().all(|id| on_battlefield(&countered, *id)),
        "so the lands are all still there — the whole content of \"it isn't a cost\"",
    );
    assert_eq!(
        countered.players[0].library.len(),
        2,
        "and nothing was found either",
    );
    assert!(
        countered.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == scapeshift.id),
        "the countered sorcery reached its owner's graveyard (CR 701.5a)",
    );
}

// ----- the sacrifice: any number, including none -----------------------------

#[test]
fn issue_721_the_sacrifice_is_asked_on_resolution_and_takes_any_number() {
    // "Any number of lands" is one question with a floor of zero and a ceiling of every
    // land the player controls — the ordinary permanent selection, with the bounds doing
    // all the work. The opponent's land is not on offer (CR 701.17b).
    let db = db();
    let mut state = main_phase(0x5CA9_E541_0000_0002);
    let mine: Vec<PermanentId> = ["forest", "mountain", "island"]
        .iter()
        .map(|slug| place(&mut state, &db, slug, PlayerId(0)))
        .collect();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let theirs = place(&mut state, &db, "swamp", PlayerId(1));
    let scapeshift = to_hand(&mut state, &db, "scapeshift", PlayerId(0));
    library_of(&mut state, &db, PlayerId(0), &["plains"]);

    let posed = settle(&apply_action(&state, &offer(scapeshift), &db), &db);
    let pending = pending_player_choice(&posed).expect("the sacrifice");
    assert_eq!(pending.chooser, PlayerId(0), "its controller answers it");
    let (candidates, (min, max)) = sacrifice_offer(&posed, &db);
    assert_eq!(min, 0, "sacrificing none is a legal answer");
    assert_eq!(max, 3, "…and so is sacrificing every land");
    assert_eq!(candidates, mine, "only the caster's own lands");
    assert!(!candidates.contains(&ogre), "a creature is not a land");
    assert!(!candidates.contains(&theirs), "and theirs is not yours");
}

#[test]
fn issue_721_sacrificing_no_lands_is_legal_and_finds_nothing() {
    // Zero is an answer, not a refusal. The search that follows is sized zero, and a
    // selection whose clamped maximum is zero is applied outright rather than posed — so
    // the spell resolves, the library is shuffled, and nothing is found.
    let db = db();
    let (posed, lands) = scapeshift_resolving(
        &db,
        0x5CA9_E541_0000_0003,
        &["forest", "mountain"],
        &["plains", "island", "shock"],
    );
    let (_, (min, _)) = sacrifice_offer(&posed, &db);
    assert_eq!(min, 0);

    let after = apply_action(
        &posed,
        &Action::AnswerPermanents { chosen: Vec::new() },
        &db,
    );
    assert!(
        pending_player_choice(&after).is_none(),
        "a search for zero cards asks nothing",
    );
    assert!(
        lands.iter().all(|id| on_battlefield(&after, *id)),
        "no land was taken",
    );
    assert_eq!(after.players[0].library.len(), 3, "nothing was found");
    assert_eq!(
        after.battlefield.len(),
        2,
        "and nothing arrived on the battlefield",
    );
}

#[test]
fn issue_721_an_empty_board_never_poses_the_sacrifice_at_all() {
    // The never-stall rule (ADR 0013 §5) rather than a clause on this card: a sacrifice
    // with nothing to sacrifice is skipped, the count stays zero, and the search that
    // reads it finds nothing. Scapeshift on an empty board is a legal, blank cast.
    let db = db();
    let (after, _) = scapeshift_resolving(&db, 0x5CA9_E541_0000_0004, &[], &["plains", "island"]);
    assert!(
        pending_player_choice(&after).is_none(),
        "neither question was worth asking",
    );
    assert!(after.battlefield.is_empty(), "nothing arrived");
    assert_eq!(after.players[0].library.len(), 2);
}

#[test]
fn cr_701_17_the_sacrificed_lands_really_leave_for_a_graveyard() {
    // A real sacrifice down the one leaves-battlefield seam, not a permanent quietly
    // dropped: the three lands are in their owner's graveyard afterwards, by instance, and
    // the fourth is untouched.
    //
    // A dies-watcher is on the board and stays silent, which is the *correct* observation
    // rather than a missing one: CR 700.4 says only a creature dies, so a land leaving for
    // a graveyard is a zone change and nothing more. The watcher is here to prove the seam
    // does not mislabel it.
    let db = db();
    let mut state = main_phase(0x5CA9_E541_0000_0005);
    let archer = place(&mut state, &db, "poison_tip_archer", PlayerId(0));
    let doomed: Vec<PermanentId> = ["forest", "mountain", "island"]
        .iter()
        .map(|slug| place(&mut state, &db, slug, PlayerId(0)))
        .collect();
    let kept = place(&mut state, &db, "swamp", PlayerId(0));
    let instances: Vec<CardInstanceId> = doomed
        .iter()
        .map(|id| {
            state
                .battlefield
                .iter()
                .find(|perm| perm.id == *id)
                .unwrap()
                .instance
        })
        .collect();
    let scapeshift = to_hand(&mut state, &db, "scapeshift", PlayerId(0));
    library_of(&mut state, &db, PlayerId(0), &["plains"]);
    let their_life = state.players[1].life;

    let posed = settle(&apply_action(&state, &offer(scapeshift), &db), &db);
    let after = apply_action(
        &posed,
        &Action::AnswerPermanents {
            chosen: doomed.clone(),
        },
        &db,
    );

    assert!(
        doomed.iter().all(|id| !on_battlefield(&after, *id)),
        "all three left the battlefield",
    );
    assert!(on_battlefield(&after, kept), "the fourth was not asked for");
    for instance in instances {
        assert!(
            after.players[0]
                .graveyard
                .iter()
                .any(|card| card.id == instance),
            "each sacrificed land is in its owner's graveyard",
        );
    }
    assert!(on_battlefield(&after, archer), "the watcher is still there");
    assert_eq!(
        after.players[1].life, their_life,
        "and it never fired: a land does not die (CR 700.4)",
    );
}

// ----- the search: up to that many -------------------------------------------

#[test]
fn issue_721_three_lands_sacrificed_allows_finding_up_to_three() {
    // The amount the second sentence reads back. Three sacrificed is a ceiling of three —
    // and a floor of none, because failing to find is always legal (CR 701.19c). The
    // library holds four lands, so the ceiling is the card's and not the pile's.
    let db = db();
    let (posed, lands) = scapeshift_resolving(
        &db,
        0x5CA9_E541_0000_0006,
        &["forest", "mountain", "island", "swamp"],
        &["plains", "island", "mountain", "forest", "shock"],
    );

    let searching = apply_action(
        &posed,
        &Action::AnswerPermanents {
            chosen: lands[..3].to_vec(),
        },
        &db,
    );
    let pending = pending_player_choice(&searching).expect("the search");
    let request = pending.question.cards().expect("a card selection");
    assert_eq!(request.max, 3, "up to that many — the three it just ate");
    assert_eq!(
        request.min, 0,
        "failing to find is always legal (CR 701.19c)",
    );
    let candidates = choice_candidates(&searching, request, &db);
    assert_eq!(candidates.len(), 4, "the four land cards in the library");

    let chosen: Vec<CardInstanceId> = candidates.iter().take(3).map(|card| card.id).collect();
    let after = apply_action(&searching, &Action::AnswerChoice { chosen }, &db);
    let arrived: Vec<&Permanent> = after
        .battlefield
        .iter()
        .filter(|perm| perm.id == lands[3] || !lands.contains(&perm.id))
        .filter(|perm| perm.id != lands[3])
        .collect();
    assert_eq!(arrived.len(), 3, "three lands found");
    assert!(
        arrived.iter().all(|perm| perm.tapped),
        "…and they enter tapped, as the card says",
    );
}

#[test]
fn cr_701_19c_finding_fewer_than_that_many_is_legal_and_so_is_finding_none() {
    // "Up to" in both directions: the same three-land sacrifice may be followed by a find
    // of one, or of nothing at all. This is the half the additional-cost authoring got
    // wrong — it searched for exactly as many as it ate.
    let db = db();
    let build = || {
        scapeshift_resolving(
            &db,
            0x5CA9_E541_0000_0007,
            &["forest", "mountain", "island"],
            &["plains", "island", "mountain", "shock"],
        )
    };

    for take in [0usize, 1] {
        let (posed, lands) = build();
        let searching = apply_action(
            &posed,
            &Action::AnswerPermanents {
                chosen: lands.clone(),
            },
            &db,
        );
        let pending = pending_player_choice(&searching).expect("the search");
        let request = pending.question.cards().expect("a card selection");
        let chosen: Vec<CardInstanceId> = choice_candidates(&searching, request, &db)
            .iter()
            .take(take)
            .map(|card| card.id)
            .collect();
        let after = apply_action(&searching, &Action::AnswerChoice { chosen }, &db);

        assert!(
            pending_player_choice(&after).is_none(),
            "the resolution finished",
        );
        assert_eq!(
            after.battlefield.len(),
            take,
            "exactly the {take} the player asked for arrived",
        );
        assert_eq!(
            after.players[0].library.len(),
            4 - take,
            "and the rest stayed in the library",
        );
    }
}

// ----- determinism ------------------------------------------------------------

#[test]
fn adr_0006_the_search_shuffles_deterministically_from_the_seed() {
    // A search ends with a shuffle (CR 701.19c) whether or not it found anything, and the
    // shuffle draws from the seeded stream — so the same seed and the same answers replay
    // to the same library, and a different seed does not.
    let db = db();
    let play = |seed: u64| {
        let (posed, lands) = scapeshift_resolving(
            &db,
            seed,
            &["forest", "mountain"],
            &["plains", "island", "mountain", "forest", "swamp", "shock"],
        );
        let searching = apply_action(
            &posed,
            &Action::AnswerPermanents {
                chosen: lands[..1].to_vec(),
            },
            &db,
        );
        let pending = pending_player_choice(&searching).expect("the search");
        let request = pending.question.cards().expect("a card selection");
        let chosen = vec![choice_candidates(&searching, request, &db)[0].id];
        apply_action(&searching, &Action::AnswerChoice { chosen }, &db)
    };

    let seed = 0x5CA9_E541_0000_0008;
    let once = play(seed);
    assert_eq!(once, play(seed), "the replay is identical");

    let library = |state: &GameState| -> Vec<CardInstanceId> {
        state.players[0]
            .library
            .iter()
            .map(|card| card.id)
            .collect()
    };
    let other = play(0x0BAD_F00D_0BAD_F00D);
    assert_ne!(
        library(&once),
        library(&other),
        "and the shuffle really is the seed's, not a fixed order",
    );
}

// ----- the shapes this fix deleted are still gone ----------------------------

#[test]
fn issue_721_scapeshift_announces_no_payment_at_all() {
    // The cast carries nothing: no sacrifice slot, no candidate list, no recorded payment.
    // What `valid_actions` advertises is the bare cast, and that is the whole announcement.
    let db = db();
    let mut state = main_phase(0x5CA9_E541_0000_0009);
    place(&mut state, &db, "forest", PlayerId(0));
    let scapeshift = to_hand(&mut state, &db, "scapeshift", PlayerId(0));
    library_of(&mut state, &db, PlayerId(0), &["plains"]);

    assert!(valid_actions(&state, &db).contains(&offer(scapeshift)));
    assert!(
        sage_engine::sacrifice_cost(&state, &db, scapeshift).is_none(),
        "there is no additional cost to pose a slot for",
    );
    let announced = apply_action(&state, &offer(scapeshift), &db);
    assert_eq!(announced.stack.len(), 1);
    assert_eq!(
        announced.stack[0].paid,
        sage_engine::PaidCost::default(),
        "and the stack object records an empty payment",
    );
}
