//! Tests for the **human pacing contract** (issue #455): the room's default-stop
//! policy, the own-turn half of a seat's stop preference, and the per-seat record of
//! where a settle acted on that seat's behalf.
//!
//! A sibling of [`super::tests`] rather than more of it: that module is ADR 0010's
//! automation coverage (does the settle move at all, and does it ever move past a
//! real decision), and this one is the opposite question — where the settle must
//! *stop*, and what it owes the player about the ground it covered. Splitting them
//! also keeps both under the `docs/coding-standards.md` file-size ceiling.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::Step;
use sage_protocol::{ChooseAction, Phase, SetStops};

use super::*;
use crate::room::test_support::*;
use crate::test_support::fixture;

/// Spawn a room on `state` with automation on and `policy` seeding default stops,
/// join `seat`, and return its first view — the one the start-of-room settle comes
/// to rest on. The #455 sibling of [`resting_view`].
async fn paced_resting_view(
    state: GameState,
    policy: StopPolicy,
    seat: Seat,
) -> (GameView, RoomHandle, tokio::task::JoinHandle<()>) {
    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .with_stop_policy(policy)
        .spawn();
    let (tx, mut rx) = view_channel();
    handle.send(RoomInput::Join { seat, outbox: tx });
    let view = wait_for_view(&mut rx).await;
    (view, handle, task)
}

/// A view's `auto_passed_steps` as `(phase, turn)` pairs — the shape these tests
/// reason about, since the point of the field is that neither half answers alone.
fn path(view: &GameView) -> Vec<(Phase, u32)> {
    view.auto_passed_steps
        .iter()
        .map(|step| (step.phase, step.turn))
        .collect()
}

#[tokio::test]
async fn issue_455_a_human_seat_stops_at_its_own_main_phase_by_default() {
    // The acceptance criterion. On a board where seat 0 can do nothing at all, ADR
    // 0020's settle used to carry it through its whole turn between two broadcasts —
    // the playtest complaint #455 records. With the human default seeded, the settle
    // comes to rest at seat 0's own precombat main and hands it the turn.
    let (view, handle, task) =
        paced_resting_view(spell_less_state(), StopPolicy::HumanMainPhases, 0).await;
    assert_eq!(
        (view.turn, view.phase),
        (1, Phase::PrecombatMain),
        "the settle stops at the seat's own precombat main"
    );
    assert!(
        view.valid_actions.iter().any(|a| a.kind == "pass_priority"),
        "the seat is handed priority there — it advances its own main phase"
    );
    assert_eq!(
        view.own_turn_stops,
        vec![Phase::PrecombatMain, Phase::PostcombatMain],
        "the effective default rides the view, so the stops UI draws what the room \
         actually honours"
    );
    assert!(
        view.stops.is_empty(),
        "the default is the own-turn half only; nothing stops on an opponent's turn"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_the_same_board_without_the_policy_still_fast_forwards() {
    // The control. Identical board, identical automation, no default-stop policy:
    // exactly ADR 0010's behaviour, so `StopPolicy::None` is a true no-op and every
    // room that never opts in is unchanged.
    let (view, handle, task) = paced_resting_view(spell_less_state(), StopPolicy::None, 0).await;
    assert!(
        !(view.turn == 1 && view.phase == Phase::PrecombatMain),
        "with no policy the idle main phase is auto-passed, not rested on: got turn \
         {} at {:?}",
        view.turn,
        view.phase
    );
    assert!(
        view.own_turn_stops.is_empty() && view.stops.is_empty(),
        "no policy seeds no stops"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_an_own_turn_stop_does_not_fire_on_an_opponents_turn() {
    // What makes the default a *main-phase* stop rather than a per-step click tax.
    // Seat 1 asks to stop at the precombat main; the game is parked in seat 0's
    // precombat main. The own-turn list must not fire there — and the any-turn list,
    // the only difference between these two rooms, must.
    let mut state = spell_less_state();
    state.step = Step::PrecombatMain;

    let (own, own_handle, own_task) = {
        let (handle, task) = Room::new(state.clone(), db())
            .with_auto_pass(AutoPassPolicy::On)
            .with_own_turn_stops(vec![vec![], vec![Phase::PrecombatMain]])
            .spawn();
        let (tx, mut rx) = view_channel();
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx,
        });
        (wait_for_view(&mut rx).await, handle, task)
    };
    assert!(
        !(own.turn == 1 && own.phase == Phase::PrecombatMain && !own.valid_actions.is_empty()),
        "seat 1's own-turn stop is silent during seat 0's precombat main"
    );
    drop(own_handle);
    own_task.await.unwrap();

    let (any, any_handle, any_task) = {
        let (handle, task) = Room::new(state, db())
            .with_auto_pass(AutoPassPolicy::On)
            .with_stops(vec![vec![], vec![Phase::PrecombatMain]])
            .spawn();
        let (tx, mut rx) = view_channel();
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx,
        });
        (wait_for_view(&mut rx).await, handle, task)
    };
    assert_eq!(
        (any.turn, any.phase),
        (1, Phase::PrecombatMain),
        "the any-turn stop is ADR 0010's original escape hatch and still fires on an \
         opponent's turn"
    );
    assert!(
        any.valid_actions.iter().any(|a| a.kind == "pass_priority"),
        "the any-turn stop hands seat 1 priority"
    );
    drop(any_handle);
    any_task.await.unwrap();
}

#[tokio::test]
async fn issue_455_ai_seats_are_never_seeded_so_throughput_is_unchanged() {
    // The throughput guarantee. The same policy over an all-AI table seeds nothing,
    // so an AI-only game settles exactly as far as it did before default stops
    // existed — the seed keys off the room's existing `ai_seats` knowledge.
    let (handle, task) = Room::new(spell_less_state(), db())
        .with_auto_pass(AutoPassPolicy::On)
        .with_ai_seats(vec![true, true])
        .with_stop_policy(StopPolicy::HumanMainPhases)
        .spawn();
    let (tx, mut rx) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx,
    });
    let view = wait_for_view(&mut rx).await;
    assert!(
        view.own_turn_stops.is_empty(),
        "an AI seat is seeded with nothing"
    );
    assert!(
        !(view.turn == 1 && view.phase == Phase::PrecombatMain),
        "so the settle fast-forwards the idle main phase exactly as before"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_a_seat_can_clear_the_default_stops_and_they_stay_cleared() {
    // The default is a starting value, not a rule. One `set_stops` with both lists
    // empty replaces it, the settle resumes ADR 0010's pacing, and the cleared
    // preference survives reconnect — the room never re-seeds a seat that has spoken.
    let mut state = spell_less_state();
    state.step = Step::Upkeep;
    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .with_stop_policy(StopPolicy::HumanMainPhases)
        .spawn();
    let (tx, mut rx) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx,
    });
    let seeded = wait_for_view(&mut rx).await;
    assert_eq!(
        seeded.phase,
        Phase::PrecombatMain,
        "the seed stopped it here"
    );

    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::SetStops(SetStops::default()),
    });
    let cleared = wait_for_view(&mut rx).await;
    assert!(
        cleared.own_turn_stops.is_empty() && cleared.stops.is_empty(),
        "an empty set_stops means stop nowhere, not give me the defaults back"
    );
    assert!(
        cleared.phase != Phase::PrecombatMain || cleared.turn != 1,
        "and the settle carried straight on past the main phase it had stopped at"
    );

    // Reconnect: the cleared preference is what comes back, not a fresh seed.
    handle.send(RoomInput::Leave { seat: 0 });
    let (tx2, mut rx2) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx2,
    });
    let resumed = wait_for_view(&mut rx2).await;
    assert!(
        resumed.own_turn_stops.is_empty(),
        "the room never re-seeds a seat that has expressed a preference"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_a_custom_stop_set_replaces_the_seed_on_both_halves() {
    // Customisation, the other half of the criterion: a seat may move its stops
    // anywhere. A step claimed on both lists is echoed back on the wider `stops`
    // only, so the client draws one state per step rather than two.
    let (handle, task) = Room::new(dealt_state(), db())
        .with_stop_policy(StopPolicy::HumanMainPhases)
        .spawn();
    let (tx, mut rx) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx,
    });
    let _ = wait_for_view(&mut rx).await;

    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::SetStops(SetStops {
            stops: vec![Phase::End, Phase::End],
            own_turn: vec![Phase::Upkeep, Phase::End],
        }),
    });
    let after = wait_for_view(&mut rx).await;
    assert_eq!(after.stops, vec![Phase::End], "duplicates collapse");
    assert_eq!(
        after.own_turn_stops,
        vec![Phase::Upkeep],
        "a step already claimed on every turn is not also claimed on your own"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_auto_passed_steps_name_where_the_settle_skipped_the_seat() {
    // The pacing contract's second half: the settle reports *where* it acted, not
    // just that it did. ADR 0010 shipped one boolean for a whole settle, which is
    // exactly enough to say "you were skipped" and not enough to say what you
    // missed. Seat 0 is idle from its untap step; the list names the steps it was
    // carried through, and stops short of the one it comes to rest at.
    let mut state = spell_less_state();
    state.step = Step::Untap;
    let (view, handle, task) = paced_resting_view(state, StopPolicy::HumanMainPhases, 0).await;

    assert_eq!(view.phase, Phase::PrecombatMain);
    assert!(view.auto_passed, "the settle did act for this seat");
    assert_eq!(
        path(&view),
        vec![(Phase::Untap, 1), (Phase::Upkeep, 1), (Phase::Draw, 1),],
        "each position it was carried through, once, in order, each stating its own \
         turn — and never the step it was handed priority at"
    );
    assert_eq!(
        view.auto_passed,
        !view.auto_passed_steps.is_empty(),
        "the boolean is exactly the list's summary"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_a_settle_that_crosses_a_turn_keeps_both_visits_and_states_the_turn() {
    // The cross-turn case the path exists for. Seat 1 can never act, so once seat 0
    // is out of the way the settle runs it through the rest of seat 0's turn, the
    // whole of its own, and into the next — visiting several steps twice.
    //
    // The two visits must both survive, and each must say which turn it belonged to:
    // a client cannot read "End appears twice" as "the turn changed", because an
    // extra combat phase (CR 506.1) repeats a step *inside* one turn. Only the server
    // knows which happened, and this is where it says so.
    let mut state = spell_less_state();
    state.step = Step::Upkeep;
    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        // Seat 0 stops at its own main phases (the human default); seat 1 stops
        // nowhere, so it is the seat the settle carries across the boundary.
        .with_stop_policy(StopPolicy::HumanMainPhases)
        .with_ai_seats(vec![false, true])
        .spawn();
    let (tx0, mut rx0) = view_channel();
    let (tx1, mut rx1) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let opening = wait_for_view(&mut rx0).await;
    let _ = wait_for_view(&mut rx1).await;

    // Seat 0 passes its way out of both of its own main phases; the settle then has
    // nothing to stop for until seat 0's main phase comes round again next turn.
    let mut view = opening;
    for _ in 0..8usize {
        let Some(pass) = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
        else {
            break;
        };
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                ..Default::default()
            }),
        });
        view = wait_for_view(&mut rx0).await;
        if view.turn > 1 {
            break;
        }
    }

    let seat1 = rx1.borrow_and_update().clone().expect("a pushed view");
    let crossed = path(&seat1);
    assert!(
        crossed.iter().any(|(_, turn)| *turn == 1) && crossed.iter().any(|(_, turn)| *turn == 2),
        "the settle carried seat 1 across the turn boundary: {crossed:?}"
    );
    let repeated = crossed
        .iter()
        .filter(|(phase, _)| *phase == Phase::End)
        .count();
    assert!(
        repeated >= 2,
        "and both visits to the end step survive as separate entries: {crossed:?}"
    );
    // The turns are what tell the two visits apart. Nothing in the phase sequence
    // alone could, which is the whole reason the field carries a turn.
    let end_turns: Vec<u32> = crossed
        .iter()
        .filter(|(phase, _)| *phase == Phase::End)
        .map(|(_, turn)| *turn)
        .collect();
    assert!(
        end_turns.windows(2).any(|w| w[0] != w[1]),
        "the repeated end step is stated as belonging to different turns: {end_turns:?}"
    );
    // Turns never go backwards along the path.
    assert!(
        crossed.windows(2).all(|w| w[0].1 <= w[1].1),
        "the path is ordered: {crossed:?}"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_455_a_resolved_removal_spell_and_its_death_reach_the_skipped_seat() {
    // The comprehension criterion: a spell the player had no answer to resolves
    // *inside* one settle, and the broadcast that follows must still carry the fact.
    // Seat 1's Shock is on the stack aimed at seat 0's Llanowar Elves; seat 0 holds
    // priority with an empty hand and no mana, so the room passes for it, the spell
    // resolves, and the creature dies — all between two views. The log window (ADR
    // 0021) is what makes that recoverable, and `auto_passed_steps` says where the
    // seat's own part in it was taken.
    //
    // It happens in seat 1's **upkeep** deliberately: that is a step no default stop
    // covers, so this is the genuine "no response, no stop, no broadcast" case the
    // criterion names. (Aimed at seat 0's own main phase the default stop would have
    // held the game there, which is the other half of the fix and is tested above.)
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.active_player = PlayerId(1);
    state.step = Step::Upkeep;
    for seat in 0..2 {
        state.players[seat].library = (0..8)
            .map(|_| state.new_instance(fixture("onakke_ogre")))
            .collect();
    }
    // Seat 0's creature, and nothing in hand to save it with.
    let elves_card = fixture("llanowar_elves");
    let elves_inst = state.new_instance(elves_card);
    let elves = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: elves,
        instance: elves_inst.id,
        printed: elves_card.into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
    });
    // Seat 1's Shock, already cast and aimed, with seat 0 holding priority over it.
    let shock = state.new_instance(fixture("shock"));
    let sid = sage_engine::StackId(state.mint_id());
    state.stack.push(sage_engine::StackObject {
        id: sid,
        controller: PlayerId(1),
        kind: sage_engine::StackObjectKind::Spell {
            card: shock,
            mode: None,
            x: None,
        },
        targets: vec![sage_engine::Target::Permanent(elves)],
    });
    state.priority = PlayerId(0);

    let (view, handle, task) = paced_resting_view(state, StopPolicy::HumanMainPhases, 0).await;

    assert!(
        !view
            .battlefield
            .iter()
            .any(|p| p.card.name == "Llanowar Elves"),
        "the creature is gone — the removal resolved during the settle"
    );
    let events: Vec<&sage_protocol::GameLogEvent> = view.log.iter().map(|e| &e.event).collect();
    assert!(
        events.iter().any(|e| matches!(
            e,
            sage_protocol::GameLogEvent::SpellResolved { card, .. } if card.name == "Shock"
        )),
        "the resolution is on the record the view carries: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            sage_protocol::GameLogEvent::PermanentDied { permanent } if permanent.name == "Llanowar Elves"
        )),
        "and so is the death it caused: {events:?}"
    );
    assert!(
        path(&view).contains(&(Phase::Upkeep, 2)),
        "the seat is told its priority over that spell was passed for it, and where: \
         {:?}",
        view.auto_passed_steps
    );

    drop(handle);
    task.await.unwrap();
}

/// The report a passed seat is owed covers **the action that passed it**, not only the
/// passes themselves (issue #644).
///
/// The reported symptom: a spell was cast on the opponent's turn, resolved, and killed a
/// creature — all inside one settle — and the player's next view showed a dead creature
/// with no explanation. `auto_passed_steps` named the steps, which is no help: a player
/// does not recognise a turn by its step list. What they need is the *events*, and the
/// log already carries them; what the log cannot say on its own is which of them this
/// seat missed, because that depends on when the seat was last shown anything.
///
/// So the mark is where the seat's unattended stretch began, and it must start at the
/// opponent's action — a mark taken at the settle's first pass would begin one event too
/// late and omit the cast, which is the exact event being explained.
#[tokio::test]
async fn issue_644_the_settle_mark_covers_the_action_that_triggered_it() {
    let mut state = spell_less_state();
    state.step = Step::Upkeep;

    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();

    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let view0 = wait_for_view(&mut rx0).await;
    let (tx1, mut rx1) = view_channel();
    handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let view1 = wait_for_view(&mut rx1).await;

    // Whichever seat the settle came to rest on is the one that acts; the other is the
    // one it will pass. Read rather than assumed, because where a spell-less board comes
    // to rest is the settle's business and not this test's.
    let (acting, mut watcher_rx, actor) = if view0.valid_actions.is_empty() {
        (view1, rx0, 1)
    } else {
        (view0, rx1, 0)
    };
    let watcher = 1 - actor;

    // The second join broadcasts to every seat, so the watcher has an unread view sitting
    // in its channel. Mark it seen, or the wait below returns *it* rather than the one the
    // action produces.
    watcher_rx.borrow_and_update();

    // Everything after this is something the watching seat has not been shown.
    let last_seen = acting.log.last().map(|entry| entry.sequence).unwrap_or(0);

    handle.send(RoomInput::Message {
        seat: actor,
        message: ClientMessage::ChooseAction(forced_move(&acting)),
    });
    let after = wait_for_view(&mut watcher_rx).await;
    let _ = watcher;

    assert!(
        after.auto_passed,
        "the watching seat holds nothing castable, so the settle acted for it"
    );
    let from = after
        .auto_passed_from
        .expect("a seat that was passed is told where its unattended stretch began");
    assert_eq!(
        from,
        last_seen + 1,
        "the mark is the first log entry recorded since the watching seat's last view — \
         which is the acting seat's action, not the settle's first pass"
    );
    assert!(
        after.log.iter().any(|entry| entry.sequence >= from),
        "and the window still carries the entries it points at, so there is a report to give"
    );

    drop(handle);
    task.await.unwrap();
}

/// No settle for this seat, no mark: the field is strictly in step with the step list
/// beside it, so a client never reports an unattended stretch that did not happen.
#[tokio::test]
async fn issue_644_a_seat_that_was_not_passed_is_given_no_mark() {
    // Automation off, so nothing is ever passed and nothing is ever marked.
    let (handle, task) = Room::new(spell_less_state(), db())
        .with_auto_pass(AutoPassPolicy::Off)
        .spawn();
    let (tx, mut rx) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx,
    });
    let idle = wait_for_view(&mut rx).await;

    assert!(!idle.auto_passed);
    assert_eq!(idle.auto_passed_from, None);

    drop(handle);
    task.await.unwrap();
}

/// The mark and the step list are one signal: whenever a seat is told it was passed, it
/// is also told where to read about it.
#[tokio::test]
async fn issue_644_the_mark_and_the_step_list_agree() {
    let (view, handle, task) = paced_resting_view(spell_less_state(), StopPolicy::None, 0).await;
    assert_eq!(
        view.auto_passed,
        view.auto_passed_from.is_some(),
        "a passed seat always has a mark, and an unpassed one never does"
    );
    drop(handle);
    task.await.unwrap();
}
