//! Client-message routing and priority automation: applying a chosen action or
//! rejecting it, recording stop preferences, and the settle loop that auto-passes
//! idle priority and resolves choiceless forced declarations (issues #264 and #453,
//! ADR 0020). These are additional `impl Room` blocks over the struct defined in the
//! module root; the tests live in [`tests`].

use rune_engine::{
    apply_action, forced_declaration_without_choice, priority_has_no_meaningful_action, Action,
    PlayerId,
};
use rune_protocol::{ActionAck, AutoPassReason, ClientMessage, SetStops};
use tracing::warn;

use crate::view::{phase_of, resolve_action};

use super::*;

/// A hard cap on how many actions one settle may apply on players' behalf, a
/// defence against a configuration that never reaches a meaningful decision.
///
/// Before issue #453 the loop terminated naturally every turn: the active player's
/// declare-attackers step is a forced choice that offers no pass, so the settle
/// always stopped there at the latest. Now that a *choiceless* declaration is
/// resolved too, that guarantee is gone for a board on which neither seat can ever
/// act — a game with genuinely nothing to do fast-forwards until this cap. Reaching
/// it is logged and the settle stops rather than hanging the task; the seat holding
/// priority is then simply handed its view, as before.
const MAX_AUTO_PASSES: usize = 256;

impl Room {
    /// Route a client message. A chosen action the engine offered this seat is
    /// applied and every connected seat is re-broadcast its view; anything else is
    /// rejected and the sender is simply re-sent its current view (full-state
    /// resync), never mutating the game.
    pub(super) fn on_message(&mut self, seat: Seat, message: &ClientMessage) {
        match message {
            ClientMessage::ChooseAction(choose) => {
                let resolved = resolve_action(&self.state, &self.db, PlayerId(seat), choose);
                // Correlate the answer with this exact submission (issue #554), before
                // the state moves, so the seat's pending UI clears on *its own* answer
                // rather than on whichever broadcast arrives next. An uncorrelated
                // submission (no id) records nothing and the wire is unchanged.
                self.record_ack(seat, &choose.submission, resolved.is_some());
                match resolved {
                    Some(action) => {
                        // Retain the public faces of everything about to be affected
                        // (issue #594) *before* the state moves: a moment is shown after
                        // its object is gone, so this is the last instant a face can be
                        // taken. Costs one walk of the public zones; nothing blocks.
                        self.observe_presentation();
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

    /// Record the acknowledgement `seat` is owed for a submission (issue #554), to be
    /// delivered on every view sent to it until this seat's next submission supersedes
    /// it (see the ack's note in [`Self::send_view_flagged`] for why it rides more than
    /// one view). A submission with no correlation id is not acknowledged at all — an
    /// older client sends exactly the message it always sent and receives exactly the
    /// view it always received — and it *clears* any ack still riding, so an
    /// uncorrelated submission is never answered with the previous one's id.
    fn record_ack(&mut self, seat: Seat, submission: &str, accepted: bool) {
        let Some(slot) = self.pending_acks.get_mut(seat) else {
            return;
        };
        *slot = (!submission.is_empty()).then(|| ActionAck {
            submission: submission.to_string(),
            accepted,
        });
    }

    /// Record a seat's priority-stop preferences (issues #264 and #455, ADR 0020) and
    /// reflect them back. The preferences are held on the room, like the display name,
    /// so they survive reconnect; a stops change can make the current priority holder
    /// newly eligible to auto-pass (they cleared a stop), so a settle runs, and the
    /// clock is re-armed only if that settle actually advanced the game.
    ///
    /// Both halves are replaced together, and recording *any* preference retires the
    /// [`StopPolicy`] seed for this seat for good — that is what makes the human
    /// default (#455) a starting value a player can clear rather than a rule they
    /// cannot escape. A bare `set_stops` with both lists empty therefore means "stop
    /// nowhere", not "give me the defaults again".
    fn on_set_stops(&mut self, seat: Seat, set: &SetStops) {
        let Some(slot) = self.stops.get_mut(seat) else {
            warn!(seat, "set_stops for a seat that does not exist; ignoring");
            return;
        };
        // Replace the seat's preference wholesale, de-duplicated so the reflected
        // lists are canonical (a client that sends the same phase twice sees it once
        // back). A step claimed on both lists keeps only the wider `any_turn` claim,
        // which is what `SeatStops::stops_at` already resolves it to — the client
        // renders one tri-state per step, so echoing it back on both would draw a
        // step as two different things at once.
        let mut any_turn = set.stops.clone();
        any_turn.dedup();
        let mut own_turn = set.own_turn.clone();
        own_turn.dedup();
        own_turn.retain(|phase| !any_turn.contains(phase));
        *slot = Some(SeatStops { any_turn, own_turn });
        let advanced = self.settle_auto_passes();
        if advanced {
            self.arm_deadline();
        }
        if !self.game_over() {
            self.broadcast();
        }
    }

    /// Settle the game past every decision that isn't one: auto-pass the priority
    /// holder while it is idle, and auto-submit a forced combat declaration that has
    /// no legal non-empty answer (issues #264 and #453, ADR 0020). Returns whether
    /// anything was applied.
    ///
    /// A no-op unless [`AutoPassPolicy::On`]. Each iteration applies an ordinary
    /// engine action on behalf of whichever seat currently holds priority — the
    /// engine's own `PassPriority` or the empty `DeclareAttackers`/`DeclareBlockers`
    /// the engine itself handed back — so the resulting state is identical to a
    /// manual click and determinism is preserved. The loop stops the instant a seat
    /// has a meaningful action, owes a forced choice it could actually answer, or has
    /// opted to stop at this step; a fixed [`MAX_AUTO_PASSES`] cap is a defensive
    /// backstop so no configuration can hang the task.
    ///
    /// Every settle ends by recording its **presentation moments** (issue #594) — the
    /// ordered window of what visibly happened, which is the only way a seat that was
    /// passed through a resolution learns the order it happened in. That is why the loop
    /// itself lives in [`Self::run_settle`]: recording has to happen on *every* path out,
    /// including the automation-off early return, and a single wrapper is the only shape
    /// in which no future path can forget.
    pub(super) fn settle_auto_passes(&mut self) -> bool {
        let (advanced, reasons) = self.run_settle();
        self.record_presentation(&reasons);
        advanced
    }

    /// The settle loop proper. Returns whether anything was applied, and why the room
    /// acted for each seat — [`AutoPassReason::ForcedDeclaration`] for a seat whose
    /// choiceless declaration the room submitted (issue #453), and the ordinary
    /// [`AutoPassReason::NoResponseAvailable`] otherwise. The reason is per seat and
    /// carried out of the loop rather than re-derived afterwards, because by the time
    /// the settle ends the board no longer shows which of the two it was.
    fn run_settle(&mut self) -> (bool, Vec<AutoPassReason>) {
        for steps in &mut self.auto_passed_steps {
            steps.clear();
        }
        let mut reasons = vec![AutoPassReason::NoResponseAvailable; self.auto_passed_steps.len()];
        if self.auto_pass != AutoPassPolicy::On {
            return (false, reasons);
        }
        let mut advanced = false;
        let mut applied = 0usize;
        loop {
            if self.game_over() || self.state.priority_holder().is_none() {
                break;
            }
            let seat = self.state.priority.0;
            let Some(action) = self.auto_action_for(seat) else {
                break;
            };
            if applied >= MAX_AUTO_PASSES {
                // Still nothing to decide after the cap. Log it and stop; the game
                // waits for a human rather than the task spinning.
                warn!("auto-pass settle hit its cap without reaching a decision; stopping");
                break;
            }
            // Where the room is acting, recorded *before* the action moves the game
            // on — the position the seat was skipped at is the one it held priority
            // in, not the one the pass lands in (issue #455). The turn rides along
            // because a repeated step is not evidence of a turn boundary: an extra
            // combat phase (CR 506.1) revisits the combat steps inside one turn. Only
            // the room can say which happened, so only the room does.
            let here = AutoPassedStep {
                phase: phase_of(self.state.step),
                turn: self.state.turn,
            };
            // Why the room is acting, recorded for the same reason and at the same
            // moment as *where* (issue #594): a settle can pass a seat and submit an
            // empty declaration for it, and the finished board cannot say which.
            if !matches!(action, Action::PassPriority) {
                if let Some(reason) = reasons.get_mut(seat) {
                    *reason = AutoPassReason::ForcedDeclaration;
                }
            }
            // The public faces as they stand, before this automatic action moves the
            // game past them (issue #594).
            self.observe_presentation();
            let next = apply_action(&self.state, &action, &self.db);
            // Defensive: a step that does not change state would loop forever.
            if next == self.state {
                break;
            }
            self.state = next;
            if let Some(steps) = self.auto_passed_steps.get_mut(seat) {
                // Several priority windows inside one step (each stack resolution
                // opens another) collapse to one entry: the seat was skipped *at*
                // that position, and saying so three times reads as three steps. Only
                // *consecutive* identical positions collapse — a position genuinely
                // revisited later in the path is a second visit and stays one.
                if steps.last() != Some(&here) {
                    steps.push(here);
                }
            }
            advanced = true;
            applied += 1;
        }
        (advanced, reasons)
    }

    /// The action, if any, the room may take on `seat`'s behalf while it holds
    /// priority — the whole of the room's automation policy, in one place.
    ///
    /// Two engine predicates feed it, in order: a seat with no meaningful action
    /// passes (issue #264), and a seat owing a combat declaration with no legal
    /// non-empty answer submits that empty declaration (issue #453, via
    /// [`forced_declaration_without_choice`], which builds the action itself so the
    /// room never re-derives one). Both are gated by the seat's own stop
    /// preferences: a stop is an explicit "hand me priority at this step", and it is
    /// honoured for a choiceless declaration exactly as it is for an idle pass.
    fn auto_action_for(&self, seat: Seat) -> Option<Action> {
        if self.stops_here(seat) {
            return None;
        }
        if priority_has_no_meaningful_action(&self.state, &self.db) {
            return Some(Action::PassPriority);
        }
        forced_declaration_without_choice(&self.state, &self.db)
    }

    /// Whether `seat` has opted to stop at the current step (issues #264 and #455) —
    /// its escape hatch from any automation the room would otherwise apply.
    ///
    /// The own-turn half of the preference is answered against the **active player**,
    /// not the priority holder: "my main phase" means the main phase of the turn I
    /// own, which is the only reading under which the human default (#455) both fixes
    /// the lost turn and leaves every opponent turn at ADR 0020's pacing.
    fn stops_here(&self, seat: Seat) -> bool {
        let here = phase_of(self.state.step);
        let own_turn = self.state.active_player.0 == seat;
        self.effective_stops(seat).stops_at(here, own_turn)
    }

    /// The stop preference in force for `seat`: the one it set, or — if it has never
    /// set one — the room's [`StopPolicy`] seed for a seat of its kind (issue #455).
    ///
    /// This is the single place the seed is resolved, so the settle loop and the view
    /// projection can never disagree about what a seat's stops *are*: the client
    /// renders what the room actually honours, defaults included, which is what keeps
    /// the stops UI reconstructable from one message.
    pub(super) fn effective_stops(&self, seat: Seat) -> SeatStops {
        match self.stops.get(seat) {
            Some(Some(set)) => set.clone(),
            _ => self.stop_policy.seed(self.seat_is_ai(seat)),
        }
    }
}

#[cfg(test)]
mod pacing_tests;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod tests;
