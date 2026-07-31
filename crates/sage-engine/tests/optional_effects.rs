//! Optional effects (issue #610): the `you may …` a player may decline while an
//! object is resolving, and the `you may pay {1}. If you do, …` they may decline by
//! not paying.
//!
//! Every test drives the **real** [`apply_action`] pipeline. The cards are written
//! here rather than taken from the catalog because no bundled card uses this effect
//! yet — the mechanism is the prerequisite, and the cards that need it each need one
//! more thing besides. What is under test is therefore the mechanism's promises: that
//! the question suspends the game, that it reaches the ability's *controller*, that
//! declining costs the rest of the ability nothing, that a cost is really charged, that
//! a cost nobody could pay never stalls anything, and that the log tells a decline apart
//! from an effect that was never offered.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, FunctionalId,
    GameEvent, GameState, Permanent, PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

/// Four definitions, each isolating one thing:
///
/// - `test_may_draw` — a free `you may draw a card`, followed by a mandatory life gain,
///   so "declining is not a fizzle" has something to be measured against;
/// - `test_plain_draw` — the same card with the optionality removed, the control for
///   "declining leaves the game as if the effect were absent";
/// - `test_may_pay_draw` — the optional-*cost* form, costing nothing to cast so a test
///   can control the pool exactly;
/// - `test_watcher` — a permanent whose *triggered* ability is optional, for the
///   routing test: its controller is not the seat holding priority when it resolves;
/// - `test_forest` — a land, so a chooser can be given mana to make mid-resolution.
const DEFINITIONS: &str = r#"[
    {"schema_version":1,"functional_id":"test_may_draw","name":"Test May Draw",
     "types":["sorcery"],"mana_cost":"",
     "spell_effects":[{"kind":"may","effects":[{"kind":"draw_card","count":1}]},
                      {"kind":"gain_life","player_ref":"controller","amount":2}]},
    {"schema_version":1,"functional_id":"test_plain_draw","name":"Test Plain Draw",
     "types":["sorcery"],"mana_cost":"",
     "spell_effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]},
    {"schema_version":1,"functional_id":"test_may_pay_draw","name":"Test May Pay Draw",
     "types":["sorcery"],"mana_cost":"",
     "spell_effects":[{"kind":"may","cost":"{G}","effects":[{"kind":"draw_card","count":1}]},
                      {"kind":"gain_life","player_ref":"controller","amount":2}]},
    {"schema_version":1,"functional_id":"test_watcher","name":"Test Watcher",
     "types":["creature"],"mana_cost":"{2}","power":1,"toughness":1,
     "abilities":[{"type":"triggered",
                   "event":{"permanent_enters":{"scope":"any_creature","except_this":true}},
                   "effects":[{"kind":"may","effects":[{"kind":"draw_card","count":1}]}]}]},
    {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
     "types":["creature"],"mana_cost":"","power":2,"toughness":2},
    {"schema_version":1,"functional_id":"test_forest","name":"Test Forest",
     "types":["land"],"mana_cost":"",
     "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                   "effects":[{"kind":"add_mana","color":"green","amount":1}]}]}
]"#;

fn db() -> CardDatabase {
    CardDatabase::from_json(DEFINITIONS).expect("the test definitions load")
}

/// The interned handle for an authored identity. Never a written-down `CardId`.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a defined card")
}

/// A two-player game at player 0's precombat main, holding priority, with **empty
/// pools**: every card here costs nothing to cast, so the only mana in any test is the
/// mana that test put there on purpose.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free
/// of summoning sickness.
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
        card,
        controller,
        ..Default::default()
    });
    id
}

/// Give player 0 a library of `count` identical cards, so a draw has somewhere to draw
/// from and a decking loss never confuses a test about drawing.
fn stock_library(state: &mut GameState, db: &CardDatabase, count: usize) {
    let card = cid(db, "test_bear");
    state.players[0].library = (0..count).map(|_| state.new_instance(card)).collect();
}

/// Cast `slug` from player 0's hand and let both players pass, so it resolves.
fn cast(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Answer the pending yes-or-no.
fn answer(state: &GameState, db: &CardDatabase, accept: bool) -> GameState {
    apply_action(state, &Action::AnswerConfirm { accept }, db)
}

/// Every event of the log, for order and presence assertions.
fn events(state: &GameState) -> Vec<&GameEvent> {
    state.log.iter().map(|entry| &entry.event).collect()
}

/// Whether the log records a declined optional effect.
fn declined(state: &GameState) -> bool {
    events(state)
        .iter()
        .any(|event| matches!(event, GameEvent::OptionalDeclined { .. }))
}

// ----- the mechanism --------------------------------------------------------

#[test]
fn issue_610_an_optional_effect_suspends_resolution_until_its_controller_answers() {
    // The question is owed, the caster is the one asked, and it is the *only* thing
    // they may do: no pass, no cast, nothing. Every other seat is offered nothing at
    // all, which is what "the game does not proceed" means concretely.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);
    let state = cast(&state, &db, "test_may_draw");

    let pending = sage_engine::pending_player_choice(&state).expect("the question is owed");
    assert_eq!(pending.chooser, PlayerId(0));
    assert_eq!(state.priority, PlayerId(0), "priority goes to the chooser");

    assert_eq!(
        valid_actions(&state, &db),
        vec![Action::AnswerConfirm { accept: false }, Action::Concede],
        "the chooser answers, concedes, and does nothing else",
    );

    let mut other_seat = state.clone();
    other_seat.priority = PlayerId(1);
    assert!(
        valid_actions(&other_seat, &db).is_empty(),
        "no other seat may act while the question is owed",
    );

    // The effects the spell had not reached have not happened yet, and neither has the
    // spell's own trip to the graveyard (CR 608.3).
    assert_eq!(state.players[0].life, 20);
    assert!(state.players[0].graveyard.is_empty());
}

#[test]
fn issue_610_declining_leaves_the_game_as_if_the_effect_were_absent() {
    // Measured against the same card with the optionality removed: after a decline the
    // two agree on every zone and on the life total. Declining skips the optional
    // effect and costs the rest of the resolution nothing.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);

    let declined_run = answer(&cast(&state, &db, "test_may_draw"), &db, false);
    let control = cast(&state, &db, "test_plain_draw");

    assert!(
        sage_engine::pending_player_choice(&declined_run).is_none(),
        "the question is answered and gone",
    );
    assert_eq!(
        declined_run.players[0].hand.len(),
        control.players[0].hand.len()
    );
    assert_eq!(
        declined_run.players[0].library.len(),
        control.players[0].library.len(),
        "declining draws nothing",
    );
    assert_eq!(declined_run.players[0].life, control.players[0].life);
    assert_eq!(
        declined_run.players[0].life, 22,
        "the sibling effect resolved"
    );
    assert_eq!(
        declined_run.players[0].graveyard.len(),
        1,
        "the spell still reached its final zone (CR 608.3)",
    );
    assert_eq!(
        declined_run.priority,
        PlayerId(0),
        "priority is handed back"
    );
}

#[test]
fn issue_610_accepting_applies_the_optional_effect_before_the_rest_of_the_ability() {
    // Order is the point: the optional effects are spliced onto the *front* of what was
    // left, so the card happens in printed order rather than optional-effects-last.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);
    let state = answer(&cast(&state, &db, "test_may_draw"), &db, true);

    assert_eq!(state.players[0].hand.len(), 1, "the card was drawn");
    assert_eq!(state.players[0].library.len(), 2);
    assert_eq!(
        state.players[0].life, 22,
        "and the sibling effect still resolved"
    );

    let order: Vec<usize> = events(&state)
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            matches!(
                event,
                GameEvent::OptionalApplied { .. }
                    | GameEvent::CardsDrawn { .. }
                    | GameEvent::LifeChanged { .. }
            )
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(order.len(), 3, "acceptance, draw, life gain");
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "the accepted effect precedes the effects that followed it",
    );
}

#[test]
fn issue_610_the_chooser_is_the_abilitys_controller_not_the_priority_holder() {
    // Seat 1's watcher triggers on seat 0's creature entering. Seat 0 casts and holds
    // priority throughout; the question still reaches seat 1, and priority returns to
    // seat 0 once it is answered.
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "test_watcher", PlayerId(1));
    state.players[1].library = (0..3)
        .map(|_| state.new_instance(cid(&db, "test_bear")))
        .collect();

    // The creature resolves, the trigger goes on the stack, and both seats pass again.
    let state = cast(&state, &db, "test_bear");
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let pending = sage_engine::pending_player_choice(&state).expect("the trigger asks");
    assert_eq!(
        pending.chooser,
        PlayerId(1),
        "the ability's controller answers, not the seat that caused it",
    );
    assert_eq!(state.priority, PlayerId(1));
    assert!(valid_actions(&state, &db).contains(&Action::AnswerConfirm { accept: false }));

    let after = answer(&state, &db, true);
    assert_eq!(after.players[1].hand.len(), 1, "seat 1 drew, not seat 0");
    assert!(after.players[0].hand.is_empty());
    assert_eq!(
        after.priority,
        PlayerId(0),
        "the interrupted priority holder gets it back",
    );
}

// ----- paying for it --------------------------------------------------------

#[test]
fn issue_610_accepting_charges_the_cost_and_an_unpaid_acceptance_is_illegal() {
    // With exactly one green floating, accepting spends it and draws. With none, the
    // acceptance is not on offer *and* is refused when submitted directly — the
    // hardening gate re-derives payability rather than trusting the offer list.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);
    state.players[0].mana_pool.add(sage_engine::Color::Green, 1);
    let owed = cast(&state, &db, "test_may_pay_draw");
    assert!(sage_engine::confirm_is_payable(&owed));

    let paid = answer(&owed, &db, true);
    assert_eq!(paid.players[0].hand.len(), 1, "the card was drawn");
    assert_eq!(paid.players[0].mana_pool.green, 0, "and the mana was spent");
    assert_eq!(paid.players[0].life, 22);

    // The same question with an empty pool: no acceptance, and none to be forced.
    let mut broke = owed.clone();
    broke.players[0].mana_pool.green = 0;
    assert!(!sage_engine::confirm_is_payable(&broke));
    assert_eq!(
        valid_actions(&broke, &db),
        vec![Action::AnswerConfirm { accept: false }, Action::Concede],
        "an acceptance nobody can pay for is never offered",
    );
    assert_eq!(
        answer(&broke, &db, true),
        broke,
        "and submitting it anyway changes nothing",
    );
}

#[test]
fn issue_610_a_cost_no_tapping_could_pay_is_declined_without_stalling() {
    // Nothing in the pool and nothing to tap: the question has no yes in it, so it is
    // never posed. The rest of the spell resolves in the same pass, and the log says
    // the effect was declined rather than saying nothing at all.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);
    let state = cast(&state, &db, "test_may_pay_draw");

    assert!(
        sage_engine::pending_player_choice(&state).is_none(),
        "an unanswerable question is not asked",
    );
    assert!(state.players[0].hand.is_empty(), "nothing was drawn");
    assert_eq!(state.players[0].life, 22, "the rest of the spell resolved");
    assert_eq!(state.players[0].graveyard.len(), 1);
    assert!(declined(&state), "and the decline is on the record");
    assert_eq!(state.priority, PlayerId(0), "nobody is waiting on anything");
}

#[test]
fn issue_610_a_payer_may_float_mana_while_the_question_is_owed() {
    // CR 605.3a: a player asked to pay mid-resolution may activate mana abilities to
    // pay. An untapped Forest is why the question is posed at all, and tapping it is
    // what turns the acceptance from unavailable into legal — without anything else in
    // the game becoming legal in the meantime.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);
    let forest = place(&mut state, &db, "test_forest", PlayerId(0));
    let owed = cast(&state, &db, "test_may_pay_draw");

    assert!(
        sage_engine::pending_player_choice(&owed).is_some(),
        "mana it could still make is why this is worth asking",
    );
    assert!(
        !sage_engine::confirm_is_payable(&owed),
        "but not yet paid for"
    );
    let tap = Action::ActivateAbility {
        permanent: forest,
        index: 0,
        targets: Vec::new(),
    };
    assert_eq!(
        valid_actions(&owed, &db),
        vec![
            Action::AnswerConfirm { accept: false },
            tap.clone(),
            Action::Concede
        ],
        "the answer, the mana ability, and concede — nothing else",
    );

    let floated = apply_action(&owed, &tap, &db);
    assert_eq!(floated.players[0].mana_pool.green, 1);
    assert!(
        sage_engine::pending_player_choice(&floated).is_some(),
        "floating mana does not answer the question",
    );
    assert!(sage_engine::confirm_is_payable(&floated));

    let paid = answer(&floated, &db, true);
    assert_eq!(paid.players[0].hand.len(), 1);
    assert_eq!(paid.players[0].mana_pool.green, 0);
    assert!(paid.battlefield.iter().all(|perm| perm.tapped));
}

// ----- the log --------------------------------------------------------------

#[test]
fn issue_610_a_declined_effect_reads_differently_from_one_that_never_existed() {
    // The acceptance criterion, as a test: three runs of the same shape of card, and
    // the log tells all three apart.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);

    let taken = answer(&cast(&state, &db, "test_may_draw"), &db, true);
    let refused = answer(&cast(&state, &db, "test_may_draw"), &db, false);
    let never_offered = cast(&state, &db, "test_plain_draw");

    assert!(events(&taken).iter().any(
        |event| matches!(event, GameEvent::OptionalApplied { player } if *player == PlayerId(0))
    ));
    assert!(!declined(&taken));

    assert!(declined(&refused));
    assert!(!events(&refused)
        .iter()
        .any(|event| matches!(event, GameEvent::OptionalApplied { .. })));

    assert!(!declined(&never_offered));
    assert!(!events(&never_offered)
        .iter()
        .any(|event| matches!(event, GameEvent::OptionalApplied { .. })));
}

#[test]
fn issue_610_a_card_selection_answer_cannot_answer_a_yes_or_no() {
    // The two answers are not interchangeable: a card selection submitted against a
    // yes-or-no is not a wrong answer, it is an answer to a question nobody asked, and
    // the game is unchanged by it.
    let db = db();
    let mut state = main_phase();
    stock_library(&mut state, &db, 3);
    let owed = cast(&state, &db, "test_may_draw");

    let bogus: Vec<CardInstance> = owed.players[0].library.clone();
    assert_eq!(
        apply_action(
            &owed,
            &Action::AnswerChoice {
                chosen: vec![bogus[0].id],
            },
            &db,
        ),
        owed,
    );
    assert!(sage_engine::pending_player_choice(&owed).is_some());
}
