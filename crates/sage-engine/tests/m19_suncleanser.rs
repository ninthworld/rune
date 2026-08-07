//! Suncleanser (issue #706): a **mode chosen as a trigger goes on the stack**, and a
//! prohibition on counters.
//!
//! Modes existed only for a cast, where the choice is part of the announcement
//! (CR 601.2b) and rides on the action that puts the spell on the stack. A trigger has no
//! announcement — the game puts it there — so CR 603.3c gives it the same moment by
//! another road: the ability arrives unanswered, its controller answers before anyone
//! receives priority, and the answer is recorded on the stack object.
//!
//! That is exactly the road a trigger's **targets** already travelled (CR 603.3d), which
//! is why the mode travels it too rather than getting a mechanism of its own. The two are
//! one answer and have to be: the mode is what says how many target slots there are and
//! what each may aim at, so asking them in sequence would leave the first answer invisible
//! to the second.
//!
//! And the choice being made on the stack rather than on resolution is observable, not
//! pedantic — an opponent responding to this trigger can see which of the two it is doing.
//!
//! The prohibition is the other half. `It can't have counters put on it` is a continuous
//! effect (CR 611.2b) that modifies one rule (CR 614.1b), and it is asked at the single
//! seam every road to a counter runs through — so it stops a counter from a spell, from an
//! activated ability, and from a state-based action alike, without any of them knowing it
//! exists.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, Color, CounterKind, FunctionalId,
    GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
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

/// Cast Suncleanser from seat 0's hand and stop at the trigger's question.
fn cast_suncleanser(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let card = state.new_instance(cid(db, "suncleanser"));
    state.players[0].hand.push(card);
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    for _ in 0..8 {
        if aiming_offers(&state, db).is_empty() && !state.stack.is_empty() {
            state = apply_action(&state, &Action::PassPriority, db);
        } else {
            break;
        }
    }
    state
}

/// Every `ChooseTriggerTargets` the game is currently offering.
fn aiming_offers(state: &GameState, db: &CardDatabase) -> Vec<Action> {
    valid_actions(state, db)
        .into_iter()
        .filter(|action| matches!(action, Action::ChooseTriggerTargets { .. }))
        .collect()
}

/// Pass priority until the stack empties.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..12 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

fn counters(state: &GameState, id: PermanentId, kind: CounterKind) -> u32 {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .and_then(|perm| perm.counters.get(&kind).copied())
        .unwrap_or(0)
}

/// **The crux.** The trigger is offered as *two* actions, one per mode — the choice is
/// made here, on the stack, and not on resolution.
#[test]
fn issue_706_a_modal_trigger_offers_one_action_per_mode() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "bogstomper", PlayerId(0));

    let state = cast_suncleanser(&state, &db);
    let offers = aiming_offers(&state, &db);

    assert_eq!(offers.len(), 2, "one offer per mode");
    let modes: Vec<Option<u8>> = offers
        .iter()
        .map(|action| match action {
            Action::ChooseTriggerTargets { mode, .. } => *mode,
            _ => None,
        })
        .collect();
    assert_eq!(
        modes,
        vec![Some(0), Some(1)],
        "and each names the mode it would take"
    );
    assert!(
        !state.stack.is_empty(),
        "the ability is on the stack while the question is asked (CR 603.3c)"
    );
}

/// The first mode: a creature's counters are removed, and it may not be given more while
/// the Cleric stands.
#[test]
fn issue_706_the_first_mode_strips_a_creature_and_forbids_more() {
    let db = db();
    let mut state = main_phase();
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == bear) {
        perm.counters.insert(CounterKind::PlusOnePlusOne, 3);
    }

    let state = cast_suncleanser(&state, &db);
    let state = apply_action(
        &state,
        &aiming_offers(&state, &db)
            .into_iter()
            .find(|action| matches!(action, Action::ChooseTriggerTargets { mode: Some(0), .. }))
            .map(|action| match action {
                Action::ChooseTriggerTargets { ability, mode, .. } => {
                    Action::ChooseTriggerTargets {
                        ability,
                        mode,
                        targets: vec![Target::Permanent(bear)],
                    }
                }
                other => other,
            })
            .expect("the first mode is on offer"),
        &db,
    );
    let state = settle(&state, &db);

    assert_eq!(
        counters(&state, bear, CounterKind::PlusOnePlusOne),
        0,
        "the counters it had are gone"
    );

    // And a fresh counter cannot be put on it. Asked at the seam itself rather than
    // through some card that happens to place one: the prohibition *lives* at the seam,
    // so this is the honest test of it and not a way around it — every effect that puts a
    // counter on a permanent gets this answer.
    let mut state = state;
    assert!(
        !state.put_counters_on_permanent(bear, CounterKind::PlusOnePlusOne, 1, &db),
        "and no more may be put on it"
    );
    assert_eq!(
        counters(&state, bear, CounterKind::PlusOnePlusOne),
        0,
        "so it still has none"
    );
}

/// The prohibition ends with its source: a Suncleanser that leaves the battlefield stops
/// forbidding anything, with nothing to clear.
#[test]
fn issue_706_the_prohibition_ends_when_the_cleric_leaves() {
    let db = db();
    let mut state = main_phase();
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));

    let state = cast_suncleanser(&state, &db);
    let state = apply_action(
        &state,
        &aiming_offers(&state, &db)
            .into_iter()
            .find(|action| matches!(action, Action::ChooseTriggerTargets { mode: Some(0), .. }))
            .map(|action| match action {
                Action::ChooseTriggerTargets { ability, mode, .. } => {
                    Action::ChooseTriggerTargets {
                        ability,
                        mode,
                        targets: vec![Target::Permanent(bear)],
                    }
                }
                other => other,
            })
            .expect("the first mode is on offer"),
        &db,
    );
    let mut state = settle(&state, &db);

    assert!(
        !state.put_counters_on_permanent(bear, CounterKind::PlusOnePlusOne, 1, &db),
        "forbidden while the Cleric stands"
    );

    // Remove the Cleric. The prohibition is keyed to it, so it is gone on the next read
    // and the state-based-action loop prunes the effect itself.
    let cleric = state
        .battlefield
        .iter()
        .find(|perm| {
            perm.printed.card() == Some(cid(&db, "suncleanser")) && perm.controller == PlayerId(0)
        })
        .map(|perm| perm.id)
        .expect("the Cleric is on the battlefield");
    state.battlefield.retain(|perm| perm.id != cleric);
    // One action, so the state-based-action loop runs and prunes the effect whose source
    // is gone. `settle` would do nothing here — the stack is already empty.
    let mut state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state.put_counters_on_permanent(bear, CounterKind::PlusOnePlusOne, 1, &db),
        "and allowed once it is gone"
    );
}

/// The second mode: an opponent loses their counters and may get no more. Nothing in the
/// catalog gives a player a counter, so this is correct and inert — which the test states
/// rather than hides, by putting one on by hand.
#[test]
fn issue_706_the_second_mode_strips_a_player_and_forbids_more() {
    let db = db();
    let mut state = main_phase();
    state.players[1].counters.insert(CounterKind::Poison, 4);

    let state = cast_suncleanser(&state, &db);
    let state = apply_action(
        &state,
        &aiming_offers(&state, &db)
            .into_iter()
            .find(|action| matches!(action, Action::ChooseTriggerTargets { mode: Some(1), .. }))
            .map(|action| match action {
                Action::ChooseTriggerTargets { ability, mode, .. } => {
                    Action::ChooseTriggerTargets {
                        ability,
                        mode,
                        targets: vec![Target::Player(PlayerId(1))],
                    }
                }
                other => other,
            })
            .expect("the second mode is on offer"),
        &db,
    );
    let mut state = settle(&state, &db);

    assert!(
        state.players[1].counters.is_empty(),
        "the opponent's counters are gone"
    );
    assert!(
        !state.put_counters_on_player(PlayerId(1), CounterKind::Poison, 1),
        "and they may get no more"
    );
    assert!(
        state.put_counters_on_player(PlayerId(0), CounterKind::Poison, 1),
        "which is a fact about that one seat and nobody else"
    );
}

/// The mode decides the slots: mode 0 aims at a creature and mode 1 at a player, and an
/// answer aimed the other way is refused.
#[test]
fn issue_706_each_mode_declares_its_own_target_slot() {
    let db = db();
    let mut state = main_phase();
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));

    let state = cast_suncleanser(&state, &db);
    let Some(Action::ChooseTriggerTargets { ability, .. }) =
        aiming_offers(&state, &db).first().cloned()
    else {
        panic!("the trigger is waiting to be answered");
    };

    // Mode 0 aimed at a player, which is not what it names.
    let wrong = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: Some(0),
            targets: vec![Target::Player(PlayerId(1))],
        },
        &db,
    );
    assert_eq!(wrong, state, "a creature slot does not take a player");

    // Mode 1 aimed at a creature, likewise.
    let also_wrong = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: Some(1),
            targets: vec![Target::Permanent(bear)],
        },
        &db,
    );
    assert_eq!(
        also_wrong, state,
        "and a player slot does not take a creature"
    );
}
