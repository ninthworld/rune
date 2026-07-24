//! Seat/spectator connection plumbing and personalized-view fan-out: joining,
//! leaving, and pushing each connected seat its own [`GameView`] (with the room's
//! per-seat name/stops/deadline overlays) and every spectator the redacted
//! [`SpectatorView`]. These are additional `impl Room` blocks over the struct defined
//! in the module root. Pure code motion out of the room module root (issue #427) — no
//! behavior change.

use rune_engine::PlayerId;
use tracing::{info, warn};

use crate::view::{personalized_view, spectator_view};

use super::*;

impl Room {
    /// The public display-name map for a `GameView` (issue #294): every seat that has
    /// a name, keyed by its `p{N}` player id. Empty when no seat is named, so the field
    /// elides from the wire and older-server behavior is preserved.
    fn player_names_map(&self) -> std::collections::BTreeMap<String, String> {
        self.player_names
            .iter()
            .enumerate()
            .filter_map(|(seat, name)| name.as_ref().map(|n| (format!("p{seat}"), n.clone())))
            .collect()
    }

    /// Seat (or re-seat) a connection and bring it current with a full view.
    pub(super) fn on_join(&mut self, seat: Seat, outbox: watch::Sender<Option<GameView>>) {
        let Some(slot) = self.seats.get_mut(seat) else {
            warn!(seat, "join for a seat that does not exist; ignoring");
            return;
        };
        *slot = Some(outbox);
        self.send_view(seat);
    }

    /// Hold a disconnected seat open without disturbing the game.
    pub(super) fn on_leave(&mut self, seat: Seat) {
        if let Some(slot) = self.seats.get_mut(seat) {
            *slot = None;
            info!(seat, "seat disconnected; held open for reconnect");
        }
    }

    /// Attach a spectator (ADR 0022, issue #351) and bring it current with a single
    /// redacted [`SpectatorView`] — the whole public board, so a mid-game spectator
    /// reconstructs its UI with no history. A spectator owns no seat and never mutates
    /// the game; a dead spectator sender is pruned lazily on the next broadcast.
    pub(super) fn on_join_spectator(&mut self, outbox: watch::Sender<Option<SpectatorView>>) {
        let mut view = spectator_view(&self.state, &self.db);
        view.player_names = self.player_names_map();
        // If the receiver is already gone, don't retain the sender.
        if outbox.send(Some(view)).is_ok() {
            self.spectators.push(outbox);
        }
    }

    /// Push the current redacted [`SpectatorView`] to every connected spectator,
    /// pruning any whose receiver has been dropped (the spectator disconnected). A
    /// no-op when there are no spectators, so a seated-only room is unaffected.
    fn broadcast_spectators(&mut self) {
        if self.spectators.is_empty() {
            return;
        }
        let mut view = spectator_view(&self.state, &self.db);
        view.player_names = self.player_names_map();
        self.spectators
            .retain(|outbox| outbox.send(Some(view.clone())).is_ok());
    }

    /// Push the seat's freshly-personalized view to its outbox. Writing to the
    /// latest-value [`watch`] never blocks and overwrites any view the reader has
    /// not yet consumed (coalescing to newest). If the receiver is gone, treat it as
    /// a disconnect and hold the seat open.
    ///
    /// When a decision clock is running (issue #263), the deciding seat's view — the
    /// one with actions on offer — carries `action_deadline` as the seconds remaining
    /// until the default action fires, computed from the absolute deadline so a
    /// reconnect sees the true remaining time.
    fn send_view(&mut self, seat: Seat) {
        self.send_view_flagged(seat, false);
    }

    /// Send `seat` its personalized view, flagging it as the response to a **rejected
    /// action** when `action_rejected` (issue #265). Only the rejection re-send in
    /// [`Self::on_message`] passes `true`; every other push (normal broadcast, join
    /// resync) goes through [`Self::send_view`] with `false`, so the transient
    /// "the game moved on" notice fires once and is never resurrected by a later resync.
    pub(super) fn send_view_flagged(&mut self, seat: Seat, action_rejected: bool) {
        let mut view = personalized_view(&self.state, &self.db, PlayerId(seat));
        // Names are a lobby/session concern, not engine state, so the room labels
        // players here rather than in the pure projection shim (issue #294).
        view.player_names = self.player_names_map();
        // Priority-stop preferences and the auto-pass indicator are likewise room
        // state, not engine state, and per-viewer (issue #264): reflect this seat's
        // stops so its stops UI is reconstructable, and flag whether reaching this
        // state auto-passed it.
        view.stops = self.stops.get(seat).cloned().unwrap_or_default();
        view.auto_passed = self.auto_passed_seats.get(seat).copied().unwrap_or(false);
        // Rejected-action feedback (issue #265): the only caller that sets this is the
        // rejection re-send, and the game state is unchanged, so this rides an otherwise
        // ordinary resync — advisory presentation, never load-bearing.
        view.action_rejected = action_rejected;
        if let Some(at) = self.deadline {
            if !view.valid_actions.is_empty() {
                view.action_deadline =
                    Some(at.saturating_duration_since(Instant::now()).as_secs_f64());
            }
        }
        let Some(slot) = self.seats.get_mut(seat) else {
            return;
        };
        let Some(outbox) = slot.as_ref() else {
            return;
        };
        if outbox.send(Some(view)).is_err() {
            *slot = None;
        }
    }

    /// Send every connected seat its own personalized view, and every spectator the
    /// current redacted view. Seated traffic is exactly as before; the spectator
    /// fan-out is a no-op when there are no spectators (ADR 0022, issue #351).
    pub(super) fn broadcast(&mut self) {
        for seat in 0..self.seats.len() {
            let connected = self.seats.get(seat).map(Option::is_some).unwrap_or(false);
            if connected {
                self.send_view(seat);
            }
        }
        self.broadcast_spectators();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use rune_protocol::{ChooseAction, ClientMessage};

    use super::*;
    use crate::room::test_support::*;

    #[tokio::test]
    async fn join_sends_each_seat_a_personalized_view_hiding_opponents_hands() {
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        let (tx1, mut rx1) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        assert!(handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1
        }));

        // Give the room task a chance to process both joins.
        let view0 = wait_for_view(&mut rx0).await;
        let view1 = wait_for_view(&mut rx1).await;

        // Each seat's view names its own receiver in `you`.
        assert_eq!(view0.you, "p0");
        assert_eq!(view1.you, "p1");

        // Player 0 sees their own two cards but only a count for player 1's hand.
        assert_eq!(view0.my_hand.len(), 2);
        assert_eq!(view0.opponents.len(), 1);
        assert_eq!(view0.opponents[0].hand_size, 1);
        // The opponent view carries no card contents at all.
        assert_eq!(view0.opponents[0].library_size, 2);

        // Player 1 symmetrically sees only their own single card.
        assert_eq!(view1.my_hand.len(), 1);
        assert_eq!(view1.opponents[0].hand_size, 2);
        assert_eq!(view1.opponents[0].library_size, 1);

        // Only the priority holder (seat 0) is offered actions.
        assert!(!view0.valid_actions.is_empty());
        assert!(view1.valid_actions.is_empty());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn a_spectator_joins_mid_game_and_receives_a_redacted_view() {
        // A seated game underway; a spectator attaches and immediately reconstructs the
        // whole public board from one SpectatorView — every seat as public counts, no
        // hand contents, and it keeps updating as the game advances (issue #351).
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let seat0_view = wait_for_view(&mut rx0).await;
        // Grab seat 0's pass action now (its view will not change on a spectator join).
        let action = seat0_view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
            .expect("a pass is offered to the priority holder");

        // A spectator attaches after the game is underway.
        let (stx, mut srx) = watch::channel::<Option<SpectatorView>>(None);
        assert!(handle.send(RoomInput::JoinSpectator { outbox: stx }));
        let spec = wait_for_spectator_view(&mut srx).await;

        // Every seat appears as a public OpponentView with only counts, no hand cards.
        assert_eq!(spec.players.len(), 2);
        assert_eq!(spec.players[0].hand_size, 2);
        assert_eq!(spec.players[1].hand_size, 1);
        // The public board is fully present (reconstruct-from-one-message).
        let json = serde_json::to_value(&spec).unwrap();
        assert!(json.get("valid_actions").is_none());
        assert!(json.get("my_hand").is_none());
        assert!(json.get("you").is_none());
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: action.id,
                token: action.token,
                targets: vec![],
            }),
        }));
        let updated = wait_for_spectator_view(&mut srx).await;
        // Still redacted, still every seat public — the update is a full public snapshot.
        assert_eq!(updated.players.len(), 2);

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn a_room_with_no_spectators_broadcasts_exactly_as_before() {
        // Zero-spectator rooms do the seated work unchanged: the spectator fan-out is a
        // no-op, so a seated pass round is byte-for-byte the two-player behavior.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view0 = wait_for_view(&mut rx0).await;
        assert_eq!(view0.you, "p0");
        assert!(!view0.valid_actions.is_empty());
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn reconnect_is_brought_current_with_a_full_view() {
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0,
        });
        let _ = wait_for_view(&mut rx0).await;

        // Disconnect: the seat is held open, the game is untouched.
        handle.send(RoomInput::Leave { seat: 0 });

        // Reconnect with a fresh outbox: the room re-sends the latest full view.
        let (tx0b, mut rx0b) = view_channel();
        handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0b,
        });
        let resumed = wait_for_view(&mut rx0b).await;
        assert_eq!(resumed.my_hand.len(), 2);
        assert!(!resumed.valid_actions.is_empty());

        drop(handle);
        task.await.unwrap();
    }

    /// A slow reader that pauses while the game advances must, on resuming, observe
    /// the *latest* view — intermediate superseded views are coalesced away and the
    /// outbox never accumulates a backlog. Exercises the per-seat `watch` outbox.
    #[tokio::test]
    async fn issue_57_slow_reader_coalesces_to_the_latest_view() {
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

        // Seat 0 reads its opening view (holds priority), then becomes a *slow
        // reader*: it stops draining rx0 for the rest of the exchange. Seat 1 stays
        // responsive and doubles as our synchronization barrier.
        let opening0 = wait_for_view(&mut rx0).await;
        let _ = wait_for_view(&mut rx1).await;
        let pass0 = opening0
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass offered to the priority holder");
        handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass0.id.clone(),
                ..Default::default()
            }),
        });

        // Seat 0 now pauses. Seat 1 receives priority and passes in turn; this pushes
        // *two* fresh views to the paused seat 0 (first "lost priority", then
        // "regained priority after the step advanced").
        let mut after0_seat1 = wait_for_view(&mut rx1).await;
        while after0_seat1.priority_player.as_deref() != Some("p1") {
            after0_seat1 = wait_for_view(&mut rx1).await;
        }
        let pass1 = after0_seat1
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("priority handed to seat 1");
        handle.send(RoomInput::Message {
            seat: 1,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass1.id.clone(),
                ..Default::default()
            }),
        });

        // Barrier: wait until seat 1 observes priority returning to p0. By then the
        // room has already written the latest view to seat 0's (paused) outbox too.
        let mut seat1_latest = wait_for_view(&mut rx1).await;
        while seat1_latest.priority_player.as_deref() != Some("p0") {
            seat1_latest = wait_for_view(&mut rx1).await;
        }

        // Seat 0 *resumes*. It must skip the intermediate "lost priority" snapshot
        // and read exactly the newest state (priority back to p0). If the outbox had
        // queued views, the first read here would be the stale no-priority view.
        let resumed0 = wait_for_view(&mut rx0).await;
        assert_eq!(resumed0.priority_player.as_deref(), Some("p0"));
        assert!(
            !resumed0.valid_actions.is_empty(),
            "coalesced view is the latest, in which seat 0 holds priority again",
        );
        // Bounded depth: a single latest value, no backlog left to drain.
        assert!(
            !rx0.has_changed().unwrap(),
            "the outbox coalesces to one latest view, never a queue of superseded ones",
        );

        drop(handle);
        task.await.unwrap();
    }
}
