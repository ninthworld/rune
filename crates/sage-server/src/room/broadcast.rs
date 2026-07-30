//! Seat/spectator connection plumbing and personalized-view fan-out: joining,
//! leaving, and pushing each connected seat its own [`GameView`] (with the room's
//! per-seat name/stops/deadline overlays) and every spectator the redacted
//! [`SpectatorView`]. These are additional `impl Room` blocks over the struct defined
//! in the module root. Pure code motion out of the room module root (issue #427) — no
//! behavior change.

use sage_engine::PlayerId;
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

    /// Whether `seat` currently has a live connection (issue #553). A disconnected
    /// seat is *held open* — the game is untouched and the seat is never conceded —
    /// so this is presentation state, not a lifecycle signal.
    fn seat_connected(&self, seat: Seat) -> bool {
        self.seats.get(seat).is_some_and(Option::is_some)
    }

    /// Whether `seat` is played by a server-side AI (issue #553). A seat past the end
    /// of the room's roster is human, matching the all-human default.
    pub(super) fn seat_is_ai(&self, seat: Seat) -> bool {
        self.ai_seats.get(seat).copied().unwrap_or(false)
    }

    /// Overlay the room-owned per-seat presentation state (issue #553) onto an
    /// already-projected seat record: whether that seat is connected, and whether it
    /// is AI-controlled.
    ///
    /// Neither fact is engine state — the engine has no notion of a socket or of an
    /// AI — so [`personalized_view`] leaves both at their "connected human" defaults
    /// and the room, which is the only thing that knows, fills them in here. This is
    /// the same seam `player_names`, `stops`, and `auto_passed` already ride.
    fn overlay_seat_presentation(&self, view: &mut GameView) {
        for (seat, opponent) in view.opponents.iter_mut().enumerate().map(|(i, o)| {
            // `opponents` skips the receiver, so recover each entry's seat index from
            // its own id rather than from its position (issue #553).
            let seat = o.player_id.strip_prefix('p').and_then(|n| n.parse().ok());
            (seat.unwrap_or(i), o)
        }) {
            opponent.connected = self.seat_connected(seat);
            opponent.ai = self.seat_is_ai(seat);
        }
    }

    /// The spectator counterpart of [`Self::overlay_seat_presentation`]: a
    /// [`SpectatorView`] projects **every** seat as an [`OpponentView`], so its seat
    /// index *is* its position, and it carries the same public format signal a seated
    /// view does (issue #553). No private state is added — connection and AI state are
    /// public presentation facts, and the format is advertised in the lobby.
    fn overlay_spectator_presentation(&self, view: &mut SpectatorView) {
        view.format.clone_from(&self.format);
        for (seat, player) in view.players.iter_mut().enumerate() {
            player.connected = self.seat_connected(seat);
            player.ai = self.seat_is_ai(seat);
        }
    }

    /// Seat (or re-seat) a connection and bring it current with a full view.
    ///
    /// Seating changes the table's *public* connection state (issue #553), so this
    /// broadcasts rather than pushing to the joining seat alone: every other seat and
    /// every spectator needs the view in which this seat is connected again. The
    /// joining seat is one of the recipients, so it is still brought current in full.
    pub(super) fn on_join(&mut self, seat: Seat, outbox: watch::Sender<Option<GameView>>) {
        let Some(slot) = self.seats.get_mut(seat) else {
            warn!(seat, "join for a seat that does not exist; ignoring");
            return;
        };
        *slot = Some(outbox);
        // A submission belongs to the connection that sent it (issue #554). A fresh
        // connection is owed no answer to the previous one's traffic, and the client
        // clears its own pending marker on that same discontinuity, so the ack is
        // dropped here rather than re-fired at a client that would ignore it.
        if let Some(ack) = self.pending_acks.get_mut(seat) {
            *ack = None;
        }
        self.broadcast();
    }

    /// Hold a disconnected seat open without disturbing the game.
    ///
    /// The *game* is untouched — that is the whole disconnect policy — but the table's
    /// public connection state changed, so the remaining seats and any spectators are
    /// re-sent a view in which this seat reads as disconnected (issue #553). Without
    /// that push they would keep rendering it as present until some unrelated action
    /// happened to trigger the next broadcast.
    pub(super) fn on_leave(&mut self, seat: Seat) {
        if let Some(slot) = self.seats.get_mut(seat) {
            *slot = None;
            info!(seat, "seat disconnected; held open for reconnect");
            self.broadcast();
        }
    }

    /// Attach a spectator (issue #351) and bring it current with a single
    /// redacted [`SpectatorView`] — the whole public board, so a mid-game spectator
    /// reconstructs its UI with no history. A spectator owns no seat and never mutates
    /// the game; a dead spectator sender is pruned lazily on the next broadcast.
    pub(super) fn on_join_spectator(&mut self, outbox: watch::Sender<Option<SpectatorView>>) {
        let mut view = spectator_view(&self.state, &self.db);
        view.player_names = self.player_names_map();
        self.overlay_spectator_presentation(&mut view);
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
        self.overlay_spectator_presentation(&mut view);
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
        // Priority-stop preferences and the auto-pass indicators are likewise room
        // state, not engine state, and per-viewer (issues #264 and #455).
        //
        // The stops reflected are the **effective** ones, policy seeds included: the
        // client must be shown the set the room actually honours, or a seat carrying
        // the human main-phase default would draw those steps as "Auto" while the
        // settle stopped at them.
        let stops = self.effective_stops(seat);
        view.stops = stops.any_turn;
        view.own_turn_stops = stops.own_turn;
        // Where the last settle acted for this seat, and the boolean summary of that
        // list ADR 0010 shipped first — derived from it rather than tracked beside it,
        // so the two can never disagree.
        view.auto_passed_steps = self
            .auto_passed_steps
            .get(seat)
            .cloned()
            .unwrap_or_default();
        view.auto_passed = !view.auto_passed_steps.is_empty();
        // Room-owned presentation metadata (issue #553): the match format, and each
        // seat's connection/AI state. Engine-derived commander identity and the
        // per-permanent commander marker already rode the pure projection.
        view.format.clone_from(&self.format);
        view.me.connected = self.seat_connected(seat);
        view.me.ai = self.seat_is_ai(seat);
        self.overlay_seat_presentation(&mut view);
        // Rejected-action feedback (issue #265): the only caller that sets this is the
        // rejection re-send, and the game state is unchanged, so this rides an otherwise
        // ordinary resync — advisory presentation, never load-bearing.
        view.action_rejected = action_rejected;
        // The acknowledgement for this seat's last correlated submission (issue #554).
        // Every other seat's view carries none.
        //
        // Copied, not taken. A seat's outbox is a latest-value `watch`, so a view sent
        // before the socket task drains the previous one *replaces* it: taking the ack
        // out on the first send would drop it on the floor whenever a second broadcast
        // (another seat acting, a settle) overtook it, and the client — which correctly
        // does not release its marker on an ack-less frame — would sit pending forever.
        // Riding every view until it is superseded makes the ack survive any amount of
        // coalescing: whichever view actually arrives carries it.
        //
        // Re-delivery is harmless because the ack is *correlated*: a client matches it
        // against the submission it is still waiting on, so a repeat of one already
        // consumed names nothing and does nothing. It is superseded by this seat's next
        // submission (correlated or not, see `record_ack`) and dropped when the seat
        // reconnects, so it never outlives the connection that earned it.
        view.action_ack = self.pending_acks.get(seat).cloned().flatten();
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
    /// fan-out is a no-op when there are no spectators (issue #351).
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

    use sage_protocol::{ChooseAction, ClientMessage};

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
                ..Default::default()
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
    async fn issue_553_seat_presentation_and_format_ride_the_room_overlay() {
        // The room owns the three facts the engine cannot know: which seats hold a
        // live connection, which are AI, and what format the game is played under.
        // Each must reach every seat's view and every spectator's, and a seat that
        // disconnects must be *broadcast* as disconnected rather than waiting for an
        // unrelated action to trigger the next push.
        let (handle, task) = Room::new(dealt_state(), db())
            .with_ai_seats(vec![false, true])
            .with_format(sage_protocol::MatchFormat {
                id: "commander".into(),
                commander: true,
            })
            .spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view0 = wait_for_view(&mut rx0).await;

        // The format signal reaches the seat verbatim.
        let format = view0.format.clone().expect("the room supplies a format");
        assert_eq!(format.id, "commander");
        assert!(format.commander);

        // Seat 1 is an AI seat and has never connected: both facts are public.
        assert_eq!(view0.opponents.len(), 1);
        assert!(view0.opponents[0].ai, "seat 1 is AI-controlled");
        assert!(!view0.opponents[0].connected, "seat 1 never joined");
        // The receiver's own record reads as a connected human.
        assert!(view0.me.connected && !view0.me.ai);

        // Seat 1 connects: seat 0 is re-broadcast a view in which it is connected.
        let (tx1, mut rx1) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1
        }));
        let view1 = wait_for_view(&mut rx1).await;
        assert!(view1.me.ai, "the AI seat's own record says so");
        let mut view0 = wait_for_view(&mut rx0).await;
        while !view0.opponents[0].connected {
            view0 = wait_for_view(&mut rx0).await;
        }

        // A spectator sees the same public facts, seat-indexed, with the format.
        let (stx, mut srx) = watch::channel::<Option<SpectatorView>>(None);
        assert!(handle.send(RoomInput::JoinSpectator { outbox: stx }));
        let spec = wait_for_spectator_view(&mut srx).await;
        assert!(spec.format.is_some_and(|f| f.commander));
        assert!(spec.players[0].connected && !spec.players[0].ai);
        assert!(spec.players[1].connected && spec.players[1].ai);

        // Seat 1 drops. The seat is held open (the game is untouched), and seat 0 is
        // pushed a view that says so — directly from authoritative state.
        assert!(handle.send(RoomInput::Leave { seat: 1 }));
        let mut after = wait_for_view(&mut rx0).await;
        while after.opponents[0].connected {
            after = wait_for_view(&mut rx0).await;
        }
        assert!(after.opponents[0].ai, "AI-ness is unaffected by the drop");
        assert!(!after.valid_actions.is_empty(), "the game is untouched");

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_553_a_room_with_no_format_or_ai_seats_sends_the_pre_553_shape() {
        // The additive default: a room built the old way (no format, no AI roster)
        // projects no format at all and marks nothing, so the wire is unchanged.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view = wait_for_view(&mut rx0).await;
        assert!(view.format.is_none());
        assert!(!view.opponents[0].ai);
        assert!(view.me.connected && !view.me.ai && !view.me.eliminated);
        let json = serde_json::to_value(&view).unwrap();
        for absent in ["format", "commander_identity"] {
            assert!(json.get(absent).is_none(), "`{absent}` elides at default");
        }
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_554_a_submission_is_acknowledged_by_its_own_id_on_the_view_that_answers_it() {
        // The correlation loop: a client tags its `ChooseAction` with an opaque id, and
        // exactly the view answering *that* submission carries the ack. This is what
        // `action_rejected` alone could not give — it says something was refused, never
        // which submission, so a pending indicator could not tell its own answer from a
        // broadcast another seat caused.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view = wait_for_view(&mut rx0).await;
        assert!(
            view.action_ack.is_none(),
            "a join resync answers no submission"
        );
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
            .expect("a pass is offered to the priority holder");

        // An accepted submission comes back acknowledged, by its own id.
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                submission: "s:17".into(),
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                targets: vec![],
            }),
        }));
        let answered = wait_for_view(&mut rx0).await;
        let ack = answered
            .action_ack
            .clone()
            .expect("the submission is acked");
        assert_eq!(ack.submission, "s:17");
        assert!(ack.accepted);

        // A stale/unknown id is rejected — and the rejection is tied to the submission
        // that caused it, not merely flagged in the abstract.
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                submission: "s:18".into(),
                action_id: "a-nope".into(),
                token: String::new(),
                targets: vec![],
            }),
        }));
        let rejected = wait_for_view(&mut rx0).await;
        let ack = rejected
            .action_ack
            .clone()
            .expect("a rejection is acked too");
        assert_eq!(ack.submission, "s:18");
        assert!(!ack.accepted);
        assert!(
            rejected.action_rejected,
            "the existing flag still rides too"
        );

        // The ack belongs to the connection that earned it: a reconnect starts with
        // nothing outstanding, so a later resync never re-fires it at a client whose
        // own marker that same discontinuity already cleared.
        assert!(handle.send(RoomInput::Leave { seat: 0 }));
        let (tx0b, mut rx0b) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0b
        }));
        let resumed = wait_for_view(&mut rx0b).await;
        assert!(resumed.action_ack.is_none());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_554_an_ack_rides_every_view_until_it_is_superseded() {
        // A seat's outbox is a latest-value `watch`: a view pushed before the socket
        // task drains the previous one *replaces* it. An ack taken out on its first
        // send is therefore lost whenever a second broadcast — another seat acting, a
        // seat dropping, a settle — overtakes it, and the client, which correctly keeps
        // its pending marker on an ack-less frame, would sit pending forever.
        //
        // The property that makes coalescing harmless is asserted directly: the ack is
        // on *every* view this seat is sent until something supersedes it, so whichever
        // view survives the overwrite carries it.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        let (tx1, rx1) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        assert!(handle.send(RoomInput::Join {
            seat: 1,
            outbox: tx1
        }));
        let view = wait_for_view(&mut rx0).await;
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
            .expect("a pass is offered to the priority holder");

        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                submission: "s:42".into(),
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                targets: vec![],
            }),
        }));
        let answered = wait_for_view(&mut rx0).await;
        assert_eq!(
            answered
                .action_ack
                .as_ref()
                .map(|ack| ack.submission.clone()),
            Some("s:42".into())
        );
        // Seat 1's own view never carries another seat's ack.
        assert!(rx1.borrow().as_ref().unwrap().action_ack.is_none());

        // An unrelated broadcast — seat 1 dropping — is exactly the frame that would
        // overwrite seat 0's answer in flight. It carries the ack too.
        assert!(handle.send(RoomInput::Leave { seat: 1 }));
        let unrelated = wait_for_view(&mut rx0).await;
        assert!(!unrelated.opponents[0].connected, "seat 1 dropped");
        assert_eq!(
            unrelated.action_ack.map(|ack| ack.submission),
            Some("s:42".into()),
            "a view coalescing over the answer must still carry it"
        );

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_554_an_uncorrelated_submission_supersedes_a_riding_ack() {
        // The ack rides views until this seat's next submission replaces it. An
        // *uncorrelated* submission is a replacement too — it clears the slot — so a
        // client that stops correlating is never answered with a stale id.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view = wait_for_view(&mut rx0).await;
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
            .expect("a pass is offered");
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                submission: "s:7".into(),
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                targets: vec![],
            }),
        }));
        let acked = wait_for_view(&mut rx0).await;
        assert_eq!(
            acked.action_ack.map(|ack| ack.submission),
            Some("s:7".into())
        );

        // A rejected, uncorrelated submission: it is answered with a resync carrying
        // no ack at all, not with "s:7" again.
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: "a-nope".into(),
                ..Default::default()
            }),
        }));
        let answered = wait_for_view(&mut rx0).await;
        assert!(answered.action_rejected);
        assert!(answered.action_ack.is_none());

        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn issue_554_an_uncorrelated_submission_is_answered_exactly_as_before() {
        // A client that sends no correlation id gets no ack, so the wire an older
        // client sees is byte-for-byte what it saw before issue #554.
        let (handle, task) = Room::new(dealt_state(), db()).spawn();
        let (tx0, mut rx0) = view_channel();
        assert!(handle.send(RoomInput::Join {
            seat: 0,
            outbox: tx0
        }));
        let view = wait_for_view(&mut rx0).await;
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .cloned()
            .expect("a pass is offered");
        assert!(handle.send(RoomInput::Message {
            seat: 0,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                ..Default::default()
            }),
        }));
        let answered = wait_for_view(&mut rx0).await;
        assert!(answered.action_ack.is_none());
        assert!(serde_json::to_value(&answered)
            .unwrap()
            .get("action_ack")
            .is_none());

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
