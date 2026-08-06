//! Choosing the order of cards put back on a library (issue #746).
//!
//! A look that says *put the rest on the bottom of your library in any order* asks its
//! controller **twice**: which card to take, and then how to arrange what is left. The
//! second question is a permutation rather than a selection, which is the whole reason it
//! is its own shape — and it is the first answer in the engine that must reach the game
//! without touching the seeded RNG, because a bottoming the player chose and a bottoming
//! the game rolled cannot share a stream and still replay.
//!
//! Every test drives the **real** [`apply_action`] pipeline. Anticipate is the card, but
//! what is under test is the mechanism: the never-stall rules over a remainder, the
//! permutation legality rule, the resume riding from the first question to the second,
//! and the determinism promise.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, choice_bounds, choice_candidates, order_candidates, pending_player_choice,
    Action, CardDatabase, CardId, CardInstanceId, ChoiceQuestion, Color, FunctionalId, GameState,
    PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity. Never a written-down `CardId`.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, stack empty, pools stocked, with an
/// explicit RNG seed so "the stream did not move" is a checkable claim.
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

/// Give `seat` a library of exactly these cards, **listed top first**.
fn library_of(state: &mut GameState, db: &CardDatabase, seat: PlayerId, slugs: &[&str]) {
    let instances: Vec<_> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[seat.0].library = instances.into_iter().rev().collect();
}

/// Cast `slug` from player 0's hand and let both seats pass, so it resolves.
fn cast(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    assert!(!state.stack.is_empty(), "{slug} did not reach the stack");
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Answer the pending **card selection** with the candidates at `picks`.
fn take(state: &GameState, db: &CardDatabase, picks: &[usize]) -> GameState {
    let pending = pending_player_choice(state).expect("a choice is owed");
    let candidates = choice_candidates(state, pending.question.cards().expect("a selection"), db);
    let chosen: Vec<CardInstanceId> = picks.iter().map(|&i| candidates[i].id).collect();
    let after = apply_action(state, &Action::AnswerChoice { chosen }, db);
    assert_ne!(after, *state, "the take was rejected as illegal");
    after
}

/// The pending **ordering**'s cards, top of the library first.
fn remainder(state: &GameState) -> Vec<CardInstanceId> {
    let request = pending_player_choice(state)
        .expect("an ordering is owed")
        .question
        .order()
        .expect("an ordering");
    order_candidates(state, request)
        .into_iter()
        .map(|inst| inst.id)
        .collect()
}

/// Answer the pending ordering with the remainder rearranged to `picks` (indices into
/// [`remainder`]), first named ending up deepest.
fn arrange(state: &GameState, db: &CardDatabase, picks: &[usize]) -> GameState {
    let cards = remainder(state);
    let order: Vec<CardInstanceId> = picks.iter().map(|&i| cards[i]).collect();
    let after = apply_action(state, &Action::AnswerOrder { order }, db);
    assert_ne!(after, *state, "the ordering was rejected as illegal");
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

/// The card names in `seat`'s hand.
fn hand_names(state: &GameState, db: &CardDatabase, seat: PlayerId) -> Vec<String> {
    state.players[seat.0]
        .hand
        .iter()
        .map(|inst| db.card(inst.card).expect("a bundled card").name.clone())
        .collect()
}

/// Anticipate resolving over a four-card library, parked on its second question.
fn anticipate_arranging(db: &CardDatabase, seed: u64) -> GameState {
    let mut state = main_phase(seed);
    library_of(
        &mut state,
        db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain"],
    );
    let asked = cast(&state, db, "anticipate");
    take(&asked, db, &[1])
}

// ----- the card -------------------------------------------------------------

#[test]
fn issue_746_anticipate_takes_one_and_bottoms_the_rest_in_the_submitted_order() {
    // Both questions, in order, on the real pipeline. The first is the ordinary look:
    // three cards, take one. The second is the new one, and its answer is the whole of
    // where those cards end up.
    let db = db();
    let mut state = main_phase(7);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain"],
    );
    let asked = cast(&state, &db, "anticipate");

    let pending = pending_player_choice(&asked).expect("the look");
    let looked_at: Vec<String> = choice_candidates(&asked, pending.question.cards().unwrap(), &db)
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(
        looked_at,
        vec!["Forest", "Island", "Swamp"],
        "the top three"
    );

    // Take the Island. The two left over are still on top of the library, which is
    // exactly where the ordering question derives them from.
    let arranging = take(&asked, &db, &[1]);
    assert!(hand_names(&arranging, &db, PlayerId(0)).contains(&"Island".to_string()));
    let pending = pending_player_choice(&arranging).expect("the arrangement is now owed");
    assert_eq!(pending.chooser, PlayerId(0), "the looker arranges");
    assert_eq!(pending.question.order().expect("an ordering").count, 2);
    let names: Vec<String> = order_candidates(&arranging, pending.question.order().unwrap())
        .into_iter()
        .map(|inst| db.card(inst.card).unwrap().name.clone())
        .collect();
    assert_eq!(names, vec!["Forest", "Swamp"], "what was not taken");

    // Swamp first, so Swamp is deepest and Forest sits just above it.
    let after = arrange(&arranging, &db, &[1, 0]);
    assert_eq!(
        library_names(&after, &db, PlayerId(0)),
        vec!["Mountain", "Forest", "Swamp"],
        "the untouched card is on top, then the arrangement, first named deepest",
    );

    // The other arrangement really is a different game, which is the point of asking.
    assert_eq!(
        library_names(&arrange(&arranging, &db, &[0, 1]), &db, PlayerId(0)),
        vec!["Mountain", "Swamp", "Forest"],
    );

    // And the rest of the resolution rode across from the first question to the second:
    // the spell finished and its card reached the graveyard (CR 608.3).
    assert!(after.stack.is_empty(), "the spell finished resolving");
    assert!(pending_player_choice(&after).is_none());
    assert_eq!(
        after.players[0]
            .graveyard
            .iter()
            .filter(|inst| inst.card == cid(&db, "anticipate"))
            .count(),
        1,
        "Anticipate reached its owner's graveyard",
    );
    assert_eq!(after.priority, PlayerId(0), "priority came back");
}

#[test]
fn issue_746_only_the_chooser_may_answer_and_nothing_else_is_on_offer() {
    // The freeze the choice queue promises, over the new shape: while the arrangement is
    // owed its chooser is offered the answer (and a concede) and no other seat is offered
    // anything at all.
    let db = db();
    let arranging = anticipate_arranging(&db, 7);

    let offers = sage_engine::valid_actions(&arranging, &db);
    assert!(offers.contains(&Action::AnswerOrder { order: Vec::new() }));
    assert!(offers
        .iter()
        .all(|action| matches!(action, Action::AnswerOrder { .. } | Action::Concede)));

    let mut opponents_turn = arranging.clone();
    opponents_turn.priority = PlayerId(1);
    assert!(sage_engine::valid_actions(&opponents_turn, &db).is_empty());
}

// ----- the legality rule ----------------------------------------------------

#[test]
fn issue_746_a_malformed_ordering_is_rejected_rather_than_partly_obeyed() {
    // A permutation has exactly one legal size and no repeats, so all three ways of
    // getting it wrong are refused outright — each as a no-op, leaving the question
    // still owed rather than half-answered.
    let db = db();
    let arranging = anticipate_arranging(&db, 7);
    let cards = remainder(&arranging);
    let (forest, swamp) = (cards[0], cards[1]);

    // A card in the library but *below* the remainder: the Mountain the look never saw.
    let mountain = arranging.players[0]
        .library
        .iter()
        .find(|inst| inst.card == cid(&db, "mountain"))
        .expect("still in the library")
        .id;
    // A card in hand, and an id that names nothing at all.
    let island = arranging.players[0]
        .hand
        .iter()
        .find(|inst| inst.card == cid(&db, "island"))
        .expect("the taken card")
        .id;

    for malformed in [
        vec![forest, forest],                  // a duplicate
        vec![swamp, swamp],                    // the other duplicate
        vec![forest, mountain],                // a card the remainder does not contain
        vec![forest, island],                  // a card that is not even in the library
        vec![forest, CardInstanceId(999_999)], // an id from nowhere
        vec![forest],                          // too short
        vec![forest, swamp, mountain],         // too long
        Vec::new(),                            // and no answer at all
    ] {
        assert_eq!(
            apply_action(&arranging, &Action::AnswerOrder { order: malformed }, &db),
            arranging,
            "a malformed ordering changes nothing",
        );
    }

    // The well-formed answers are the two permutations, and both are accepted.
    for order in [vec![forest, swamp], vec![swamp, forest]] {
        assert_ne!(
            apply_action(&arranging, &Action::AnswerOrder { order }, &db),
            arranging,
        );
    }

    // An ordering aimed at a question that is not one is an answer nobody asked for.
    let mut state = main_phase(7);
    library_of(&mut state, &db, PlayerId(0), &["forest", "island", "swamp"]);
    let selecting = cast(&state, &db, "anticipate");
    assert!(matches!(
        pending_player_choice(&selecting).map(|p| &p.question),
        Some(ChoiceQuestion::Cards(_)),
    ));
    assert_eq!(
        apply_action(
            &selecting,
            &Action::AnswerOrder {
                order: choice_candidates(
                    &selecting,
                    pending_player_choice(&selecting)
                        .unwrap()
                        .question
                        .cards()
                        .unwrap(),
                    &db,
                )
                .iter()
                .map(|inst| inst.id)
                .collect(),
            },
            &db,
        ),
        selecting,
    );
}

// ----- the mandatory take ---------------------------------------------------

#[test]
fn issue_746_a_mandatory_take_cannot_be_declined() {
    // Anticipate reads *put one of them into your hand*, not "you may put". With three
    // cards to choose from the bounds are exactly one, and an answer that takes none is
    // refused by the same gate a forged card id is — as a no-op, with the question still
    // owed rather than half-answered.
    let db = db();
    let mut state = main_phase(7);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "island", "swamp", "mountain"],
    );
    let asked = cast(&state, &db, "anticipate");

    let pending = pending_player_choice(&asked).expect("the look");
    let request = pending.question.cards().expect("a selection");
    assert_eq!(
        choice_bounds(&asked, request, &db),
        (1, 1),
        "exactly one, not up to one",
    );

    let candidates: Vec<CardInstanceId> = choice_candidates(&asked, request, &db)
        .into_iter()
        .map(|inst| inst.id)
        .collect();
    for refused in [
        Vec::new(),                         // declining the take
        vec![candidates[0], candidates[1]], // and taking two of them
    ] {
        assert_eq!(
            apply_action(&asked, &Action::AnswerChoice { chosen: refused }, &db),
            asked,
            "an answer outside the mandatory bounds changes nothing",
        );
    }

    // Taking one is the only legal answer, and it works.
    let after = take(&asked, &db, &[0]);
    assert!(hand_names(&after, &db, PlayerId(0)).contains(&"Forest".to_string()));
    assert!(
        pending_player_choice(&after).is_some_and(|pending| pending.question.order().is_some()),
        "and the arrangement follows as it always did",
    );
}

#[test]
fn issue_746_a_library_that_cannot_meet_the_mandatory_take_still_settles() {
    // The floor belongs to the never-stall rule, not to the card (ADR 0013 §5): it is
    // clamped to what the zone can actually supply, so a minimum nothing can meet becomes
    // no minimum and the effect resolves with an empty selection. Two ways to get there,
    // and neither may hang.
    let db = db();

    // (a) Nothing to look at at all.
    let empty = main_phase(3);
    assert!(empty.players[0].library.is_empty());
    let after = cast(&empty, &db, "anticipate");
    assert!(
        pending_player_choice(&after).is_none(),
        "an empty library asks nothing, mandatory take or not",
    );
    assert!(after.stack.is_empty(), "the spell finished resolving");
    assert!(after.players[0].hand.is_empty(), "and took nothing");
    assert_eq!(
        after.players[0]
            .graveyard
            .iter()
            .filter(|inst| inst.card == cid(&db, "anticipate"))
            .count(),
        1,
        "Anticipate still reached its graveyard rather than hanging on the stack",
    );

    // (b) A full window, and a filter nothing in it matches — the general shape, where
    //     the library is not empty and the floor still cannot be met. No bundled card
    //     poses this, so the card is written here.
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_insistence","name":"Test Insistence",
             "types":["sorcery"],"mana_cost":"",
             "spell_effects":[
               {"kind":"look_at_top","count":3,"take":1,"take_min":1,
                "filter":{"kind":"creature"}},
               {"kind":"gain_life","player_ref":"controller","amount":3}]},
            {"schema_version":1,"functional_id":"test_waste","name":"Test Waste",
             "types":["land"],"mana_cost":""}
        ]"#,
    )
    .unwrap();
    let mut state = main_phase(3);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["test_waste", "test_waste", "test_waste", "test_waste"],
    );
    let before = state.players[0].life;
    let after = cast(&state, &db, "test_insistence");

    assert!(
        pending_player_choice(&after).is_none(),
        "no creature to take, so the mandatory take is not a question",
    );
    assert!(after.stack.is_empty());
    assert_eq!(after.players[0].hand.len(), 0, "nothing was taken");
    assert_eq!(after.players[0].library.len(), 4, "and nothing was lost");
    assert_eq!(
        after.players[0].life,
        before + 3,
        "the effect after the look still ran",
    );
}

// ----- the never-stall rules ------------------------------------------------

#[test]
fn issue_746_a_remainder_of_one_card_is_never_asked_and_still_moves() {
    // ADR 0013 §5 over a remainder: one card has one arrangement, so there is nothing to
    // decide and nothing is posed. It still has to *go* to the bottom, which is the half
    // that would be easy to lose by simply not asking.
    let db = db();
    let mut state = main_phase(11);
    library_of(&mut state, &db, PlayerId(0), &["forest", "island"]);
    let asked = cast(&state, &db, "anticipate");
    let after = take(&asked, &db, &[0]);

    assert!(
        pending_player_choice(&after).is_none(),
        "one card is not a question",
    );
    assert!(after.stack.is_empty(), "the resolution finished");
    assert_eq!(hand_names(&after, &db, PlayerId(0)), vec!["Forest"]);
    assert_eq!(library_names(&after, &db, PlayerId(0)), vec!["Island"]);
}

#[test]
fn issue_746_a_remainder_of_nothing_is_never_asked() {
    // The other end of the same rule: a one-card library is looked at, taken, and there
    // is nothing left to arrange.
    let db = db();
    let mut state = main_phase(11);
    library_of(&mut state, &db, PlayerId(0), &["forest"]);
    let asked = cast(&state, &db, "anticipate");
    let after = take(&asked, &db, &[0]);

    assert!(pending_player_choice(&after).is_none());
    assert!(after.players[0].library.is_empty());
    assert_eq!(hand_names(&after, &db, PlayerId(0)), vec!["Forest"]);
}

#[test]
fn issue_746_a_look_that_can_take_nothing_still_asks_for_the_arrangement() {
    // The settled-outright path of `pose_choices`: the *take* has no legal answer, so it
    // is applied with an empty selection and never posed — but the remainder is still
    // three cards, and "in any order" is still a decision. Skipping the first question
    // must not skip the second, and the rest of the resolution has to end up riding on
    // whichever question was actually asked.
    //
    // No bundled card poses this, so the card is written here.
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_foresight","name":"Test Foresight",
             "types":["sorcery"],"mana_cost":"",
             "spell_effects":[
               {"kind":"look_at_top","count":3,"take":1,
                "filter":{"kind":"creature"},"bottom_order":"chosen"},
               {"kind":"draw_card","count":1}]},
            {"schema_version":1,"functional_id":"test_waste","name":"Test Waste",
             "types":["land"],"mana_cost":""}
        ]"#,
    )
    .unwrap();
    let mut state = main_phase(11);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["test_waste", "test_waste", "test_waste", "test_waste"],
    );
    let asked = cast(&state, &db, "test_foresight");

    let pending = pending_player_choice(&asked).expect("the arrangement is owed");
    assert!(
        pending.question.order().is_some(),
        "no creature to take, so the only question left is the arrangement",
    );
    assert_eq!(pending.question.order().unwrap().count, 3);

    let after = arrange(&asked, &db, &[2, 0, 1]);
    assert!(pending_player_choice(&after).is_none());
    // The effect after the look still ran, so the remainder really did ride across onto
    // the question that was asked rather than the one that was skipped.
    assert_eq!(after.players[0].hand.len(), 1, "the draw still happened");
}

// ----- determinism ----------------------------------------------------------

#[test]
fn issue_746_an_ordered_bottoming_draws_nothing_from_the_rng_and_replays_identically() {
    // The load-bearing one (ADR 0006). The random bottoming advances the seeded stream;
    // a player-ordered one must not, or every later shuffle in the game lands somewhere
    // else on replay — and the divergence would only show up turns after the card that
    // caused it.
    let db = db();
    let seed = 0xC0FF_EE00_1234_5678;

    let play = |seed: u64| {
        let mut state = main_phase(seed);
        library_of(
            &mut state,
            &db,
            PlayerId(0),
            &["forest", "island", "swamp", "mountain"],
        );
        let asked = cast(&state, &db, "anticipate");
        arrange(&take(&asked, &db, &[1]), &db, &[1, 0])
    };

    let start = main_phase(seed).rng_seed;
    let once = play(seed);
    assert_eq!(
        once.rng_seed, start,
        "the ordered bottoming consumed nothing from the seeded stream",
    );

    // Same seed, same answers, same game — the whole state, not just the library.
    assert_eq!(once, play(seed), "the replay is identical");

    // And the seed genuinely decides nothing here: a different one plays out the same,
    // which is only true because the answer replaced the roll.
    let other = play(0x0BAD_F00D_0BAD_F00D);
    assert_eq!(
        library_names(&once, &db, PlayerId(0)),
        library_names(&other, &db, PlayerId(0)),
        "a player's arrangement is not a function of the seed",
    );
}

#[test]
fn issue_746_a_random_bottoming_still_draws_from_the_rng() {
    // The control for the test above: the cards printed *in a random order* are
    // untouched by any of this, and their bottoming still moves the stream. Militia
    // Bugler looks at four and bottoms three of them at random.
    let db = db();
    let mut state = main_phase(0xC0FF_EE00_1234_5678);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "walking_corpse", "island", "swamp", "mountain"],
    );
    let mut asked = cast(&state, &db, "militia_bugler");
    while pending_player_choice(&asked).is_none() {
        asked = apply_action(&asked, &Action::PassPriority, &db);
    }
    let before = asked.rng_seed;
    let after = take(&asked, &db, &[0]);

    assert!(
        pending_player_choice(&after).is_none(),
        "a random bottoming asks no second question",
    );
    assert_ne!(after.rng_seed, before, "the shuffle drew from the stream");
}
