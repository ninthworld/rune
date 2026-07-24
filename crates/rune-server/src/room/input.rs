//! Client-message routing and priority automation: applying a chosen action or
//! rejecting it, recording stop preferences, and the auto-pass settle loop (issue
//! #264, ADR 0020). These are additional `impl Room` blocks over the struct defined
//! in the module root. Pure code motion out of the room module root (issue #427) —
//! no behavior change.

use rune_engine::{apply_action, priority_has_no_meaningful_action, Action, PlayerId};
use rune_protocol::{ClientMessage, SetStops};
use tracing::warn;

use crate::view::{phase_of, resolve_action};

use super::*;

/// A hard cap on how many priority passes one settle may apply, a defence against a
/// pathological stop configuration that never reaches a meaningful decision. The
/// loop terminates naturally far below this every turn (the active player's
/// declare-attackers step is a forced choice that offers no pass), so hitting the
/// cap signals a bug; it is logged and the settle stops rather than hanging the task.
const MAX_AUTO_PASSES: usize = 256;

impl Room {
    /// Route a client message. A chosen action the engine offered this seat is
    /// applied and every connected seat is re-broadcast its view; anything else is
    /// rejected and the sender is simply re-sent its current view (full-state
    /// resync), never mutating the game.
    pub(super) fn on_message(&mut self, seat: Seat, message: &ClientMessage) {
        match message {
            ClientMessage::ChooseAction(choose) => {
                match resolve_action(&self.state, &self.db, PlayerId(seat), choose) {
                    Some(action) => {
                        self.state = apply_action(&self.state, &action, &self.db);
                        // Auto-pass any idle priority the action left behind (a no-op
                        // when automation is off), then restart the clock for whatever
                        // decision now rests (a no-op when timers are off).
                        self.settle_auto_passes();
                        self.arm_deadline();
                        // A terminal result is delivered once by the run loop's final
                        // broadcast; don't re-send the same full-state view here.
                        if !self.game_over() {
                            self.broadcast();
                        }
                    }
                    None => {
                        warn!(
                            seat,
                            action_id = %choose.action_id,
                            "rejected action id not offered to this seat"
                        );
                        // Re-send the unchanged view flagged as a rejection (issue #265)
                        // so the client can show a brief, non-blaming "the game moved on"
                        // notice. With a `valid_actions`-driven client this is a rare
                        // stale-view race, not a user error.
                        self.send_view_flagged(seat, true);
                    }
                }
            }
            ClientMessage::SetStops(set) => self.on_set_stops(seat, set),
        }
    }

    /// Record a seat's priority-stop preferences (issue #264, ADR 0020) and reflect
    /// them back. The preferences are held on the room, like the display name, so
    /// they survive reconnect; a stops change can make the current priority holder
    /// newly eligible to auto-pass (they cleared a stop), so a settle runs, and the
    /// clock is re-armed only if that settle actually advanced the game.
    fn on_set_stops(&mut self, seat: Seat, set: &SetStops) {
        let Some(slot) = self.stops.get_mut(seat) else {
            warn!(seat, "set_stops for a seat that does not exist; ignoring");
            return;
        };
        // Replace the seat's set wholesale, de-duplicated so the reflected list is
        // canonical (a client that sends the same phase twice sees it once back).
        let mut stops = set.stops.clone();
        stops.dedup();
        *slot = stops;
        let advanced = self.settle_auto_passes();
        if advanced {
            self.arm_deadline();
        }
        if !self.game_over() {
            self.broadcast();
        }
    }

    /// Auto-pass the priority holder while it is idle and has not opted to stop at the
    /// current step (issue #264, ADR 0020). Returns whether any pass was applied.
    ///
    /// A no-op unless [`AutoPassPolicy::On`]. Each iteration passes priority for
    /// whichever seat currently holds it — the engine's own `PassPriority`, so the
    /// resulting state is identical to a manual pass and determinism is preserved.
    /// The loop stops the instant a seat has a meaningful action, owes a forced choice
    /// (a window with no pass on offer — e.g. the active player's declare-attackers),
    /// or has opted to stop; a fixed [`MAX_AUTO_PASSES`] cap is a defensive backstop
    /// so a pathological configuration can never hang the task.
    pub(super) fn settle_auto_passes(&mut self) -> bool {
        for flag in &mut self.auto_passed_seats {
            *flag = false;
        }
        if self.auto_pass != AutoPassPolicy::On {
            return false;
        }
        let mut advanced = false;
        let mut passes = 0usize;
        loop {
            if self.game_over() || self.state.priority_holder().is_none() {
                break;
            }
            let seat = self.state.priority.0;
            if !self.should_auto_pass(seat) {
                break;
            }
            if passes >= MAX_AUTO_PASSES {
                // Still idle after the cap: a stop configuration that never rests. Log
                // it and stop; the game waits for a human rather than the task spinning.
                warn!("auto-pass settle hit its cap without reaching a decision; stopping");
                break;
            }
            let next = apply_action(&self.state, &Action::PassPriority, &self.db);
            // Defensive: a pass that does not change state would loop forever.
            if next == self.state {
                break;
            }
            self.state = next;
            if let Some(flag) = self.auto_passed_seats.get_mut(seat) {
                *flag = true;
            }
            advanced = true;
            passes += 1;
        }
        advanced
    }

    /// Whether `seat`, which currently holds priority, should be auto-passed: the
    /// engine reports it has no meaningful action **and** the seat has not opted to
    /// stop at the current step (issue #264). The engine predicate is the rules
    /// authority (the client may not make this call); the stop set is the seat's
    /// opt-in escape hatch.
    fn should_auto_pass(&self, seat: Seat) -> bool {
        if !priority_has_no_meaningful_action(&self.state, &self.db) {
            return false;
        }
        let here = phase_of(self.state.step);
        !self
            .stops
            .get(seat)
            .is_some_and(|stops| stops.contains(&here))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use rune_engine::Step;
    use rune_protocol::{ChooseAction, Phase, SetStops};

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
        assert_eq!(after_round.phase, rune_protocol::Phase::Upkeep);

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

    // ----- Basic priority automation (issue #264, ADR 0020) -----

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
        state.players[1].mana_pool.add(rune_engine::Color::Blue, 3);
        // Something on the stack for the counterspell to legally target.
        let boar = state.new_instance(fixture("onakke_ogre"));
        let sid = rune_engine::StackId(state.mint_id());
        state.stack.push(rune_engine::StackObject {
            id: sid,
            controller: PlayerId(0),
            kind: rune_engine::StackObjectKind::Spell { card: boar },
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
        // The display-only indicator: reaching the first forced decision auto-passes
        // seat 0 through the early idle steps, so its resting view is flagged.
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
            "seat 0 was auto-passed to reach its first forced decision (indicator set)"
        );
        // It rests on the forced attacker declaration (no pass on offer there).
        assert!(
            view0
                .valid_actions
                .iter()
                .any(|a| a.kind == "declare_attackers"),
            "the settle halted at the active player's forced combat declaration"
        );
        drop(handle);
        task.await.unwrap();
    }
}
