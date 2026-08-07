//! Per-room **undo**: the table rule, the bounded checkpoint history, and the
//! rollback itself (issue #648).
//!
//! Undo is a *table* rule and therefore entirely the server's. The engine is pure and
//! holds no history at all — a `GameState` is an immutable snapshot, so "the state
//! before that action" is a value the room already had and simply did not keep. That
//! is the whole implementation: the room keeps a bounded stack of the states it was in
//! when it last asked somebody a question, and an undo pops one.
//!
//! Everything the acceptance criteria list — hidden zones, library order, the stack,
//! priority and pass state, mana and payments, counters, the RNG position, the
//! game-over verdict — is restored because *all* of it is `GameState` and a whole
//! `GameState` goes back. There is no field-by-field rollback here, and there must
//! never be one: a rollback that enumerated what to restore would silently stop
//! restoring whatever the engine grew next.
//!
//! # Where a checkpoint falls
//! One checkpoint per **server-accepted transition**: the state as it stood when the
//! room last broadcast a position somebody had to answer, captured immediately before
//! the action that left it. So one undo takes back one action *and* whatever the
//! settle did after it (ADR 0010) — the pair is what a player experienced as a single
//! move, and taking back half of it would land the table in a position nobody ever
//! saw.
//!
//! It follows that a restored checkpoint is **not settled again**. It was already
//! settled when it was made: the settle loop had stopped there, which is why a view was
//! sent from it. Re-running automation over a restored state would fast-forward
//! straight back out of the position the player asked to return to.
//!
//! # What it costs
//! A checkpoint is a full `GameState` clone, so history is bounded
//! ([`DEFAULT_UNDO_LIMIT`]) and the bound is on the wire ([`UndoView::limit`]) — a
//! client that could not see it would offer a rollback the room cannot perform.
//! Nothing is cloned at all at a table that did not enable undo, which is every table
//! by default.

use sage_engine::PlayerId;
use sage_protocol::UndoView;
use tracing::{info, warn};

use super::*;

/// How many checkpoints a room keeps by default: enough to walk back through a
/// misclick and the few moves around it, and far short of the whole game.
///
/// The number is a memory bound, not a rules statement — each checkpoint is a whole
/// `GameState` — which is exactly why the client is told what it is rather than left
/// to assume history is unlimited.
pub(crate) const DEFAULT_UNDO_LIMIT: usize = 20;

/// A room's **undo** policy (issue #648): whether this table lets a player take the
/// last accepted transition back, and how far back the room will hold.
///
/// The same shape as [`TimerPolicy`](super::TimerPolicy) and
/// [`AutoPassPolicy`](super::AutoPassPolicy), and off by default for the same reason:
/// an off policy is bit-for-bit the behaviour before undo existed — no clone, no
/// history, no field on the wire — so every existing room, test, and headless game is
/// unchanged. The lobby turns it on for a room whose host asked for it
/// ([`RoomConfig::undo_enabled`](sage_protocol::RoomConfig)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UndoPolicy {
    /// Nothing can be taken back, and no state is retained to take it back to.
    #[default]
    Off,
    /// Any seat may roll the table back to an earlier accepted transition, up to
    /// `limit` of them.
    On {
        /// The most checkpoints held at once; the oldest is dropped past it.
        limit: usize,
    },
}

impl UndoPolicy {
    /// The policy a table configured with `enabled` plays under — the one place the
    /// room's default depth is chosen, so the lobby names a table rule and never a
    /// number.
    #[must_use]
    pub fn allowed(enabled: bool) -> Self {
        if enabled {
            Self::On {
                limit: DEFAULT_UNDO_LIMIT,
            }
        } else {
            Self::Off
        }
    }

    /// How many checkpoints this policy retains, or `0` when it retains none.
    fn limit(self) -> usize {
        match self {
            Self::Off => 0,
            Self::On { limit } => limit,
        }
    }
}

impl Room {
    /// Keep the current state as a restorable checkpoint (issue #648), dropping the
    /// oldest once the policy's limit is reached.
    ///
    /// Called immediately **before** the room applies an accepted transition, so what
    /// is kept is the position the table was actually asked about. A no-op — and, more
    /// to the point, no clone — under [`UndoPolicy::Off`].
    pub(super) fn checkpoint(&mut self) {
        let limit = self.undo.limit();
        if limit == 0 {
            return;
        }
        self.history.push(self.state.clone());
        // Bounded from the far end: the oldest checkpoint is the one nobody is coming
        // back to, and dropping it is the only way the memory a long game holds stays
        // flat rather than growing with the turn count.
        if self.history.len() > limit {
            self.history.remove(0);
        }
    }

    /// What this table's undo can do right now, for the view (issue #648), or `None`
    /// at a table that does not allow undo — the absence a client reads as "draw no
    /// undo control at all".
    pub(super) fn undo_view(&self) -> Option<UndoView> {
        match self.undo {
            UndoPolicy::Off => None,
            UndoPolicy::On { limit } => Some(UndoView {
                available: u32::try_from(self.history.len()).unwrap_or(u32::MAX),
                limit: u32::try_from(limit).unwrap_or(u32::MAX),
            }),
        }
    }

    /// Whether an undo would succeed if one arrived now: the table allows it and at
    /// least one earlier checkpoint survives.
    pub(super) fn undo_available(&self) -> bool {
        self.undo != UndoPolicy::Off && !self.history.is_empty()
    }

    /// Roll the table back one checkpoint at `seat`'s request (issue #648).
    ///
    /// Any seat may ask, at any time, without the others' agreement — that is the rule
    /// the room was configured with, and a table that did not want it did not enable
    /// it. Rejected when the table does not allow undo or nothing earlier survives; a
    /// rejection changes no state and re-sends the asker its current view flagged, the
    /// same non-fatal answer a stale `choose_action` gets.
    ///
    /// On success the *whole* previous state is restored and every newer checkpoint is
    /// already gone — `pop` is the truncation, so play after a rollback simply builds a
    /// new branch on top of the restored one. The restored state is broadcast to
    /// everyone, seats and spectators alike, so no client is left describing a position
    /// the room no longer holds.
    pub(super) fn on_undo(&mut self, seat: Seat) {
        if seat >= self.seats.len() {
            warn!(seat, "undo from a seat that does not exist; ignoring");
            return;
        }
        if self.undo == UndoPolicy::Off {
            warn!(seat, "undo requested at a table that does not allow it");
            self.send_view_flagged(seat, true);
            return;
        }
        let Some(restored) = self.history.pop() else {
            warn!(seat, "undo requested with no earlier checkpoint to restore");
            self.send_view_flagged(seat, true);
            return;
        };
        // The rollback, and the one thing it adds to the state it restores: the log
        // entry saying it happened and who asked. The engine numbers that entry —
        // sequence numbers are its to mint — and everything the undone transition
        // wrote went back with the state that held it.
        self.state = restored.with_undo_recorded(PlayerId(seat));
        // The settle report describes a transition that no longer exists. Clearing it
        // is not tidiness: left alone it would tell a seat it was passed through steps
        // the restored state has not reached.
        for steps in &mut self.auto_passed_steps {
            steps.clear();
        }
        for mark in &mut self.auto_passed_from {
            *mark = None;
        }
        // Deliberately **no** settle here. The restored state is one the settle had
        // already stopped at — a view was sent from it — so running automation over it
        // again would carry the table straight back out of the position it was asked
        // to return to.
        self.arm_deadline();
        info!(
            seat,
            remaining = self.history.len(),
            "undid the last transition"
        );
        self.broadcast();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use sage_protocol::{ChooseAction, ClientMessage, GameLogEvent, GameView};

    use super::*;
    use crate::room::test_support::*;

    /// A room at a table that allows undo, with both seats connected and each brought
    /// current. Returns the handle, the task, and both outboxes.
    async fn undoable_room(
        state: sage_engine::GameState,
    ) -> (
        RoomHandle,
        tokio::task::JoinHandle<()>,
        watch::Receiver<Option<GameView>>,
        watch::Receiver<Option<GameView>>,
    ) {
        let (handle, task) = Room::new(state, db())
            .with_undo(UndoPolicy::allowed(true))
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
        wait_for_view(&mut rx0).await;
        wait_for_view(&mut rx1).await;
        (handle, task, rx0, rx1)
    }

    /// Take the named action from a view, as the client would.
    fn take(view: &GameView, kind: &str) -> ClientMessage {
        let action = view
            .valid_actions
            .iter()
            .find(|a| a.kind == kind)
            .unwrap_or_else(|| panic!("{kind} is on offer"));
        ClientMessage::ChooseAction(ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn issue_648_an_undo_restores_the_whole_previous_state_for_every_seat() {
        // Seat 0 plays a land: it leaves the hand and arrives on the battlefield. One
        // undo puts *both* halves back — the point being that a checkpoint is a whole
        // `GameState`, so a hidden zone returns with the visible one.
        let (handle, task, mut rx0, mut rx1) = undoable_room(dealt_state()).await;
        let before = rx0.borrow_and_update().clone().unwrap();
        let hand_before = before.my_hand.len();
        assert!(before.battlefield.is_empty(), "nothing is in play yet");

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&before, "play_land"),
        });
        let played = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert_eq!(played.my_hand.len(), hand_before - 1, "the land left hand");
        assert_eq!(played.battlefield.len(), 1, "and reached the battlefield");

        // Seat 1 asks for the undo — any player may, without the others' agreement.
        handle.send(RoomInput::Message {
            seat: 1,
            message: ClientMessage::Undo,
        });
        let restored = wait_for_view(&mut rx0).await;
        let restored_other = wait_for_view(&mut rx1).await;
        assert_eq!(restored.my_hand.len(), hand_before, "the card came back");
        assert!(restored.battlefield.is_empty(), "and left the battlefield");
        assert_eq!(
            restored.turn, before.turn,
            "the restored position is the one that was asked about"
        );
        // Every connected client is synchronized to the same restored state.
        assert!(restored_other.battlefield.is_empty());

        // And the log says who did it — the one fact the restored board cannot show.
        let undone = restored
            .log
            .iter()
            .rev()
            .find_map(|entry| match &entry.event {
                GameLogEvent::Undone { player } => Some(player.clone()),
                _ => None,
            })
            .expect("the rollback is recorded");
        assert_eq!(undone, "p1", "the log names the seat that asked");

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_a_reconnect_after_an_undo_lands_on_the_restored_state() {
        // The room is the sole writer of the one state, so a seat that was away for the
        // rollback is brought current with it and never with the position it left. The
        // reconnect is the ordinary one — a fresh outbox and one full view.
        let (handle, task, mut rx0, mut rx1) = undoable_room(dealt_state()).await;
        let opening = rx0.borrow_and_update().clone().unwrap();

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&opening, "play_land"),
        });
        let _ = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;

        // Seat 1 drops, seat 0 takes its own land back, and seat 1 comes back.
        handle.send(RoomInput::Leave { seat: 1 });
        drop(rx1);
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::Undo,
        });
        let _ = wait_for_view(&mut rx0).await;

        let (tx1b, mut rx1b) = view_channel();
        handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1b,
        });
        let resumed = wait_for_view(&mut rx1b).await;
        assert!(
            resumed.battlefield.is_empty(),
            "the reconnect lands on the restored board, not the pre-undo one"
        );
        assert_eq!(resumed.undo.unwrap().available, 0);

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_undos_walk_back_through_history_and_stop_when_it_is_empty() {
        // Two accepted transitions, two checkpoints. Each undo restores one and the
        // count falls; the third request has nothing left and is rejected without
        // changing anything.
        let (handle, task, mut rx0, mut rx1) = undoable_room(dealt_state()).await;
        let opening = rx0.borrow_and_update().clone().unwrap();
        assert_eq!(
            opening.undo.unwrap().available,
            0,
            "nothing has happened yet, so there is nothing to take back"
        );

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&opening, "play_land"),
        });
        let after_land = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert_eq!(after_land.undo.unwrap().available, 1);

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&after_land, "pass_priority"),
        });
        let after_pass = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert_eq!(after_pass.undo.unwrap().available, 2);

        for expected in [1u32, 0] {
            handle.send(RoomInput::Message {
                seat: 0,
                message: ClientMessage::Undo,
            });
            let view = wait_for_view(&mut rx0).await;
            let _ = wait_for_view(&mut rx1).await;
            assert_eq!(view.undo.unwrap().available, expected);
        }

        // Back at the opening position, with the hand it started with.
        let oldest = rx0.borrow_and_update().clone().unwrap();
        assert_eq!(oldest.my_hand.len(), opening.my_hand.len());
        assert!(oldest.battlefield.is_empty());

        // One more request: refused, and the sender is told so on an otherwise
        // unchanged view — the same non-fatal answer a stale action gets.
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::Undo,
        });
        let refused = wait_for_view(&mut rx0).await;
        assert!(refused.action_rejected, "nothing was left to restore");
        assert_eq!(refused.undo.unwrap().available, 0);
        assert_eq!(refused.my_hand.len(), opening.my_hand.len());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_play_after_an_undo_builds_a_new_branch_over_the_restored_state() {
        // The checkpoint an undo restores is *popped*, so everything newer is already
        // gone: the next action starts a fresh branch and the count reflects it rather
        // than the history that was discarded.
        let (handle, task, mut rx0, mut rx1) = undoable_room(dealt_state()).await;
        let opening = rx0.borrow_and_update().clone().unwrap();

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&opening, "play_land"),
        });
        let _ = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;

        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::Undo,
        });
        let restored = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert_eq!(restored.undo.unwrap().available, 0);

        // A different move from the restored position: play continues normally, and the
        // one checkpoint held is this branch's, not the abandoned one's.
        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&restored, "pass_priority"),
        });
        let branched = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert_eq!(branched.undo.unwrap().available, 1);
        assert!(
            branched.battlefield.is_empty(),
            "the undone land stayed undone"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_a_table_without_undo_offers_none_and_refuses_the_request() {
        // The default room: no field on the view at all, so a client draws no control,
        // and a request that arrives anyway changes nothing.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let opening = wait_for_view(&mut rx0).await;
        assert!(opening.undo.is_none(), "undo is off unless a table asks");

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&opening, "play_land"),
        });
        let played = wait_for_view(&mut rx0).await;
        assert!(played.undo.is_none());

        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::Undo,
        });
        let refused = wait_for_view(&mut rx0).await;
        assert!(refused.action_rejected, "the table does not allow undo");
        assert_eq!(
            refused.battlefield.len(),
            1,
            "and nothing was rolled back regardless"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_history_is_bounded_and_the_bound_rides_the_view() {
        // A room holding at most two checkpoints keeps exactly two however long the
        // game runs, and says so — a client that could not see the bound would offer a
        // rollback the room cannot perform.
        let (handle, task) = Room::new(dealt_state(), db())
            .with_undo(UndoPolicy::On { limit: 2 })
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
        wait_for_view(&mut rx0).await;
        wait_for_view(&mut rx1).await;

        let mut latest = rx0.borrow_and_update().clone().unwrap();
        for _ in 0..4 {
            let actor = if latest.valid_actions.is_empty() {
                1
            } else {
                0
            };
            let view = if actor == 0 {
                latest.clone()
            } else {
                rx1.borrow_and_update().clone().unwrap()
            };
            handle.send(RoomInput::Message {
                seat: actor,
                message: take(&view, "pass_priority"),
            });
            latest = wait_for_view(&mut rx0).await;
            let _ = wait_for_view(&mut rx1).await;
        }
        let undo = latest.undo.expect("an undo table states its availability");
        assert_eq!(undo.limit, 2, "the bound is on the wire");
        assert_eq!(undo.available, 2, "and history never grows past it");

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_undoing_the_move_that_ended_the_game_returns_it_to_play() {
        // Seat 0's pass runs the state-based action that kills the 0-life opponent, so
        // the game ends. The room broadcasts the terminal view and — because a
        // checkpoint survives and a seat is still connected — keeps serving: the undo
        // puts the table back to a live position.
        let (handle, task, mut rx0, mut rx1) = undoable_room(near_terminal_state()).await;
        let opening = rx0.borrow_and_update().clone().unwrap();
        assert!(opening.result.is_none(), "the game is still live");

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&opening, "pass_priority"),
        });
        let over = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert!(over.result.is_some(), "the pass ended the game");
        assert_eq!(
            over.undo.unwrap().available,
            1,
            "the transition that ended it is a checkpoint like any other"
        );

        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::Undo,
        });
        let revived = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert!(revived.result.is_none(), "the game is live again");
        assert!(
            !revived.valid_actions.is_empty(),
            "and the seat that acted can act again"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_648_a_finished_game_ends_the_room_once_the_last_seat_leaves() {
        // The hold-open is bounded by what it is for: somebody who could still ask.
        // A finished table whose players have all gone has nobody left to undo for, so
        // the room stops on its own exactly as it always did — the alternative is a
        // task per finished game that never ends.
        let (handle, task, mut rx0, mut rx1) = undoable_room(near_terminal_state()).await;
        let opening = rx0.borrow_and_update().clone().unwrap();

        handle.send(RoomInput::Message {
            seat: 0,
            message: take(&opening, "pass_priority"),
        });
        let over = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        assert!(over.result.is_some(), "the pass ended the game");

        // Still alive while a seat is connected and a checkpoint survives — that is
        // the whole point of holding it open.
        handle.send(RoomInput::Leave { seat: 0 });
        let _ = wait_for_view(&mut rx1).await;
        assert!(
            handle.is_active(),
            "seat 1 is still here and could still undo"
        );

        // The last player leaves: nothing can reopen the game, so the task returns.
        handle.send(RoomInput::Leave { seat: 1 });
        task.await.expect("the room task ends on its own");
        drop(handle);
    }
}
