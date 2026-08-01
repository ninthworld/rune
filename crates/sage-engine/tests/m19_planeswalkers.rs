//! The five M19 planeswalkers, each ability driven through the **real**
//! [`apply_action`] pipeline (issue #620).
//!
//! `tests/planeswalkers.rs` proves the loyalty *mechanism* on inline definitions; this
//! file proves the five shipped cards. A card is "supported" only if playing it does
//! what the card says, so a definition that parses is not evidence of anything: every
//! test here activates a loyalty ability for real, pays for it out of real loyalty,
//! answers whatever the resolution asks, and asserts on the state that comes out.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, choice_candidates, pending_player_choice, target_requirements,
    valid_actions, Action, CardDatabase, CardId, CardInstance, CardType, Color, CounterKind,
    FunctionalId, GameState, Keyword, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main with an empty stack, pools stocked so
/// payability never decides a test that is about an effect, and libraries stocked so a
/// multi-turn walk is never cut short by the CR 704.5c decking loss.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
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
    let filler = cid(db, "forest");
    for seat in [PlayerId(0), PlayerId(1)] {
        for _ in 0..40 {
            let instance = state.new_instance(filler);
            state.players[seat.0].library.push(instance);
        }
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

/// Put a **planeswalker** of `slug` onto the battlefield under player 0 with `loyalty`
/// counters on it.
///
/// The loyalty is set explicitly rather than taken from the printed value because every
/// ultimate here costs more than the card enters with, and climbing to seven by
/// activating `+1` over six turns would be testing the turn structure rather than the
/// ability. Setting it is also *mandatory*, not a convenience: a planeswalker placed
/// with no loyalty counters is collected by CR 704.5i before anything can be activated.
fn place_walker(state: &mut GameState, db: &CardDatabase, slug: &str, loyalty: u32) -> PermanentId {
    let id = place(state, db, slug, PlayerId(0));
    let perm = state.battlefield.iter_mut().find(|p| p.id == id).unwrap();
    perm.counters.insert(CounterKind::Loyalty, loyalty);
    id
}

/// Put `slug` into `seat`'s hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Put `slug` into `seat`'s graveyard and return the instance.
fn to_graveyard(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// Whether the priority holder is currently offered ability `index` of `permanent`.
fn offers(state: &GameState, db: &CardDatabase, permanent: PermanentId, index: usize) -> bool {
    valid_actions(state, db).contains(&Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
    })
}

/// Activate loyalty ability `index` of `permanent` with `targets` and let it resolve,
/// asserting that the engine actually offered it first.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    targets: Vec<Target>,
) -> GameState {
    assert!(
        offers(state, db, permanent, index),
        "ability {index} was not offered"
    );
    let after = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets,
        },
        db,
    );
    assert_ne!(&after, state, "the activation was rejected");
    let after = apply_action(&after, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

/// Take one action on behalf of whoever holds priority: pass where passing is offered,
/// otherwise the first non-concede action there is.
fn advance(state: &GameState, db: &CardDatabase) -> GameState {
    let offered = valid_actions(state, db);
    let chosen = if offered.contains(&Action::PassPriority) {
        Action::PassPriority
    } else {
        offered
            .into_iter()
            .find(|a| a != &Action::Concede)
            .expect("some action is always available")
    };
    let after = apply_action(state, &chosen, db);
    assert_ne!(&after, state, "the pipeline stalled");
    after
}

/// How many permanents on the battlefield are named `name`.
fn count_named(state: &GameState, db: &CardDatabase, name: &str) -> usize {
    state
        .battlefield
        .iter()
        .filter(|p| p.printed.face(db).is_some_and(|face| face.name() == name))
        .count()
}

/// The loyalty on `id`, or `0` when it is no longer on the battlefield.
fn loyalty(state: &GameState, id: PermanentId) -> u32 {
    state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.counter_count(CounterKind::Loyalty))
}

// ----- the shape all five share ---------------------------------------------

#[test]
fn issue_620_every_planeswalker_enters_with_its_printed_loyalty() {
    // CR 306.5b and CR 606.1 over all five at once: each is a planeswalker with a
    // printed starting loyalty and exactly three loyalty abilities, and each really
    // arrives with that many counters when cast through the pipeline.
    let db = db();
    let expected = [
        ("ajani_adversary_of_tyrants", "{2}{W}{W}", 4),
        ("tezzeret_artifice_master", "{3}{U}{U}", 5),
        ("liliana_untouched_by_death", "{2}{B}{B}", 5),
        ("sarkhan_fireblood", "{1}{R}{R}", 4),
        ("vivien_reid", "{3}{G}{G}", 5),
    ];
    for (slug, cost, starting) in expected {
        let card = db.card(cid(&db, slug)).unwrap();
        assert!(card.has_type(CardType::Planeswalker), "{slug}");
        assert_eq!(card.mana_cost, cost, "{slug}");
        assert_eq!(card.loyalty, Some(starting), "{slug}");
        assert_eq!(
            card.abilities.len(),
            3,
            "{slug} has three loyalty abilities"
        );
        assert!(
            card.abilities.iter().all(sage_engine::is_loyalty_ability),
            "{slug}'s abilities are all loyalty abilities"
        );

        let mut state = main_phase(&db);
        let instance = to_hand(&mut state, &db, slug, PlayerId(0));
        let cast = apply_action(
            &state,
            &Action::CastSpell {
                card: instance,
                targets: Vec::new(),
            },
            &db,
        );
        let cast = apply_action(&cast, &Action::PassPriority, &db);
        let resolved = apply_action(&cast, &Action::PassPriority, &db);
        let walker = resolved
            .battlefield
            .iter()
            .find(|p| p.instance == instance.id)
            .unwrap_or_else(|| panic!("{slug} did not reach the battlefield"));
        assert_eq!(
            walker.counter_count(CounterKind::Loyalty),
            starting,
            "{slug}"
        );
    }
}

#[test]
fn issue_620_an_ultimate_the_walker_cannot_pay_for_is_never_offered() {
    // CR 606.3: a loyalty cost is payable only out of loyalty the permanent has.
    // Vivien's `-8` is unofferable at 5 and offerable at 8.
    let db = db();
    let mut state = main_phase(&db);
    let vivien = place_walker(&mut state, &db, "vivien_reid", 5);
    assert!(
        !offers(&state, &db, vivien, 2),
        "-8 is unpayable at 5 loyalty"
    );

    let mut richer = state;
    richer
        .battlefield
        .iter_mut()
        .find(|p| p.id == vivien)
        .unwrap()
        .counters
        .insert(CounterKind::Loyalty, 8);
    assert!(
        offers(&richer, &db, vivien, 2),
        "-8 is payable at 8 loyalty"
    );
}

// ----- Ajani, Adversary of Tyrants ------------------------------------------

#[test]
fn issue_620_ajani_plus_one_counters_up_to_two_targets_and_accepts_fewer() {
    // "Put a +1/+1 counter on each of **up to two** target creatures": zero, one, and
    // two are all legal announcements, and each does exactly as much as it named.
    let db = db();
    let mut base = main_phase(&db);
    let ajani = place_walker(&mut base, &db, "ajani_adversary_of_tyrants", 4);
    let first = place(&mut base, &db, "onakke_ogre", PlayerId(0));
    let second = place(&mut base, &db, "onakke_ogre", PlayerId(0));

    let two = activate(
        &base,
        &db,
        ajani,
        0,
        vec![Target::Permanent(first), Target::Permanent(second)],
    );
    assert_eq!(characteristics(&two, first, &db).power, Some(5));
    assert_eq!(characteristics(&two, second, &db).power, Some(5));
    assert_eq!(loyalty(&two, ajani), 5, "+1 raised the loyalty");

    let one = activate(&base, &db, ajani, 0, vec![Target::Permanent(first)]);
    assert_eq!(characteristics(&one, first, &db).power, Some(5));
    assert_eq!(characteristics(&one, second, &db).power, Some(4));

    // None at all: still a legal activation, and not a fizzle — it never had a target
    // to lose (CR 608.2b).
    let none = activate(&base, &db, ajani, 0, Vec::new());
    assert_eq!(characteristics(&none, first, &db).power, Some(4));
    assert_eq!(characteristics(&none, second, &db).power, Some(4));
    assert_eq!(loyalty(&none, ajani), 5);
}

#[test]
fn issue_620_ajani_plus_one_refuses_one_creature_named_twice_or_a_third_target() {
    // CR 601.2c: the targets of one group must be different objects, and there is no
    // third slot to fill. Both are rejected at the gate, so the activation is a no-op.
    let db = db();
    let mut state = main_phase(&db);
    let ajani = place_walker(&mut state, &db, "ajani_adversary_of_tyrants", 4);
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let other = place(&mut state, &db, "sun_sentinel", PlayerId(0));
    let third = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    for illegal in [
        vec![Target::Permanent(ogre), Target::Permanent(ogre)],
        vec![
            Target::Permanent(ogre),
            Target::Permanent(other),
            Target::Permanent(third),
        ],
    ] {
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: ajani,
                index: 0,
                targets: illegal.clone(),
            },
            &db,
        );
        assert_eq!(after, state, "{illegal:?} must be rejected outright");
    }
}

#[test]
fn issue_620_ajani_plus_one_advertises_two_optional_slots() {
    // The requirement surface: two slots for the same spec, both optional, so "up to
    // two" is reconstructable from the view rather than from a rule a client would have
    // to know.
    let db = db();
    let mut state = main_phase(&db);
    let ajani = place_walker(&mut state, &db, "ajani_adversary_of_tyrants", 4);
    place(&mut state, &db, "onakke_ogre", PlayerId(0));

    let requirements = target_requirements(
        &state,
        &db,
        &Action::ActivateAbility {
            permanent: ajani,
            index: 0,
            targets: Vec::new(),
        },
    );
    assert_eq!(requirements.len(), 2, "two slots for up to two targets");
    assert!(
        requirements.iter().all(|r| r.optional),
        "an 'up to' group's slots may all be left empty"
    );
}

#[test]
fn issue_620_ajani_plus_one_is_offered_with_no_creature_on_the_board() {
    // The other half of "up to": an announcement choosing nothing is legal, so an empty
    // board is no reason to withhold the ability — unlike the `-2`, whose one slot is
    // mandatory and which is therefore withheld (CR 601.2c).
    let db = db();
    let mut state = main_phase(&db);
    let ajani = place_walker(&mut state, &db, "ajani_adversary_of_tyrants", 4);
    assert_eq!(state.battlefield.len(), 1, "only Ajani is on the board");
    assert!(
        offers(&state, &db, ajani, 0),
        "+1 is offered with no targets"
    );
    assert!(
        !offers(&state, &db, ajani, 1),
        "-2 is withheld with no creature card in the graveyard"
    );
}

#[test]
fn issue_620_ajani_minus_two_returns_a_cheap_creature_card_from_the_graveyard() {
    // "Return target creature card with mana value 2 or less from your graveyard to the
    // battlefield." The cap is real — a three-drop in the same graveyard is not a
    // candidate — and what comes back is a fresh battlefield object.
    let db = db();
    let mut state = main_phase(&db);
    let ajani = place_walker(&mut state, &db, "ajani_adversary_of_tyrants", 4);
    // Walking Corpse is {1}{B}: mana value 2, within the cap.
    let cheap = to_graveyard(&mut state, &db, "walking_corpse", PlayerId(0));
    // Onakke Ogre is {2}{R}: mana value 3, outside it.
    let dear = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));

    let requirements = target_requirements(
        &state,
        &db,
        &Action::ActivateAbility {
            permanent: ajani,
            index: 1,
            targets: Vec::new(),
        },
    );
    assert_eq!(requirements.len(), 1);
    assert!(
        !requirements[0].optional,
        "the -2's one target is mandatory"
    );
    assert_eq!(
        requirements[0].candidates,
        vec![Target::Card(cheap.id)],
        "only the mana-value-2-or-less creature card is a candidate"
    );

    let after = activate(&state, &db, ajani, 1, vec![Target::Card(cheap.id)]);
    assert_eq!(loyalty(&after, ajani), 2, "-2 was paid out of 4");
    assert!(
        after
            .battlefield
            .iter()
            .any(|p| p.instance == cheap.id && p.controller == PlayerId(0)),
        "the returned card is on the battlefield under Ajani's controller"
    );
    assert!(
        !after.players[0].graveyard.iter().any(|c| c.id == cheap.id),
        "and it has left the graveyard"
    );
    assert!(
        after.players[0].graveyard.iter().any(|c| c.id == dear.id),
        "the expensive creature card stayed put"
    );
}

#[test]
fn issue_620_ajanis_emblem_makes_three_lifelinking_cats_at_each_of_its_end_steps() {
    // The ultimate, all the way through: seven loyalty spent, Ajani collected by
    // CR 704.5i before the ability even resolves, and the emblem it left behind making
    // cats for the rest of the game — on its controller's end steps only.
    let db = db();
    let mut state = main_phase(&db);
    let ajani = place_walker(&mut state, &db, "ajani_adversary_of_tyrants", 7);

    let after = activate(&state, &db, ajani, 2, Vec::new());
    assert!(
        !after.battlefield.iter().any(|p| p.id == ajani),
        "seven loyalty spent leaves Ajani at zero, and CR 704.5i collects it"
    );
    assert_eq!(after.emblems.len(), 1, "the emblem outlived its source");

    let start_turn = after.turn;
    let mut walked = after;
    while walked.turn == start_turn {
        walked = advance(&walked, &db);
    }
    assert_eq!(count_named(&walked, &db, "Cat"), 3);
    let cat = walked
        .battlefield
        .iter()
        .find(|p| p.printed.face(&db).is_some_and(|f| f.name() == "Cat"))
        .unwrap();
    let cat_characteristics = characteristics(&walked, cat.id, &db);
    assert_eq!(cat_characteristics.power, Some(1));
    assert_eq!(cat_characteristics.toughness, Some(1));
    assert!(cat_characteristics.keywords.contains(&Keyword::Lifelink));
    assert_eq!(cat.controller, PlayerId(0));

    // The opponent's end step adds none; the controller's next one adds three more.
    while walked.turn == start_turn + 1 {
        walked = advance(&walked, &db);
    }
    assert_eq!(
        count_named(&walked, &db, "Cat"),
        3,
        "the emblem is scoped to its controller's own end step"
    );
    while walked.turn == start_turn + 2 {
        walked = advance(&walked, &db);
    }
    assert_eq!(count_named(&walked, &db, "Cat"), 6);
}

// ----- Tezzeret, Artifice Master --------------------------------------------

#[test]
fn issue_620_tezzeret_plus_one_makes_a_colourless_flying_thopter() {
    let db = db();
    let mut state = main_phase(&db);
    let tezzeret = place_walker(&mut state, &db, "tezzeret_artifice_master", 5);

    let after = activate(&state, &db, tezzeret, 0, Vec::new());

    assert_eq!(loyalty(&after, tezzeret), 6);
    let thopter = after
        .battlefield
        .iter()
        .find(|p| p.printed.face(&db).is_some_and(|f| f.name() == "Thopter"))
        .expect("a Thopter token");
    let face = thopter.printed.face(&db).unwrap();
    assert!(face.has_type(CardType::Artifact) && face.has_type(CardType::Creature));
    assert!(face.colors().is_empty(), "a Thopter token is colourless");
    assert_eq!(face.power(), Some(1));
    assert!(characteristics(&after, thopter.id, &db)
        .keywords
        .contains(&Keyword::Flying));
}

#[test]
fn issue_620_tezzeret_zero_draws_one_or_two_by_the_artifact_count() {
    // "Draw a card. If you control three or more artifacts, draw two cards instead."
    // The condition is judged on resolution against the board as it then is, so the same
    // ability draws a different number on two boards differing by one artifact.
    let db = db();
    let mut two_artifacts = main_phase(&db);
    let tezzeret = place_walker(&mut two_artifacts, &db, "tezzeret_artifice_master", 5);
    for _ in 0..2 {
        place(&mut two_artifacts, &db, "millstone", PlayerId(0));
    }
    let before = two_artifacts.players[0].hand.len();

    let short = activate(&two_artifacts, &db, tezzeret, 1, Vec::new());
    assert_eq!(
        short.players[0].hand.len() - before,
        1,
        "two artifacts is not three"
    );
    assert_eq!(loyalty(&short, tezzeret), 5, "a 0 ability costs nothing");

    let mut three_artifacts = two_artifacts;
    place(&mut three_artifacts, &db, "millstone", PlayerId(0));
    let long = activate(&three_artifacts, &db, tezzeret, 1, Vec::new());
    assert_eq!(
        long.players[0].hand.len() - before,
        2,
        "three artifacts draws two"
    );
}

#[test]
fn issue_620_tezzeret_zero_counts_only_its_controllers_artifacts() {
    // "You control three or more artifacts" — an opponent's do not count, which is why
    // the count is controller-relative rather than a board scan.
    let db = db();
    let mut state = main_phase(&db);
    let tezzeret = place_walker(&mut state, &db, "tezzeret_artifice_master", 5);
    for _ in 0..3 {
        place(&mut state, &db, "millstone", PlayerId(1));
    }
    let before = state.players[0].hand.len();

    let after = activate(&state, &db, tezzeret, 1, Vec::new());

    assert_eq!(after.players[0].hand.len() - before, 1);
}

#[test]
fn issue_620_tezzerets_emblem_searches_a_permanent_onto_the_battlefield_each_end_step() {
    // The ultimate: nine loyalty, and an emblem that searches at every one of its
    // controller's end steps. The search is a mid-resolution choice, so the walk has to
    // answer it — which is the point: an emblem's trigger reaches the choice machinery
    // exactly as a permanent's does.
    let db = db();
    let mut state = main_phase(&db);
    let tezzeret = place_walker(&mut state, &db, "tezzeret_artifice_master", 9);
    let after = activate(&state, &db, tezzeret, 2, Vec::new());
    assert!(!after.battlefield.iter().any(|p| p.id == tezzeret));
    assert_eq!(after.emblems.len(), 1);

    let start_turn = after.turn;
    let mut walked = after;
    let mut asked = 0;
    while walked.turn == start_turn {
        if let Some(pending) = pending_player_choice(&walked) {
            let request = pending.question.cards().expect("the search asks for cards");
            let candidates = choice_candidates(&walked, request, &db);
            assert!(
                !candidates.is_empty(),
                "a stocked library offers a permanent card to find"
            );
            walked = apply_action(
                &walked,
                &Action::AnswerChoice {
                    chosen: vec![candidates[0].id],
                },
                &db,
            );
            asked += 1;
            continue;
        }
        walked = advance(&walked, &db);
    }
    assert_eq!(asked, 1, "the emblem asked exactly once, at the end step");
    assert!(
        count_named(&walked, &db, "Forest") >= 1,
        "the found permanent card is on the battlefield"
    );
}

// ----- Liliana, Untouched by Death ------------------------------------------

#[test]
fn issue_620_liliana_plus_one_drains_only_when_a_zombie_was_milled_this_way() {
    // "Mill three cards. If at least one Zombie card was milled this way, each opponent
    // loses 2 life and you gain 2." The condition reads what *this resolution* milled —
    // a Zombie already sitting in the graveyard proves nothing.
    let db = db();
    let mut without = main_phase(&db);
    let liliana = place_walker(&mut without, &db, "liliana_untouched_by_death", 5);
    to_graveyard(&mut without, &db, "walking_corpse", PlayerId(0));

    let quiet = activate(&without, &db, liliana, 0, Vec::new());
    assert_eq!(quiet.players[0].life, sage_engine::STARTING_LIFE);
    assert_eq!(quiet.players[1].life, sage_engine::STARTING_LIFE);
    assert_eq!(
        quiet.players[0].graveyard.len(),
        4,
        "three cards were milled"
    );

    // Now put a Zombie on top of the library: the same ability drains.
    let mut with = without;
    let zombie = with.new_instance(cid(&db, "walking_corpse"));
    with.players[0].library.push(zombie);

    let drained = activate(&with, &db, liliana, 0, Vec::new());
    assert_eq!(drained.players[0].life, sage_engine::STARTING_LIFE + 2);
    assert_eq!(drained.players[1].life, sage_engine::STARTING_LIFE - 2);
}

#[test]
fn issue_620_liliana_minus_two_shrinks_a_creature_by_the_zombies_you_control() {
    // "Target creature gets -X/-X until end of turn, where X is the number of Zombies
    // you control." X is fixed on resolution, so a Zombie that dies afterwards does not
    // give the shrunk creature its toughness back.
    let db = db();
    let mut state = main_phase(&db);
    let liliana = place_walker(&mut state, &db, "liliana_untouched_by_death", 5);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0)); // a 4/2
    let zombie = place(&mut state, &db, "walking_corpse", PlayerId(0));
    place(&mut state, &db, "walking_corpse", PlayerId(1)); // an opponent's Zombie

    let after = activate(&state, &db, liliana, 1, vec![Target::Permanent(victim)]);

    assert_eq!(loyalty(&after, liliana), 3);
    let shrunk = characteristics(&after, victim, &db);
    assert_eq!(shrunk.power, Some(3), "one Zombie you control, not two");
    assert_eq!(shrunk.toughness, Some(1));

    let mut later = after;
    later.battlefield.retain(|p| p.id != zombie);
    assert_eq!(
        characteristics(&later, victim, &db).toughness,
        Some(1),
        "the modifier is a fixed amount, not a live count"
    );
}

#[test]
fn issue_620_liliana_minus_two_can_be_lethal_with_enough_zombies() {
    let db = db();
    let mut state = main_phase(&db);
    let liliana = place_walker(&mut state, &db, "liliana_untouched_by_death", 5);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0)); // a 4/2
    for _ in 0..2 {
        place(&mut state, &db, "walking_corpse", PlayerId(0));
    }

    let after = activate(&state, &db, liliana, 1, vec![Target::Permanent(victim)]);

    assert!(
        !after.battlefield.iter().any(|p| p.id == victim),
        "-2/-2 puts a 4/2 at zero toughness, and CR 704.5f collects it"
    );
}

#[test]
fn issue_620_liliana_minus_three_lets_zombie_creature_spells_be_cast_from_the_graveyard() {
    // "You may cast Zombie creature spells from your graveyard this turn." The cards
    // stay in the graveyard until cast; only Zombies are offered; the permission lapses
    // with the turn.
    let db = db();
    let mut state = main_phase(&db);
    let liliana = place_walker(&mut state, &db, "liliana_untouched_by_death", 5);
    let zombie = to_graveyard(&mut state, &db, "walking_corpse", PlayerId(0));
    let not_a_zombie = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));

    let cast = |card: CardInstance| Action::CastSpell {
        card,
        targets: Vec::new(),
    };
    assert!(
        !valid_actions(&state, &db).contains(&cast(zombie)),
        "nothing is castable from a graveyard without the permission"
    );

    let after = activate(&state, &db, liliana, 2, Vec::new());
    assert_eq!(loyalty(&after, liliana), 2);
    assert!(
        valid_actions(&after, &db).contains(&cast(zombie)),
        "the Zombie creature card is now castable from the graveyard"
    );
    assert!(
        !valid_actions(&after, &db).contains(&cast(not_a_zombie)),
        "a non-Zombie in the same graveyard is not"
    );

    // Casting it really moves it: graveyard → stack → battlefield.
    let cast_state = apply_action(&after, &cast(zombie), &db);
    assert!(!cast_state.players[0]
        .graveyard
        .iter()
        .any(|c| c.id == zombie.id));
    let resolved = apply_action(&cast_state, &Action::PassPriority, &db);
    let resolved = apply_action(&resolved, &Action::PassPriority, &db);
    assert!(
        resolved.battlefield.iter().any(|p| p.instance == zombie.id),
        "the Zombie is on the battlefield"
    );

    // The permission is scoped to the turn it was granted on.
    let start_turn = after.turn;
    let mut next_turn = after;
    while next_turn.turn == start_turn {
        next_turn = advance(&next_turn, &db);
    }
    assert!(
        next_turn.graveyard_casting.is_empty(),
        "the permission lapses with the turn"
    );
    assert!(
        !valid_actions(&next_turn, &db).contains(&cast(zombie)),
        "and the card is no longer castable from the graveyard"
    );
}

// ----- Sarkhan, Fireblood ---------------------------------------------------

#[test]
fn issue_620_sarkhan_plus_one_discards_then_draws_and_draws_nothing_from_an_empty_hand() {
    // "Discard a card. If a card is discarded this way, draw a card." The intervening-if
    // is what makes the empty hand safe: the discard moves nothing, so nothing is drawn.
    let db = db();
    let mut state = main_phase(&db);
    let sarkhan = place_walker(&mut state, &db, "sarkhan_fireblood", 4);
    let held = to_hand(&mut state, &db, "shock", PlayerId(0));

    let activated = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: sarkhan,
            index: 0,
            targets: Vec::new(),
        },
        &db,
    );
    let activated = apply_action(&activated, &Action::PassPriority, &db);
    let resolving = apply_action(&activated, &Action::PassPriority, &db);
    let pending = pending_player_choice(&resolving).expect("a discard is owed");
    assert_eq!(pending.chooser, PlayerId(0));

    let answered = apply_action(
        &resolving,
        &Action::AnswerChoice {
            chosen: vec![held.id],
        },
        &db,
    );
    assert!(answered.players[0]
        .graveyard
        .iter()
        .any(|c| c.id == held.id));
    assert_eq!(
        answered.players[0].hand.len(),
        1,
        "one card discarded, one drawn"
    );
    assert_eq!(loyalty(&answered, sarkhan), 5);

    // With an empty hand the question is never posed, and nothing is drawn.
    let mut empty = main_phase(&db);
    let sarkhan = place_walker(&mut empty, &db, "sarkhan_fireblood", 4);
    empty.players[0].hand.clear();
    let after = activate(&empty, &db, sarkhan, 0, Vec::new());
    assert!(
        after.players[0].hand.is_empty(),
        "no card was discarded, so none is drawn"
    );
    assert!(pending_player_choice(&after).is_none());
}

#[test]
fn issue_620_sarkhan_plus_one_makes_mana_spendable_only_on_dragon_spells() {
    // "Add {R}{R}. Spend this mana only to cast Dragon spells." A mana ability, so it
    // never uses the stack; the restriction rides on the mana, so a Dragon spell can
    // spend it and an ordinary creature spell cannot.
    let db = db();
    let mut state = main_phase(&db);
    state.players[0].mana_pool = Default::default();
    let sarkhan = place_walker(&mut state, &db, "sarkhan_fireblood", 4);
    let dragon = to_hand(&mut state, &db, "volcanic_dragon", PlayerId(0)); // {4}{R}{R} Dragon
    let ogre = to_hand(&mut state, &db, "onakke_ogre", PlayerId(0)); // {2}{R}, not a Dragon

    let mut state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: sarkhan,
            index: 1,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(loyalty(&state, sarkhan), 5);
    assert!(
        state.stack.is_empty(),
        "a mana ability never uses the stack"
    );
    assert_eq!(state.players[0].mana_pool.total(), 2);
    // Enough colourless for the generic halves of both costs, so the *restriction* is
    // the only thing that can decide which spell is offered.
    state.players[0].mana_pool.add_colorless(4);

    let offered = valid_actions(&state, &db);
    assert!(
        offered.contains(&Action::CastSpell {
            card: dragon,
            targets: Vec::new()
        }),
        "the restricted red pays for a Dragon spell"
    );
    assert!(
        !offered.contains(&Action::CastSpell {
            card: ogre,
            targets: Vec::new()
        }),
        "and pays for nothing else — the Ogre's red pip has no unrestricted red"
    );

    let cast = apply_action(
        &state,
        &Action::CastSpell {
            card: dragon,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(
        cast.players[0].mana_pool.total(),
        0,
        "the restricted mana was spent on the Dragon"
    );
}

#[test]
fn issue_620_sarkhan_minus_seven_makes_three_flying_dragons() {
    let db = db();
    let mut state = main_phase(&db);
    let sarkhan = place_walker(&mut state, &db, "sarkhan_fireblood", 7);

    let after = activate(&state, &db, sarkhan, 2, Vec::new());

    assert!(!after.battlefield.iter().any(|p| p.id == sarkhan));
    assert_eq!(count_named(&after, &db, "Dragon"), 3);
    for dragon in after
        .battlefield
        .iter()
        .filter(|p| p.printed.face(&db).is_some_and(|f| f.name() == "Dragon"))
    {
        let face = dragon.printed.face(&db).unwrap();
        assert_eq!(face.power(), Some(5));
        assert_eq!(face.toughness(), Some(5));
        assert_eq!(face.colors(), [Color::Red]);
        assert!(characteristics(&after, dragon.id, &db)
            .keywords
            .contains(&Keyword::Flying));
    }
}

// ----- Vivien Reid ----------------------------------------------------------

#[test]
fn issue_620_vivien_plus_one_offers_a_creature_or_land_from_the_top_four() {
    // "Look at the top four cards of your library. You may reveal a creature or land
    // card from among them and put it into your hand." The filter is the union of two
    // classes, which is one choice on the card and had been two in the vocabulary.
    let db = db();
    let mut state = main_phase(&db);
    let vivien = place_walker(&mut state, &db, "vivien_reid", 5);
    state.players[0].library.clear();
    for slug in ["shock", "onakke_ogre", "forest", "island"] {
        let instance = state.new_instance(cid(&db, slug));
        state.players[0].library.push(instance);
    }

    let activated = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: vivien,
            index: 0,
            targets: Vec::new(),
        },
        &db,
    );
    let activated = apply_action(&activated, &Action::PassPriority, &db);
    let resolving = apply_action(&activated, &Action::PassPriority, &db);

    let pending = pending_player_choice(&resolving).expect("a look is owed");
    let request = pending.question.cards().unwrap();
    let candidates = choice_candidates(&resolving, request, &db);
    assert_eq!(
        candidates.len(),
        3,
        "the two lands and the creature, not the instant"
    );

    let taken = candidates[0];
    let after = apply_action(
        &resolving,
        &Action::AnswerChoice {
            chosen: vec![taken.id],
        },
        &db,
    );
    assert!(after.players[0].hand.iter().any(|c| c.id == taken.id));
    assert_eq!(
        after.players[0].library.len(),
        3,
        "the rest went to the bottom"
    );
    assert_eq!(loyalty(&after, vivien), 6);
}

#[test]
fn issue_620_vivien_minus_three_destroys_any_of_three_classes_and_nothing_else() {
    // One slot, three classes (CR 601.2c): an artifact, an enchantment, or a creature
    // *with flying* — and a ground creature is not a candidate at all.
    let db = db();
    let mut state = main_phase(&db);
    let vivien = place_walker(&mut state, &db, "vivien_reid", 5);
    let artifact = place(&mut state, &db, "millstone", PlayerId(1));
    let enchantment = place(&mut state, &db, "ajani_s_welcome", PlayerId(1));
    let flyer = place(&mut state, &db, "snapping_drake", PlayerId(1));
    let ground = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let requirements = target_requirements(
        &state,
        &db,
        &Action::ActivateAbility {
            permanent: vivien,
            index: 1,
            targets: Vec::new(),
        },
    );
    let candidates = &requirements[0].candidates;
    for legal in [artifact, enchantment, flyer] {
        assert!(candidates.contains(&Target::Permanent(legal)));
    }
    assert!(
        !candidates.contains(&Target::Permanent(ground)),
        "a creature without flying is outside all three classes"
    );

    let after = activate(&state, &db, vivien, 1, vec![Target::Permanent(flyer)]);
    assert!(!after.battlefield.iter().any(|p| p.id == flyer));
    assert_eq!(loyalty(&after, vivien), 2);
}

#[test]
fn issue_620_viviens_emblem_grants_two_plus_two_vigilance_trample_and_indestructible() {
    // The ultimate, and the only place in the catalog indestructible appears: an
    // emblem's static abilities make its controller's creatures bigger, vigilant,
    // trampling — and immune to destruction, which is a rules exception rather than a
    // display string, so it is proved by failing to kill something.
    let db = db();
    let mut state = main_phase(&db);
    let vivien = place_walker(&mut state, &db, "vivien_reid", 8);
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0)); // a 4/2
    let theirs = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let after = activate(&state, &db, vivien, 2, Vec::new());
    assert!(!after.battlefield.iter().any(|p| p.id == vivien));
    assert_eq!(after.emblems.len(), 1);

    let buffed = characteristics(&after, mine, &db);
    assert_eq!(buffed.power, Some(6));
    assert_eq!(buffed.toughness, Some(4));
    for keyword in [
        Keyword::Vigilance,
        Keyword::Trample,
        Keyword::Indestructible,
    ] {
        assert!(buffed.keywords.contains(&keyword), "{keyword:?}");
    }
    assert_eq!(
        characteristics(&after, theirs, &db).power,
        Some(4),
        "an opponent's creature gets none of it"
    );

    // Lethal marked damage does not destroy it (CR 702.12); the opponent's identical
    // Ogre, with the same damage and no emblem, dies.
    let mut struck = after;
    for id in [mine, theirs] {
        struck
            .battlefield
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .damage = 99;
    }
    let settled = apply_action(&struck, &Action::PassPriority, &db);
    assert!(
        settled.battlefield.iter().any(|p| p.id == mine),
        "an indestructible creature survives lethal marked damage"
    );
    assert!(
        !settled.battlefield.iter().any(|p| p.id == theirs),
        "and an ordinary one does not"
    );
}

#[test]
fn issue_620_indestructible_survives_a_destroy_effect() {
    // The other half of CR 702.12: an effect that says "destroy" does nothing to it. The
    // spell still resolves and still had a legal target — it simply achieves nothing.
    let db = db();
    let mut state = main_phase(&db);
    let vivien = place_walker(&mut state, &db, "vivien_reid", 8);
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let mut after = activate(&state, &db, vivien, 2, Vec::new());
    let murder = to_hand(&mut after, &db, "murder", PlayerId(0));

    let cast = apply_action(
        &after,
        &Action::CastSpell {
            card: murder,
            targets: vec![Target::Permanent(mine)],
        },
        &db,
    );
    let resolved = apply_action(&cast, &Action::PassPriority, &db);
    let resolved = apply_action(&resolved, &Action::PassPriority, &db);

    assert!(
        resolved.battlefield.iter().any(|p| p.id == mine),
        "destroy does nothing to an indestructible creature"
    );
    assert!(
        resolved.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == murder.id),
        "the Murder still resolved and reached the graveyard"
    );
}
