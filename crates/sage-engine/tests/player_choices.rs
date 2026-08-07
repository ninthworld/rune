//! Mid-resolution player choices (issue #604): discard, scry, look-at-the-top, and
//! library search.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! The mechanism's promises are what is under test, not any one card: that a choice
//! suspends the game, that it reaches the player the effect names rather than whoever
//! held priority, that a question with no legal answer resolves instead of stalling,
//! and that a search shuffles deterministically. The cards are the evidence.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, choice_bounds, choice_candidates, pending_player_choice, Action,
    CardDatabase, CardId, CardInstance, CardInstanceId, ChoiceOutcome, ChoiceRequest, ChoiceZone,
    Color, FunctionalId, GameEvent, GameState, PendingChoice, Permanent, PermanentId, PlayerId,
    StackObjectKind, Step, Target,
};

/// The card-selection request of a pending choice. Every choice in this file is one —
/// the yes-or-no shape has its own test file (`optional_effects.rs`).
fn cards_of(pending: &PendingChoice) -> &ChoiceRequest {
    pending.question.cards().expect("a card selection")
}

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity. Never a written-down `CardId`.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game parked at player 0's precombat main with an empty stack and both
/// pools stocked, so payability never decides a test that is about an effect.
fn main_phase() -> GameState {
    main_phase_seeded(0)
}

/// [`main_phase`] with an explicit RNG seed, for the shuffle-determinism tests.
fn main_phase_seeded(seed: u64) -> GameState {
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

/// Give `seat` a hand of exactly these cards, in order, and return the instances.
fn hand_of(
    state: &mut GameState,
    db: &CardDatabase,
    seat: PlayerId,
    slugs: &[&str],
) -> Vec<CardInstance> {
    let instances: Vec<CardInstance> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[seat.0].hand = instances.clone();
    instances
}

/// Give `seat` a library of exactly these cards, **listed top first** (the engine
/// stores a library bottom-first, so this reverses), and return the instances in the
/// same top-first order.
fn library_of(
    state: &mut GameState,
    db: &CardDatabase,
    seat: PlayerId,
    slugs: &[&str],
) -> Vec<CardInstance> {
    let instances: Vec<CardInstance> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[seat.0].library = instances.iter().rev().copied().collect();
    instances
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and
/// free of summoning sickness.
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

/// Cast `slug` from player 0's hand with `targets` and let both players pass, so it
/// resolves. Goes through the ordinary cast gate.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    assert!(
        state.stack.iter().any(|o| matches!(
            o.kind,
            StackObjectKind::Spell { card, .. } if card.id == instance.id
        )),
        "{slug} did not reach the stack — the cast was rejected"
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Resolve every triggered ability currently on the stack by passing priority, until
/// the stack is empty or the game is waiting on a choice.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..12 {
        if state.stack.is_empty() || pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// Answer the pending choice with the cards at `picks` (indices into the freshly
/// computed candidate list), in that order.
fn answer(state: &GameState, db: &CardDatabase, picks: &[usize]) -> GameState {
    let pending = pending_player_choice(state).expect("a choice is owed");
    let candidates = choice_candidates(state, cards_of(pending), db);
    let chosen: Vec<CardInstanceId> = picks.iter().map(|&i| candidates[i].id).collect();
    let after = apply_action(state, &Action::AnswerChoice { chosen }, db);
    assert_ne!(after, *state, "the answer was rejected as illegal");
    after
}

/// The card names in `seat`'s library, listed **top first**.
fn library_names(state: &GameState, db: &CardDatabase, seat: PlayerId) -> Vec<String> {
    state.players[seat.0]
        .library
        .iter()
        .rev()
        .map(|inst| db.card(inst.card).expect("a bundled card").name.clone())
        .collect()
}

/// The card names in `seat`'s hand, in hand order.
fn hand_names(state: &GameState, db: &CardDatabase, seat: PlayerId) -> Vec<String> {
    state.players[seat.0]
        .hand
        .iter()
        .map(|inst| db.card(inst.card).expect("a bundled card").name.clone())
        .collect()
}

// ----- the mechanism --------------------------------------------------------

#[test]
fn issue_604_a_choice_suspends_the_game_and_only_its_chooser_may_act() {
    // Mind Rot is the routing question in miniature: player 0 casts it, player 1
    // chooses. While the choice is owed the game offers player 1 exactly one thing and
    // player 0 nothing at all, and priority returns to player 0 once it is answered.
    let db = db();
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(1), &["forest", "shock", "murder"]);
    let state = cast(&state, &db, "mind_rot", vec![Target::Player(PlayerId(1))]);

    let pending = pending_player_choice(&state).expect("Mind Rot asked its target to discard");
    assert_eq!(pending.chooser, PlayerId(1), "the targeted seat chooses");
    assert_eq!(cards_of(pending).subject, PlayerId(1));
    assert_eq!(cards_of(pending).outcome, ChoiceOutcome::Discard);
    assert_eq!(cards_of(pending).zone, ChoiceZone::Hand);

    // Priority is with the chooser, not the caster, and is remembered for the return.
    assert_eq!(state.priority, PlayerId(1));
    assert_eq!(state.interrupted_priority, Some(PlayerId(0)));

    // The chooser is offered the answer and a concede, and nothing else — no pass, no
    // spells, no responses.
    let offered = sage_engine::valid_actions(&state, &db);
    assert_eq!(
        offered,
        vec![Action::AnswerChoice { chosen: Vec::new() }, Action::Concede],
    );

    // No other seat may act meanwhile: hand priority to the caster and the game offers
    // them nothing.
    let mut elsewhere = state.clone();
    elsewhere.priority = PlayerId(0);
    assert!(sage_engine::valid_actions(&elsewhere, &db).is_empty());

    // Two cards, exactly — the clamped bounds say so, and an answer of one is rejected.
    assert_eq!(choice_bounds(&state, cards_of(pending), &db), (2, 2));
    let candidates = choice_candidates(&state, cards_of(pending), &db);
    let short = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![candidates[0].id],
        },
        &db,
    );
    assert_eq!(short, state, "an under-filled answer changes nothing");

    let after = answer(&state, &db, &[0, 2]);
    assert_eq!(hand_names(&after, &db, PlayerId(1)), vec!["Shock"]);
    assert_eq!(after.players[1].graveyard.len(), 2);
    assert!(pending_player_choice(&after).is_none());
    // Priority came back to the seat it was taken from, and the game moves on.
    assert_eq!(after.priority, PlayerId(0));
    assert_eq!(after.interrupted_priority, None);
    assert!(after
        .log
        .iter()
        .any(|entry| matches!(entry.event, GameEvent::CardsDiscarded { count: 2, .. })));
}

#[test]
fn issue_604_the_chooser_can_be_someone_other_than_the_zones_owner() {
    // Duress reads an opponent's hand and the *caster* picks from it — the one choice
    // where chooser and subject come apart. The filter is enforced by the candidate
    // set: a creature and a land in that hand are simply not offered.
    let db = db();
    let mut state = main_phase();
    hand_of(
        &mut state,
        &db,
        PlayerId(1),
        &["forest", "walking_corpse", "murder", "shock"],
    );
    let state = cast(&state, &db, "duress", vec![Target::Player(PlayerId(1))]);

    let pending = pending_player_choice(&state).expect("Duress asked its caster to choose");
    assert_eq!(pending.chooser, PlayerId(0), "the caster chooses");
    assert_eq!(
        cards_of(pending).subject,
        PlayerId(1),
        "from the target's hand"
    );
    assert_eq!(
        state.priority,
        PlayerId(0),
        "the caster already had priority"
    );

    let offered: Vec<String> = choice_candidates(&state, cards_of(pending), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(
        offered,
        vec!["Murder", "Shock"],
        "only noncreature, nonland cards are choosable"
    );

    let after = answer(&state, &db, &[0]);
    assert_eq!(
        hand_names(&after, &db, PlayerId(1)),
        vec!["Forest", "Walking Corpse", "Shock"]
    );
    assert_eq!(after.players[1].graveyard.len(), 1);
}

#[test]
fn issue_604_a_choice_with_no_legal_answer_resolves_without_stalling() {
    // Three ways a question can have no answer, each of which must resolve rather than
    // freeze the table: an empty hand, a hand with nothing the filter admits, and an
    // empty library.
    let db = db();

    // Mind Rot at an empty hand.
    let state = main_phase();
    assert!(state.players[1].hand.is_empty());
    let after = cast(&state, &db, "mind_rot", vec![Target::Player(PlayerId(1))]);
    assert!(pending_player_choice(&after).is_none(), "nothing to ask");
    assert!(after.stack.is_empty(), "the spell finished resolving");
    assert_eq!(after.priority, PlayerId(0), "priority never moved");
    assert!(
        after.players[0]
            .graveyard
            .iter()
            .any(|c| c.card == cid(&db, "mind_rot")),
        "the sorcery still reached its graveyard (CR 608.3)"
    );

    // Duress at a hand of nothing but creatures and lands.
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(1), &["forest", "walking_corpse"]);
    let after = cast(&state, &db, "duress", vec![Target::Player(PlayerId(1))]);
    assert!(pending_player_choice(&after).is_none());
    assert_eq!(after.players[1].hand.len(), 2, "nothing was discarded");
    assert!(after.stack.is_empty());

    // A scry with an empty library.
    let mut state = main_phase();
    state.players[0].library.clear();
    let after = settle(&cast(&state, &db, "sift", Vec::new()), &db);
    // Sift's own draws deck the caster here, which is a *loss*, not a stall — the
    // point is only that nothing is left waiting on an answer.
    assert!(pending_player_choice(&after).is_none());
}

#[test]
fn issue_604_a_suspended_spell_finishes_its_remaining_effects_and_reaches_its_graveyard() {
    // Sift draws three and *then* discards, so its resolution suspends with a step
    // still to go. Neither half may be lost: the discard must happen, and the sorcery
    // itself must still reach the graveyard afterwards (CR 608.3).
    let db = db();
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(0), &["murder"]);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain"],
    );
    let state = cast(&state, &db, "sift", Vec::new());

    let pending = pending_player_choice(&state).expect("the discard suspends the resolution");
    assert_eq!(pending.chooser, PlayerId(0));
    assert!(
        state.players[0].graveyard.is_empty(),
        "the spell has not finished resolving yet"
    );

    // Discard the Murder that was already in hand; the three draws stay.
    let after = answer(&state, &db, &[0]);
    assert_eq!(
        hand_names(&after, &db, PlayerId(0)),
        vec!["Forest", "Island", "Swamp"],
        "the draws happened, and the chosen card left"
    );
    assert!(
        after.players[0]
            .graveyard
            .iter()
            .any(|c| c.card == cid(&db, "sift")),
        "the sorcery finished resolving into its graveyard"
    );
    assert!(after.stack.is_empty());
}

#[test]
fn issue_604_a_choice_can_come_after_earlier_effects_have_already_happened() {
    // Sift is Tormenting Voice's mirror: draw three, *then* discard. The discard's
    // candidate set must be the hand as it is at that moment — including the cards the
    // same spell just drew.
    let db = db();
    let mut state = main_phase();
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain"],
    );
    let state = cast(&state, &db, "sift", Vec::new());

    let pending = pending_player_choice(&state).expect("Sift discards after its draws");
    let candidates: Vec<String> = choice_candidates(&state, cards_of(pending), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(
        candidates,
        vec!["Forest", "Island", "Swamp"],
        "the three freshly drawn cards are the ones on offer"
    );

    let after = answer(&state, &db, &[1]);
    assert_eq!(
        hand_names(&after, &db, PlayerId(0)),
        vec!["Forest", "Swamp"]
    );
}

#[test]
fn issue_604_an_etb_trigger_can_pose_a_choice_to_a_seat_that_never_had_priority() {
    // Psychic Symbiont's trigger is aimed by its controller and then *resolves* into a
    // question for the targeted opponent — two hand-offs of priority in one card, in
    // opposite directions.
    let db = db();
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(1), &["shock", "murder"]);
    library_of(&mut state, &db, PlayerId(0), &["forest"]);
    let state = cast(&state, &db, "psychic_symbiont", Vec::new());

    // The trigger is on the stack owed a target; its controller aims it.
    let ability =
        sage_engine::pending_trigger_target_choice(&state).expect("the ETB trigger is unaimed");
    assert_eq!(state.priority, PlayerId(0));
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Player(PlayerId(1))],
        },
        &db,
    );
    let state = settle(&state, &db);

    let pending = pending_player_choice(&state).expect("the resolved trigger asks the opponent");
    assert_eq!(pending.chooser, PlayerId(1));
    assert_eq!(state.priority, PlayerId(1));

    let after = answer(&state, &db, &[0]);
    assert_eq!(after.players[1].hand.len(), 1);
    assert_eq!(
        hand_names(&after, &db, PlayerId(0)),
        vec!["Forest"],
        "and its controller drew afterwards"
    );
    assert_eq!(after.priority, PlayerId(0), "priority came back");
}

// ----- scry, look-and-take, search ------------------------------------------

#[test]
fn issue_604_scry_puts_the_chosen_cards_on_the_bottom_in_the_chosen_order() {
    // Omenspeaker looks at two and may bottom any number of them, so the bounds are a
    // range rather than an exact count — and the *order* of the answer is the order the
    // cards end up in.
    let db = db();
    let mut state = main_phase();
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain"],
    );
    let state = settle(&cast(&state, &db, "omenspeaker", Vec::new()), &db);

    let pending = pending_player_choice(&state).expect("the ETB scry");
    assert_eq!(cards_of(pending).outcome, ChoiceOutcome::BottomChosen);
    assert_eq!(
        choice_bounds(&state, cards_of(pending), &db),
        (0, 2),
        "any number of the two, including none"
    );
    let looked_at: Vec<String> = choice_candidates(&state, cards_of(pending), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(
        looked_at,
        vec!["Forest", "Island"],
        "the top two, top first"
    );

    // Bottom both, Island first: Island ends up deepest.
    let after = answer(&state, &db, &[1, 0]);
    assert_eq!(
        library_names(&after, &db, PlayerId(0)),
        vec!["Swamp", "Mountain", "Forest", "Island"],
    );

    // Bottoming nothing is a legal answer too, and leaves the library untouched.
    let kept = answer(&state, &db, &[]);
    assert_eq!(
        library_names(&kept, &db, PlayerId(0)),
        vec!["Forest", "Island", "Swamp", "Mountain"],
    );
}

#[test]
fn issue_604_a_look_takes_what_the_filter_admits_and_bottoms_the_rest() {
    // Militia Bugler looks at four and may take one creature with power 2 or less. The
    // filter narrows the *candidates*, but the "rest" that goes to the bottom is every
    // card it looked at, matching or not.
    let db = db();
    let mut state = main_phase();
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &[
            "forest",
            "gigantosaurus",  // a creature, power 10 — looked at, not takeable
            "walking_corpse", // a 2/2 — takeable
            "island",
            "swamp",
        ],
    );
    let state = settle(&cast(&state, &db, "militia_bugler", Vec::new()), &db);

    let pending = pending_player_choice(&state).expect("the ETB look");
    assert_eq!(choice_bounds(&state, cards_of(pending), &db), (0, 1));
    let takeable: Vec<String> = choice_candidates(&state, cards_of(pending), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(takeable, vec!["Walking Corpse"], "power 2 or less, only");

    let after = answer(&state, &db, &[0]);
    assert!(hand_names(&after, &db, PlayerId(0)).contains(&"Walking Corpse".to_string()));
    // The library is the untouched fifth card on top of the three bottomed ones, in
    // some order the seeded shuffle chose.
    let library = library_names(&after, &db, PlayerId(0));
    assert_eq!(library.len(), 4);
    assert_eq!(library[0], "Swamp", "the card never looked at stays on top");
    let mut bottomed = library[1..].to_vec();
    bottomed.sort();
    assert_eq!(bottomed, vec!["Forest", "Gigantosaurus", "Island"]);
}

#[test]
fn issue_604_a_look_that_finds_nothing_still_bottoms_what_it_looked_at() {
    // The no-legal-answer path is not "do nothing": the aftermath still happens, or a
    // Militia Bugler that whiffed would leave four known cards sitting on top.
    let db = db();
    let mut state = main_phase();
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain", "plains"],
    );
    let state = settle(&cast(&state, &db, "militia_bugler", Vec::new()), &db);

    assert!(
        pending_player_choice(&state).is_none(),
        "no creature to take, so nothing is asked"
    );
    let library = library_names(&state, &db, PlayerId(0));
    assert_eq!(library.len(), 5);
    assert_eq!(library[0], "Plains", "the fifth card is still on top");
    let mut bottomed = library[1..].to_vec();
    bottomed.sort();
    assert_eq!(bottomed, vec!["Forest", "Island", "Mountain", "Swamp"]);
}

#[test]
fn issue_604_a_look_can_put_its_take_onto_the_battlefield_tapped() {
    // Elvish Rejuvenator's found land enters through the same battlefield seam a
    // resolving permanent spell uses, so it is tapped, has a fresh identity, and is a
    // real permanent rather than a card parked somewhere.
    let db = db();
    let mut state = main_phase();
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["shock", "murder", "forest", "island", "swamp", "plains"],
    );
    let state = settle(&cast(&state, &db, "elvish_rejuvenator", Vec::new()), &db);

    let pending = pending_player_choice(&state).expect("the ETB look");
    let lands: Vec<String> = choice_candidates(&state, cards_of(pending), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(lands, vec!["Forest", "Island", "Swamp"]);

    let after = answer(&state, &db, &[1]);
    let island = after
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "island")))
        .expect("the chosen land entered the battlefield");
    assert!(island.tapped, "onto the battlefield tapped");
    assert_eq!(island.controller, PlayerId(0));
    assert_eq!(
        library_names(&after, &db, PlayerId(0))[0],
        "Plains",
        "the card below the looked-at five is still on top"
    );
}

#[test]
fn issue_604_a_search_finds_by_name_puts_it_onto_the_battlefield_and_shuffles() {
    // Elvish Clancaller searches for its own twin. The filter compares printed identity,
    // so the other Elf in the library is not a match — and the anthem half of the card
    // still works, which is what makes the two halves worth having together.
    let db = db();
    let mut state = main_phase();
    let caller = place(&mut state, &db, "elvish_clancaller", PlayerId(0));
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &[
            "forest",
            "llanowar_elves",
            "elvish_clancaller",
            "island",
            "swamp",
        ],
    );
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: caller,
            index: 1,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = settle(&state, &db);

    let pending = pending_player_choice(&state).expect("the search");
    assert_eq!(cards_of(pending).zone, ChoiceZone::Library);
    let found: Vec<String> = choice_candidates(&state, cards_of(pending), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(
        found,
        vec!["Elvish Clancaller"],
        "another Elf is not a card with this card's name"
    );

    let after = answer(&state, &db, &[0]);
    let twin = after
        .battlefield
        .iter()
        .find(|perm| {
            perm.printed.card() == Some(cid(&db, "elvish_clancaller")) && perm.id != caller
        })
        .expect("the found copy is on the battlefield");
    assert!(!twin.tapped);
    assert_eq!(after.players[0].library.len(), 4, "the found card left");
    assert!(after
        .log
        .iter()
        .any(|entry| matches!(entry.event, GameEvent::LibrarySearched { .. })));

    // Each Clancaller pumps the *other* Elves, so both are 2/2 and Llanowar Elves would
    // be a 3/3 if it were out. The anthem is unaffected by the search machinery.
    assert_eq!(characteristics(&after, caller, &db).power, Some(2));
    assert_eq!(characteristics(&after, twin.id, &db).power, Some(2));
}

#[test]
fn issue_604_a_search_shuffles_deterministically_and_a_failed_one_shuffles_too() {
    // The shuffle draws from the seeded stream, so the same seed replays the same
    // post-search order — and a search that finds nothing (always legal, CR 701.19c)
    // shuffles all the same, which is what stops it being a free look at the deck.
    let db = db();
    let deck = [
        "forest", "island", "swamp", "mountain", "plains", "shock", "murder",
    ];

    let order_for = |seed: u64| {
        let mut state = main_phase_seeded(seed);
        let caller = place(&mut state, &db, "elvish_clancaller", PlayerId(0));
        library_of(&mut state, &db, PlayerId(0), &deck);
        let state = settle(
            &apply_action(
                &state,
                &Action::ActivateAbility {
                    permanent: caller,
                    index: 1,
                    targets: Vec::new(),
                    payment: Vec::new(),
                },
                &db,
            ),
            &db,
        );
        // Nothing named Elvish Clancaller is in this library, so there is nothing to
        // ask: the search resolves outright — and still shuffles.
        assert!(pending_player_choice(&state).is_none());
        library_names(&state, &db, PlayerId(0))
    };

    let once = order_for(0xC0FF_EE00_1234_5678);
    let again = order_for(0xC0FF_EE00_1234_5678);
    assert_eq!(once, again, "the same seed replays the same shuffle");
    assert_ne!(
        once,
        deck.iter()
            .map(|slug| db.card(cid(&db, slug)).unwrap().name.clone())
            .collect::<Vec<_>>(),
        "the library really was shuffled",
    );
    assert_ne!(
        once,
        order_for(0x0BAD_F00D_0BAD_F00D),
        "a different seed shuffles differently",
    );
}

// ----- answering ------------------------------------------------------------

#[test]
fn issue_604_an_answer_naming_a_card_outside_the_candidate_set_is_rejected() {
    // The regenerate-and-check discipline: membership is tested against the set that
    // exists now, so a card in the chooser's own hand that this choice never offered —
    // and a card that is not in the zone at all — are both refused, as a no-op.
    let db = db();
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(1), &["forest", "shock", "murder"]);
    let mine = hand_of(&mut state, &db, PlayerId(0), &["island"]);
    let state = cast(&state, &db, "duress", vec![Target::Player(PlayerId(1))]);

    // Duress offers only the opponent's noncreature, nonland cards.
    let forest = state.players[1]
        .hand
        .iter()
        .find(|inst| inst.card == cid(&db, "forest"))
        .expect("the land is still in hand")
        .id;
    for forged in [forest, mine[0].id, CardInstanceId(9_999_999)] {
        assert_eq!(
            apply_action(
                &state,
                &Action::AnswerChoice {
                    chosen: vec![forged]
                },
                &db,
            ),
            state,
            "an answer outside the candidate set changes nothing",
        );
    }

    // Naming the same legal card twice is not two cards.
    let mut two_deep = main_phase();
    hand_of(&mut two_deep, &db, PlayerId(1), &["shock", "murder"]);
    let two_deep = cast(
        &two_deep,
        &db,
        "mind_rot",
        vec![Target::Player(PlayerId(1))],
    );
    let repeated = choice_candidates(
        &two_deep,
        cards_of(pending_player_choice(&two_deep).unwrap()),
        &db,
    )[0]
    .id;
    assert_eq!(
        apply_action(
            &two_deep,
            &Action::AnswerChoice {
                chosen: vec![repeated, repeated]
            },
            &db,
        ),
        two_deep,
        "one card cannot answer a two-card discard twice",
    );
}

#[test]
fn issue_604_a_short_hand_discards_what_it_has() {
    // "Discard two cards" with one card in hand is one discard, not a deadlock: the
    // bounds clamp to what is actually there.
    let db = db();
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(1), &["shock"]);
    let state = cast(&state, &db, "mind_rot", vec![Target::Player(PlayerId(1))]);

    let pending = pending_player_choice(&state).expect("one card is still a choice");
    assert_eq!(choice_bounds(&state, cards_of(pending), &db), (1, 1));
    let after = answer(&state, &db, &[0]);
    assert!(after.players[1].hand.is_empty());
    assert_eq!(after.players[1].graveyard.len(), 1);
}

#[test]
fn issue_604_answering_is_pure_and_the_input_state_is_never_mutated() {
    // The engine-wide invariant, restated for the one path that mutates a hidden zone
    // and resumes a suspended resolution.
    let db = db();
    let mut state = main_phase();
    hand_of(&mut state, &db, PlayerId(1), &["forest", "shock"]);
    let state = cast(&state, &db, "mind_rot", vec![Target::Player(PlayerId(1))]);
    let snapshot = state.clone();
    let _after = answer(&state, &db, &[0, 1]);
    assert_eq!(state, snapshot);
}
