//! Tests for message routing and the priority-automation settle loop. Split out of
//! `input.rs` when the automation coverage for issue #453 pushed that file past the
//! file-size ceiling — pure code motion, following the `actions.rs`/`actions/tests.rs`
//! precedent in the engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::Step;
use sage_protocol::{ChooseAction, Phase, SetStops};

use super::*;
use crate::room::test_support::*;
use crate::test_support::fixture;

#[tokio::test]
async fn two_players_advance_a_round_of_pass_priority() {
    let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
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
    let initial0 = wait_for_view(&mut rx0).await;
    let _ = wait_for_view(&mut rx1).await;

    // Seat 0 holds priority: choose its "pass" action by the offered id.
    let pass0 = initial0
        .valid_actions
        .iter()
        .find(|a| a.kind == "pass_priority")
        .expect("pass offered to priority holder");
    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: pass0.id.clone(),
            ..Default::default()
        }),
    });

    // After seat 0 passes, priority moves to seat 1, who is now offered a pass.
    let after0_seat1 = wait_for_view(&mut rx1).await;
    let pass1 = after0_seat1
        .valid_actions
        .iter()
        .find(|a| a.kind == "pass_priority")
        .expect("priority handed to seat 1");
    assert_eq!(after0_seat1.priority_player.as_deref(), Some("p1"));
    handle.send(RoomInput::Message {
        seat: 1,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: pass1.id.clone(),
            ..Default::default()
        }),
    });

    // Both passed: the step advances and priority returns to the active player.
    // Seat 0 was broadcast a view after each pass; drain to the end-of-round
    // one (priority back to p0).
    let mut after_round = wait_for_view(&mut rx0).await;
    while after_round.priority_player.as_deref() != Some("p0") {
        after_round = wait_for_view(&mut rx0).await;
    }
    assert_eq!(after_round.phase, sage_protocol::Phase::Upkeep);

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn unknown_action_id_is_rejected_and_state_is_resent_unchanged() {
    let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let before = wait_for_view(&mut rx0).await;

    // A nonsense id is not among the offered actions: rejected.
    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: "does-not-exist".to_string(),
            ..Default::default()
        }),
    });
    let resent = wait_for_view(&mut rx0).await;
    // The rejection re-sends the identical view — the game did not advance.
    assert_eq!(resent.phase, before.phase);
    assert_eq!(resent.priority_player, before.priority_player);
    assert_eq!(resent.valid_actions, before.valid_actions);
    // …but it is flagged as a rejection so the client can surface the transient
    // "the game moved on" notice (issue #265). The initial view was not flagged.
    assert!(!before.action_rejected);
    assert!(resent.action_rejected);

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn action_from_a_seat_without_priority_is_rejected() {
    let (handle, task) = Room::new(GameState::new_two_player(), db()).spawn();
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
    let _ = wait_for_view(&mut rx0).await;
    let _ = wait_for_view(&mut rx1).await;

    // Seat 1 does not hold priority; even "a0" (a real id for seat 0) is not an
    // action offered to seat 1, so it is rejected and seat 1 is resynced.
    handle.send(RoomInput::Message {
        seat: 1,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: "a0".to_string(),
            ..Default::default()
        }),
    });
    let resent = wait_for_view(&mut rx1).await;
    assert!(resent.valid_actions.is_empty());
    // The resync is flagged as a rejection for the sending seat (issue #265).
    assert!(resent.action_rejected);
    // Seat 0 was never re-broadcast because nothing changed: its latest-value
    // outbox holds no view newer than the one already observed.
    assert!(!rx0.has_changed().unwrap());

    drop(handle);
    task.await.unwrap();
}

// ----- Basic priority automation (issue #264, ADR 0010) -----

#[tokio::test]
async fn issue_264_automation_off_by_default_elides_stops_and_indicator() {
    // The default policy changes nothing on the wire: no stops, never auto-passed.
    let (handle, task) = Room::new(dealt_state(), db()).spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let view0 = wait_for_view(&mut rx0).await;
    assert!(view0.stops.is_empty(), "no stops by default");
    assert!(
        !view0.auto_passed,
        "nothing is auto-passed under the off policy"
    );
    assert!(
        view0
            .valid_actions
            .iter()
            .any(|a| a.kind == "pass_priority"),
        "the seat still gets a manual pass with automation off"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_264_auto_pass_dramatically_reduces_manual_passes_on_a_spell_less_turn() {
    // Acceptance: with default stops, a spell-less turn requires dramatically fewer
    // manual passes. Drive the identical spell-less turn twice — automation off vs
    // on — and count the clicks each cost.
    let (off_handle, off_task) = Room::new(spell_less_state(), db()).spawn();
    let (tx0, mut off0) = view_channel();
    let (tx1, mut off1) = view_channel();
    off_handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    off_handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let off_clicks = count_clicks_until_turn(&off_handle, &mut off0, &mut off1, 2).await;
    drop(off_handle);
    off_task.await.unwrap();

    let (on_handle, on_task) = Room::new(spell_less_state(), db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx0, mut on0) = view_channel();
    let (tx1, mut on1) = view_channel();
    on_handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    on_handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let on_clicks = count_clicks_until_turn(&on_handle, &mut on0, &mut on1, 2).await;
    drop(on_handle);
    on_task.await.unwrap();

    assert!(
        off_clicks >= 8,
        "the manual baseline spends many passes on a spell-less turn: {off_clicks}"
    );
    assert!(
        on_clicks * 3 < off_clicks,
        "automation makes a spell-less turn dramatically cheaper: on={on_clicks} off={off_clicks}"
    );
}

#[tokio::test]
async fn issue_264_stop_preferences_survive_reconnect() {
    // Preferences set over the wire are held on the room, so a disconnect/reconnect
    // re-sends them in full — they never live only in client memory.
    let (handle, task) = Room::new(dealt_state(), db()).spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let _ = wait_for_view(&mut rx0).await;

    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::SetStops(SetStops {
            stops: vec![Phase::Upkeep, Phase::End],
            ..Default::default()
        }),
    });
    let after = wait_for_view(&mut rx0).await;
    assert_eq!(after.stops, vec![Phase::Upkeep, Phase::End]);

    // Disconnect and reconnect with a fresh outbox: the stops come back in full.
    handle.send(RoomInput::Leave { seat: 0 });
    let (tx0b, mut rx0b) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0b,
    });
    let resumed = wait_for_view(&mut rx0b).await;
    assert_eq!(
        resumed.stops,
        vec![Phase::Upkeep, Phase::End],
        "stop preferences survive reconnect"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_264_a_relevant_stop_keeps_priority_at_an_idle_step() {
    // A seat that has opted to stop at a step still receives priority there even
    // when idle — the escape hatch from an auto-pass chain. Seat 0 is idle at its
    // postcombat main; with a stop there it is handed priority rather than passed.
    let mut state = spell_less_state();
    state.step = Step::PostcombatMain;
    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .with_stops(vec![vec![Phase::PostcombatMain], vec![]])
        .spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let view0 = wait_for_view(&mut rx0).await;
    assert_eq!(
        view0.phase,
        Phase::PostcombatMain,
        "the stop halts the settle at the postcombat main"
    );
    assert!(
        view0
            .valid_actions
            .iter()
            .any(|a| a.kind == "pass_priority"),
        "the stopped seat is handed priority (a manual pass), not auto-passed"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_264_without_a_stop_the_same_idle_step_is_auto_passed() {
    // The control for the test above: the same idle postcombat main, no stop, is
    // auto-passed through — the seat never rests there.
    let mut state = spell_less_state();
    state.step = Step::PostcombatMain;
    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    // The settle fast-forwards past the postcombat main to the next forced choice
    // (a combat declaration on the following turn); seat 0's resting view is no
    // longer a pass at its postcombat main.
    let view0 = wait_for_view(&mut rx0).await;
    assert!(
        !(view0.phase == Phase::PostcombatMain
            && view0.turn == 1
            && view0
                .valid_actions
                .iter()
                .any(|a| a.kind == "pass_priority")),
        "with no stop the idle postcombat main is auto-passed, not rested on"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_264_a_castable_instant_is_never_auto_passed() {
    // Safety: a seat with an instant-speed play always keeps priority, even with
    // automation on and no stop. Seat 1 holds an affordable instant on seat 0's
    // turn; the engine reports it non-idle, so the room never passes for it.
    let mut state = GameState::new_two_player();
    state.step = Step::Upkeep; // seat 0's turn, seat 1 may respond at instant speed
    let bolt = state.new_instance(fixture("cancel"));
    state.players[1].hand = vec![bolt];
    // `cancel` costs {1}{U}{U}; three blue pays both blue pips and the generic.
    state.players[1].mana_pool.add(sage_engine::Color::Blue, 3);
    // Something on the stack for the counterspell to legally target.
    let boar = state.new_instance(fixture("onakke_ogre"));
    let sid = sage_engine::StackId(state.mint_id());
    state.stack.push(sage_engine::StackObject {
        paid: Default::default(),
        id: sid,
        controller: PlayerId(0),
        kind: sage_engine::StackObjectKind::Spell {
            card: boar,
            mode: None,
            x: None,
        },
        targets: Vec::new(),
    });
    state.priority = PlayerId(1);

    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx1, mut rx1) = view_channel();
    handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let view1 = wait_for_view(&mut rx1).await;
    assert!(
        view1.valid_actions.iter().any(|a| a.kind == "cast_spell"),
        "a seat with a castable instant keeps priority — never auto-passed out of a response"
    );
    assert!(!view1.auto_passed, "the seat was not auto-passed");
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_264_auto_passed_indicator_flags_the_skipped_seat() {
    // The display-only indicator: the settle passes seat 0 through the early idle
    // steps, so its resting view is flagged.
    //
    // Issue #453 changed this test's second half, not what it tests. It used to
    // assert the settle halted on the active player's forced attacker declaration
    // — on this creature-less board that declaration has no legal non-empty
    // answer, so the settle now resolves it as an empty declaration rather than
    // handing a human an empty prompt. The indicator is still what is under test;
    // the resting view is now asserted to be free of that empty prompt.
    let (handle, task) = Room::new(spell_less_state(), db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let view0 = wait_for_view(&mut rx0).await;
    assert!(
        view0.auto_passed,
        "seat 0 was passed through the idle steps (indicator set)"
    );
    assert!(
        !view0
            .valid_actions
            .iter()
            .any(|a| a.kind == "declare_attackers" || a.kind == "declare_blockers"),
        "a declaration with no candidates is never shown to the player"
    );
    drop(handle);
    task.await.unwrap();
}

// ----- Choiceless forced declarations (issue #453) -----

/// A two-player game parked at `step` with seat `priority` holding, on turn 2 so
/// a permanent entering at `entered_turn: 0` is free of summoning sickness.
///
/// Each seat holds a Forest and a library of uncastable creatures. The library
/// keeps the draw step from ending the game (an empty one decks its owner); the
/// Forest gives the active player a meaningful action at its main phases, so a
/// settle that resolves the combat declarations comes naturally to rest there
/// instead of running to [`MAX_AUTO_PASSES`].
fn combat_state(step: Step, priority: usize) -> GameState {
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.step = step;
    state.priority = PlayerId(priority);
    for seat in 0..2 {
        let land = state.new_instance(fixture("forest"));
        state.players[seat].hand = vec![land];
        state.players[seat].library = (0..8)
            .map(|_| state.new_instance(fixture("onakke_ogre")))
            .collect();
    }
    state
}

/// Put an untapped Walking Corpse (a vanilla 2/2) on the battlefield under
/// `controller`, attacking `attacking` if given.
fn creature(
    state: &mut GameState,
    controller: usize,
    attacking: Option<sage_engine::AttackTarget>,
) {
    let card = fixture("walking_corpse");
    let inst = state.new_instance(card);
    let id = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id,
        instance: inst.id,
        printed: card.into(),
        controller: PlayerId(controller),
        tapped: false,
        entered_turn: 0,
        attacking,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
        copied: None,
    });
}

/// Spawn a room on `state` under `policy`, join `seat`, and return its first
/// view — the one the settle at room start comes to rest on.
async fn resting_view(
    state: GameState,
    policy: AutoPassPolicy,
    seat: Seat,
) -> (GameView, RoomHandle, tokio::task::JoinHandle<()>) {
    let (handle, task) = Room::new(state, db()).with_auto_pass(policy).spawn();
    let (tx, mut rx) = view_channel();
    handle.send(RoomInput::Join { seat, outbox: tx });
    let view = wait_for_view(&mut rx).await;
    (view, handle, task)
}

#[tokio::test]
async fn issue_453_a_player_with_no_legal_attackers_is_never_shown_the_prompt() {
    // The reported bug: turn-one board, no creature, and the game still asks the
    // active player to declare attackers. The settle now submits the empty
    // declaration and play continues past the declare steps.
    let (view, handle, task) = resting_view(
        combat_state(Step::DeclareAttackers, 0),
        AutoPassPolicy::On,
        0,
    )
    .await;
    assert!(
        !view
            .valid_actions
            .iter()
            .any(|a| a.kind == "declare_attackers"),
        "the empty attacker declaration is resolved, not prompted"
    );
    assert!(
        view.phase != Phase::DeclareAttackers,
        "play continued past the declare-attackers step"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_453_a_player_with_a_legal_attacker_still_gets_the_forced_prompt() {
    // The safety property: one eligible attacker is a real choice and must reach
    // the player. Never auto-declared past.
    let mut state = combat_state(Step::DeclareAttackers, 0);
    creature(&mut state, 0, None);
    let (view, handle, task) = resting_view(state, AutoPassPolicy::On, 0).await;
    assert_eq!(view.phase, Phase::DeclareAttackers);
    assert!(
        view.valid_actions
            .iter()
            .any(|a| a.kind == "declare_attackers"),
        "a seat with an eligible attacker keeps its forced choice"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_453_a_player_with_no_legal_blockers_is_never_shown_the_prompt() {
    // The defender holds an untapped creature but nothing is attacking them (an
    // empty attacker declaration got the game here), so no block is legal.
    let mut state = combat_state(Step::DeclareBlockers, 1);
    state.attackers_declared = true;
    creature(&mut state, 1, None);
    let (view, handle, task) = resting_view(state, AutoPassPolicy::On, 1).await;
    assert!(
        !view
            .valid_actions
            .iter()
            .any(|a| a.kind == "declare_blockers"),
        "the empty blocker declaration is resolved, not prompted"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_453_a_player_with_a_legal_blocker_still_gets_the_forced_prompt() {
    // The blocker half of the safety property: an attacker attacking this seat
    // and an untapped creature to block with is a real choice.
    let mut state = combat_state(Step::DeclareBlockers, 1);
    state.attackers_declared = true;
    creature(
        &mut state,
        0,
        Some(sage_engine::AttackTarget::Player(PlayerId(1))),
    );
    creature(&mut state, 1, None);
    let (view, handle, task) = resting_view(state, AutoPassPolicy::On, 1).await;
    assert_eq!(view.phase, Phase::DeclareBlockers);
    assert!(
        view.valid_actions
            .iter()
            .any(|a| a.kind == "declare_blockers"),
        "a seat with an eligible blocker keeps its forced choice"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_453_auto_pass_off_still_prompts_the_empty_declaration() {
    // `AutoPassPolicy::Off` reproduces the pre-automation behaviour bit for bit:
    // the same creature-less board still hands the active player the empty
    // declare-attackers prompt.
    let (view, handle, task) = resting_view(
        combat_state(Step::DeclareAttackers, 0),
        AutoPassPolicy::Off,
        0,
    )
    .await;
    assert_eq!(view.phase, Phase::DeclareAttackers);
    assert!(view
        .valid_actions
        .iter()
        .any(|a| a.kind == "declare_attackers"));
    assert!(!view.auto_passed);
    drop(handle);
    task.await.unwrap();
}

/// Drive the room with [`forced_move`] until either seat's latest view reaches
/// `until_turn`, counting how many of the views the driver had to answer were
/// combat-declaration prompts. A view offering `declare_attackers` or
/// `declare_blockers` on a creature-less board is exactly the empty prompt issue
/// #453 is about.
async fn count_declaration_prompts(
    handle: &RoomHandle,
    rx0: &mut watch::Receiver<Option<GameView>>,
    rx1: &mut watch::Receiver<Option<GameView>>,
    until_turn: u32,
) -> usize {
    let mut prompts = 0usize;
    for _ in 0..500usize {
        let v0 = rx0.borrow_and_update().clone();
        let v1 = rx1.borrow_and_update().clone();
        if v0
            .as_ref()
            .or(v1.as_ref())
            .is_some_and(|v| v.turn >= until_turn)
        {
            break;
        }
        let actor = if v0.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
            v0.map(|v| (0usize, v))
        } else if v1.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
            v1.map(|v| (1usize, v))
        } else {
            None
        };
        match actor {
            Some((seat, view)) => {
                prompts += view
                    .valid_actions
                    .iter()
                    .filter(|a| a.kind == "declare_attackers" || a.kind == "declare_blockers")
                    .count();
                handle.send(RoomInput::Message {
                    seat,
                    message: ClientMessage::ChooseAction(forced_move(&view)),
                });
                tokio::select! {
                    _ = rx0.changed() => {}
                    _ = rx1.changed() => {}
                }
            }
            None => {
                tokio::select! {
                    r0 = rx0.changed() => { if r0.is_err() { break; } }
                    r1 = rx1.changed() => { if r1.is_err() { break; } }
                }
            }
        }
    }
    prompts
}

#[tokio::test]
async fn issue_453_a_scripted_turn_shows_no_empty_declaration_prompts() {
    // Counting prompts across the same scripted turn twice: automation off, the
    // creature-less board still charges the players an empty attacker and an
    // empty blocker declaration; automation on, both are gone.
    let (off_handle, off_task) = Room::new(combat_state(Step::Upkeep, 0), db()).spawn();
    let (tx0, mut off0) = view_channel();
    let (tx1, mut off1) = view_channel();
    off_handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    off_handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let off_prompts = count_declaration_prompts(&off_handle, &mut off0, &mut off1, 3).await;
    drop(off_handle);
    off_task.await.unwrap();

    let (on_handle, on_task) = Room::new(combat_state(Step::Upkeep, 0), db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx0, mut on0) = view_channel();
    let (tx1, mut on1) = view_channel();
    on_handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    on_handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let on_prompts = count_declaration_prompts(&on_handle, &mut on0, &mut on1, 3).await;
    drop(on_handle);
    on_task.await.unwrap();

    assert!(
        off_prompts >= 2,
        "the manual baseline still pays for both empty declarations: {off_prompts}"
    );
    assert_eq!(
        on_prompts, 0,
        "automation resolves every choiceless declaration: {on_prompts}"
    );
}

// ----- Floating mana no longer stalls the settle (issue #537) -----

#[tokio::test]
async fn issue_537_a_seat_that_tapped_mana_and_cast_nothing_still_auto_passes() {
    // Regression for the stall behind issue #537. Mana pools were never emptied
    // (CR 500.4), so a seat that tapped a land and cast nothing carried the mana
    // forever; `valid_actions` kept offering the now-affordable spell and
    // `priority_has_no_meaningful_action` kept reporting the seat non-idle, so the
    // room's settle refused to auto-pass it ever again. In a four-player slice
    // that stalled every seat that had ever tapped a land.
    //
    // Revitalize is a {W} instant with no targets, so whether it is castable turns
    // on the mana pool alone — not on timing or on a legal target being present.
    //
    // Issue #453 moved this test's resting point, not its subject. The seat controls
    // only a Plains, so it has no legal attacker at all — the settle now resolves
    // that declaration as an empty one instead of stopping to ask, and carries the
    // seat through combat to its postcombat main. A second Plains in hand is what
    // gives the settle somewhere to come to rest (a land is playable at sorcery
    // speed, so the seat is meaningfully non-idle there); without it the settle would
    // run on into the next turn, which tests nothing about floating mana. The
    // assertions that are #537's actual subject — an empty pool and no phantom
    // `cast_spell` on offer — are unchanged.
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let plains = sage_engine::PermanentId(state.mint_id());
    let land = state.new_instance(fixture("plains"));
    state.battlefield.push(sage_engine::Permanent {
        id: plains,
        instance: land.id,
        printed: fixture("plains").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
        copied: None,
    });
    let second = sage_engine::PermanentId(state.mint_id());
    let second_land = state.new_instance(fixture("plains"));
    state.battlefield.push(sage_engine::Permanent {
        id: second,
        instance: second_land.id,
        printed: fixture("plains").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
        copied: None,
    });
    let heal = state.new_instance(fixture("revitalize"));
    let spare_land = state.new_instance(fixture("plains"));
    state.players[0].hand = vec![heal, spare_land];

    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });

    // Seat 0 is not idle (tapping the Plains would pay for Revitalize), so it is
    // handed priority and offered the land's mana ability.
    let view0 = wait_for_view(&mut rx0).await;
    assert_eq!(view0.phase, Phase::PrecombatMain);
    let tap = view0
        .valid_actions
        .iter()
        .find(|a| a.kind == "activate_ability")
        .expect("the untapped land's mana ability is offered");
    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: tap.id.clone(),
            token: tap.token.clone(),
            ..Default::default()
        }),
    });

    // Tap the second land too: Revitalize costs {1}{W}.
    let after_first = wait_for_view(&mut rx0).await;
    let tap_again = after_first
        .valid_actions
        .iter()
        .find(|a| a.kind == "activate_ability")
        .expect("the second untapped land's mana ability is offered");
    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: tap_again.id.clone(),
            token: tap_again.token.clone(),
            ..Default::default()
        }),
    });

    // The mana is floating and the spell is now genuinely castable.
    let floated = wait_for_view(&mut rx0).await;
    assert_eq!(
        floated.mana_pool,
        vec!["{W}".to_string(), "{W}".to_string()],
        "tapping both Plains floated {{W}}{{W}}"
    );
    let pass = floated
        .valid_actions
        .iter()
        .find(|a| a.kind == "pass_priority")
        .expect("the seat may still pass");
    assert!(
        floated.valid_actions.iter().any(|a| a.kind == "cast_spell"),
        "the floated mana makes Revitalize castable"
    );

    // Seat 0 declines to cast and passes. The step ends, CR 500.4 empties the
    // pool, and the seat is idle again — the settle must carry it forward.
    handle.send(RoomInput::Message {
        seat: 0,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: pass.id.clone(),
            token: pass.token.clone(),
            ..Default::default()
        }),
    });

    let mut resting = wait_for_view(&mut rx0).await;
    for _ in 0..16usize {
        if resting.phase != Phase::PrecombatMain && !resting.valid_actions.is_empty() {
            break;
        }
        resting = wait_for_view(&mut rx0).await;
    }
    assert!(
        resting.mana_pool.is_empty(),
        "the pool emptied when the precombat main phase ended (CR 500.4), got {:?}",
        resting.mana_pool
    );
    assert!(
        !resting.valid_actions.iter().any(|a| a.kind == "cast_spell"),
        "with the pool empty and the land tapped nothing is castable — the seat is \
         not held on a phantom cast"
    );
    assert_eq!(
        resting.phase,
        Phase::PostcombatMain,
        "the settle auto-passed the now-idle seat through combat instead of stalling"
    );
    assert!(
        !resting
            .valid_actions
            .iter()
            .any(|a| a.kind == "declare_attackers" || a.kind == "declare_blockers"),
        "the seat controls no creature, so #453 resolved both declarations rather \
         than resting on an empty prompt"
    );
    assert!(
        resting.valid_actions.iter().any(|a| a.kind == "play_land"),
        "the resting view is a real decision — the land it may still play — not \
         another pass"
    );

    drop(handle);
    task.await.unwrap();
}
