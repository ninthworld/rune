//! The room task's lifecycle and driver loop: [`Room::spawn`], the [`Room::run`]
//! message loop, terminal-state detection, and the decision-clock arming/expiry
//! (issue #263). These are additional `impl Room` blocks; the struct and its
//! constructors live in the module root. Pure code motion out of the room module
//! root (issue #427) — no behavior change.

use rune_engine::apply_action;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::info;

use super::policy::timeout_default_action;
use super::*;

impl Room {
    /// Spawn the room on a Tokio task, returning a [`RoomHandle`] for delivering
    /// inputs and the [`JoinHandle`] of the task. The task runs until every sender
    /// half of its input channel has been dropped.
    pub fn spawn(self) -> (RoomHandle, JoinHandle<()>) {
        let (inbox_tx, inbox_rx) = mpsc::channel(ROOM_INBOX_CAPACITY);
        let handle = tokio::spawn(self.run(inbox_rx));
        (RoomHandle { inbox: inbox_tx }, handle)
    }

    /// The room's message loop. Exposed for tests and embedders that want to drive
    /// a room on their own task; most callers use [`Room::spawn`].
    ///
    /// The loop ends — and the task returns — when either the input channel closes
    /// (every [`RoomHandle`] dropped) or the game reaches a terminal state (a player
    /// has lost). On game over it pushes one final broadcast so every connected seat
    /// sees the finished board, then stops; the lobby reclaims the room afterward
    /// (issue #54).
    pub async fn run(mut self, mut inbox: mpsc::Receiver<RoomInput>) {
        // Fast-forward any idle opening priority (a no-op when automation is off),
        // then start the clock on whatever decision actually rests (issue #264).
        self.settle_auto_passes();
        self.arm_deadline();
        while !self.game_over() {
            // Copy the deadline out so the timer future borrows nothing of `self`
            // (the input arm needs `&mut self`). A `None` deadline parks forever, so
            // the timer arm simply never fires when no clock is running.
            let deadline = self.deadline;
            tokio::select! {
                maybe = inbox.recv() => {
                    let Some(input) = maybe else {
                        info!("room input channel closed; room task stopping");
                        return;
                    };
                    match input {
                        RoomInput::Join { seat, outbox } => self.on_join(seat, outbox),
                        RoomInput::Message { seat, message } => self.on_message(seat, &message),
                        RoomInput::Leave { seat } => self.on_leave(seat),
                        RoomInput::JoinSpectator { outbox } => self.on_join_spectator(outbox),
                    }
                }
                () = async move {
                    match deadline {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    self.on_timeout();
                }
            }
        }
        // A player has lost: the game is over. Push the terminal state to every
        // connected seat as the final broadcast, then stop — nothing further can
        // happen, and the lobby will reclaim the room (issue #54).
        self.broadcast();
        info!("game reached a terminal state; room task stopping after final broadcast");
    }

    /// Whether the game has reached a terminal state (CR 104.2a).
    ///
    /// Delegates entirely to the engine's [`GameState::is_over`], which decides
    /// terminality from the losing conditions it models; the room only reads that
    /// verdict and never decides a loss itself, keeping all game logic in the
    /// engine. On a terminal state the room stops advancing and keeps broadcasting
    /// the final view (see [`Room::run`]).
    pub(super) fn game_over(&self) -> bool {
        self.state.is_over()
    }

    /// (Re)arm the decision clock for the state the room now presents (issue #263).
    ///
    /// Sets an absolute deadline `limit` from now when a timer policy is active and a
    /// decision is actually pending; clears it otherwise. Called after every applied
    /// action (a fresh decision) and at room start — but never on join/leave, so a
    /// reconnect observes the real remaining time rather than restarting the clock.
    pub(super) fn arm_deadline(&mut self) {
        self.deadline = match self.timer {
            TimerPolicy::PerDecision { limit } if self.decision_pending() => {
                Some(Instant::now() + limit)
            }
            _ => None,
        };
    }

    /// Whether a live in-game decision is pending — the game is not over and some
    /// seat holds priority (and so has actions to take). The clock only runs while
    /// this holds.
    fn decision_pending(&self) -> bool {
        !self.game_over() && self.state.priority_holder().is_some()
    }

    /// The decision clock expired (issue #263): apply the conservative default action
    /// on the deciding player's behalf, then restart the clock for the next decision.
    ///
    /// If there is no safe default (mulligan/discard) or the default would be a
    /// no-op, the clock is left cleared for this decision rather than hot-looping —
    /// the decision then waits for the player (or a future idle-escalation policy).
    fn on_timeout(&mut self) {
        if let Some(action) = timeout_default_action(&self.state, &self.db) {
            let next = apply_action(&self.state, &action, &self.db);
            if next != self.state {
                self.state = next;
                // The default action can leave idle priority behind, exactly as a
                // player's action does; settle it before re-arming (a no-op when
                // automation is off).
                self.settle_auto_passes();
                self.arm_deadline();
                if !self.game_over() {
                    self.broadcast();
                }
                return;
            }
        }
        // Nothing safe to do automatically: stop timing this decision.
        self.deadline = None;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::time::Duration;

    use rune_protocol::{ChooseAction, ClientMessage};

    use super::*;
    use crate::room::test_support::*;

    #[tokio::test]
    async fn issue_54_room_stops_after_the_game_reaches_a_terminal_state() {
        let (handle, task) = Room::new(near_terminal_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let opening = wait_for_view(&mut rx0).await;

        // Seat 0 holds priority. Passing runs state-based actions, which mark the
        // 0-life opponent as lost: the game becomes terminal.
        let pass = opening
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to the priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                ..Default::default()
            }),
        });

        // The room pushes exactly one final broadcast, then shuts down: seat 0
        // receives the terminal view and its outbox then closes.
        let _final_view = wait_for_view(&mut rx0).await;
        assert!(
            rx0.changed().await.is_err(),
            "the room's outbox should close once the game is over",
        );

        // The task terminates on its own, without any handle being dropped.
        task.await
            .expect("room task should terminate after game over");
    }

    #[tokio::test]
    async fn issue_119_final_broadcast_carries_the_game_result() {
        // On game over the room's final broadcast carries the terminal result, so a
        // client learns the winner and reason from the last view (CR 104.2a). While
        // the game is live the result is absent.
        let (handle, task) = Room::new(near_terminal_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let opening = wait_for_view(&mut rx0).await;
        assert!(opening.result.is_none(), "a live game carries no result");

        let pass = opening
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to the priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                ..Default::default()
            }),
        });

        // The single pass runs the SBA that marks the 0-life opponent lost; the
        // final broadcast then carries the terminal result.
        let final_view = wait_for_view(&mut rx0).await;
        let result = final_view
            .result
            .expect("the final broadcast carries the game result");
        assert_eq!(result.winner.as_deref(), Some("p0"));
        assert_eq!(result.reason, rune_protocol::GameOverReason::LifeZero);
        assert!(
            final_view.valid_actions.is_empty(),
            "a terminal view offers no actions"
        );

        task.await
            .expect("room task should terminate after game over");
    }

    // ----- Decision timers (issue #263) -----

    #[tokio::test]
    async fn issue_263_timers_off_by_default_leave_no_deadline() {
        // The default policy reproduces the pre-timer behavior: no `action_deadline`
        // rides the view, so existing flows are unchanged.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        assert!(!view0.valid_actions.is_empty(), "seat 0 is the decider");
        assert!(
            view0.action_deadline.is_none(),
            "no clock runs under the default (off) policy"
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn issue_263_timer_projects_a_deadline_that_survives_reconnect() {
        // With a per-decision clock, the deciding seat's view carries the seconds
        // remaining. The deadline is absolute, so a reconnect midway through sees the
        // reduced remaining time rather than a fresh clock.
        let policy = TimerPolicy::PerDecision {
            limit: Duration::from_secs(30),
        };
        let (handle, task) = Room::new(dealt_state(), db())
            .with_timer_policy(policy)
            .spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let view0 = wait_for_view(&mut rx0).await;
        let first = view0.action_deadline.expect("the decider is on the clock");
        assert!(
            (29.0..=30.0).contains(&first),
            "roughly the full limit remains at the start: {first}"
        );

        // Ten seconds pass without an action, then seat 0 reconnects. The re-sent
        // view reflects ~20s left — the clock did not restart.
        tokio::time::advance(Duration::from_secs(10)).await;
        let (tx0b, mut rx0b) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0b,
        });
        let reconnect = wait_for_view(&mut rx0b).await;
        let remaining = reconnect.action_deadline.expect("still on the clock");
        assert!(
            (19.0..=21.0).contains(&remaining),
            "reconnect keeps the absolute deadline, ~20s left: {remaining}"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn issue_263_timer_expiry_takes_the_default_action() {
        // Nobody acts; when the clock runs out the room takes the safe default
        // (pass priority) on the deciding seat's behalf. Here that pass runs
        // state-based actions that finish the game (player 1 was at 0 life), so the
        // effect is observable as the terminal result on the final broadcast — and
        // the auto-advancing paused clock has a terminal state to stop at.
        let policy = TimerPolicy::PerDecision {
            limit: Duration::from_secs(30),
        };
        let (handle, task) = Room::new(near_terminal_dealt_state(), db())
            .with_timer_policy(policy)
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
        let view0 = wait_for_view(&mut rx0).await;
        let _view1 = wait_for_view(&mut rx1).await;
        assert!(
            view0.action_deadline.is_some(),
            "seat 0 is on the clock and no one has acted"
        );

        // The paused runtime auto-advances to the deadline; the default pass fires,
        // the game ends, and the room's final broadcast carries the result.
        let terminal = wait_for_view(&mut rx0).await;
        assert!(
            terminal.result.is_some(),
            "the timed-out default action drove the game to a terminal state"
        );
        // The task stops on its own once terminal.
        task.await.unwrap();
        drop(handle);
    }
}
