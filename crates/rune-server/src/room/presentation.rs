//! The room's **presentation trail** (issue #594): the ordered, monotonically
//! identified, bounded window of display-only moments that rides every view.
//!
//! # The problem this exists for
//! The room applies an action and then *settles* — resolving the stack, passing
//! priority for idle seats (ADR 0020), advancing steps — before it broadcasts, and the
//! per-seat outbox is a latest-value [`watch`](tokio::sync::watch): a view pushed while
//! an earlier one is still in flight replaces it. Both together mean a seat is handed a
//! *final* board where the game passed through a sequence of causal states. Diffing two
//! boards says a creature is gone; it cannot say whether it was countered, killed,
//! sacrificed, or exiled — and the client is forbidden to guess (`AGENTS.md`: zero game
//! logic in the client). So the server states the sequence.
//!
//! # Why a carried window, and not a queue the server drains
//! The same no-loss contract [`GameView::log`](rune_protocol::GameView::log) already
//! uses: every view carries the recent **unconsumed suffix**, so nothing depends on the
//! previous message having been seen. Two broadcasts that coalesce into one lose no
//! moments, a reconnecting client reads the identical field, and the server never has
//! to know what any client has rendered. The alternative — a per-seat queue the room
//! drains on send — would lose exactly the moments that coalescing was supposed to
//! preserve, since the drained view is the one that gets replaced.
//!
//! **The server never sleeps for presentation.** Nothing here delays a broadcast, a
//! settle, or an applied action; dwell is entirely the client's, and a consumer that
//! ignores the field (the CLI, the AI harness) plays the identical game.
//!
//! # Derived, never a second source of truth
//! Moments are produced from two things the room already has:
//! - the suffix of the receiver-safe protocol log past a cursor, mapped 1:1 by the pure
//!   [`crate::view::moments_from_log`];
//! - the room's own `auto_passed_steps` accumulator, for the per-seat
//!   [`MomentKind::PhasesSkipped`].
//!
//! Nothing is invented by diffing states, which is why the trail can never disagree
//! with the log about what happened.

use std::collections::{BTreeMap, VecDeque};

use rune_engine::StackObjectKind;
use rune_protocol::{
    AutoPassReason, CardView, GameLogEntry, MomentKind, MomentZone, PresentationMoment,
    PRESENTATION_WINDOW,
};

use crate::view::{
    card_entity_id, card_view, moments_from_log, permanent_card_view, permanent_entity_id,
    MomentPosition, RetainedObjects,
};

use super::*;

/// How many retained public faces the trail caches. Bounded because a long game visits
/// far more objects than any window can show, and an unbounded map on a long-lived room
/// task is a leak with extra steps. Oldest-inserted is evicted first: a face is only
/// ever needed for a moment recorded moments ago.
const FACE_CAP: usize = 256;

/// One moment plus **who may see it**.
///
/// Almost every moment is public — the board is public, and a spectator and every
/// opponent read the same stream. The exception is
/// [`MomentKind::PhasesSkipped`](rune_protocol::MomentKind::PhasesSkipped), which names
/// where *one* receiver was passed; it belongs to that seat alone, and its filtering out
/// of every other stream is the sanctioned reason a receiver's ids have gaps
/// (`docs/protocol.md`).
struct Retained {
    /// The seat this moment is for, or `None` for a public moment every receiver
    /// (spectators included) sees.
    audience: Option<Seat>,
    /// The moment as it goes on the wire.
    moment: PresentationMoment,
}

/// The room's ordered window of presentation moments, and the retained faces they are
/// rendered with (issue #594).
///
/// Held on the [`Room`] and never shared: like every other piece of room state, the room
/// task is its sole writer.
#[derive(Default)]
pub(super) struct PresentationTrail {
    /// The retained moments, oldest first. Bounded at twice
    /// [`PRESENTATION_WINDOW`] so that after per-seat filtering a receiver still has a
    /// full window's worth to project, without the room holding history no view will
    /// ever carry.
    moments: VecDeque<Retained>,
    /// Retained **public** card faces by entity id — battlefield, stack, graveyard,
    /// exile, and command only.
    ///
    /// **A hand or library face is never cached, under any circumstance.** That is the
    /// whole information-safety guard of this module, and it is enforced here at the
    /// point of *observation* rather than at projection: a moment crosses seats (every
    /// opponent and every spectator read the same public moments), so a privately-known
    /// face that reached this map could not be redacted afterwards — there is no
    /// per-receiver filter on a `MomentObject`. Nothing downstream needs to re-check,
    /// because nothing downstream can leak what was never observed.
    faces: BTreeMap<String, CardView>,
    /// Insertion order for [`Self::faces`], so eviction is oldest-first.
    face_order: VecDeque<String>,
    /// The public zone each observed object was in when the current batch **opened**,
    /// cleared at the end of every batch. The only honest answer to "where did this come
    /// from" for a move whose event does not name its origin (CR 903.9a); a later
    /// observation inside the same batch must not overwrite it, or the origin recorded
    /// would be the destination.
    origins: BTreeMap<String, MomentZone>,
    /// The highest log sequence already turned into moments. Entries at or below it are
    /// never re-derived, which is what makes recording idempotent across the repeated
    /// projections a coalescing broadcast implies.
    log_cursor: u64,
    /// The next moment id. Monotonic per room and never reused, so a client can treat it
    /// as a watermark.
    next_id: u64,
    /// The next batch id — one per recording that actually produces moments, so a batch
    /// always names a real causal group ("these six things happened because of that one
    /// click") rather than a settle that did nothing.
    next_batch: u64,
    /// Where the game was when the current batch opened, captured by the first
    /// observation of the batch. `None` between batches; the recording takes it, and
    /// falls back to the post-settle position when a settle applied nothing at all.
    opened_at: Option<MomentPosition>,
}

impl PresentationTrail {
    /// Cache the **public** faces of everything currently visible, and remember which
    /// zone each was in if this is the first observation of the batch.
    ///
    /// Called immediately *before* every applied action, which is what makes the cache
    /// useful: a moment is rendered after its object is gone, so the face has to be
    /// taken while the object is still there. Hands and libraries are deliberately not
    /// walked — see [`Self::faces`].
    pub(super) fn observe(&mut self, state: &GameState, db: &CardDatabase) {
        if self.opened_at.is_none() {
            self.opened_at = Some(MomentPosition::of(state));
        }
        for perm in &state.battlefield {
            self.retain(
                permanent_entity_id(perm.id),
                permanent_card_view(state, perm, db),
                MomentZone::Battlefield,
            );
        }
        for object in &state.stack {
            // An ability's face is its source permanent's, already retained above; only
            // a spell brings a card of its own onto the stack.
            if let StackObjectKind::Spell { card } = &object.kind {
                let id = card_entity_id(card.id);
                self.retain(id.clone(), card_view(id, card.card, db), MomentZone::Stack);
            }
        }
        for player in &state.players {
            for (pile, zone) in [
                (&player.graveyard, MomentZone::Graveyard),
                (&player.exile, MomentZone::Exile),
                (&player.command, MomentZone::Command),
            ] {
                for instance in pile {
                    let id = card_entity_id(instance.id);
                    self.retain(id.clone(), card_view(id, instance.card, db), zone);
                }
            }
        }
    }

    /// Retain one public face, refreshing an existing entry in place and evicting the
    /// oldest when the cache is full.
    fn retain(&mut self, id: String, card: CardView, zone: MomentZone) {
        self.origins.entry(id.clone()).or_insert(zone);
        if let Some(slot) = self.faces.get_mut(&id) {
            *slot = card;
            return;
        }
        while self.faces.len() >= FACE_CAP {
            let Some(oldest) = self.face_order.pop_front() else {
                break;
            };
            self.faces.remove(&oldest);
        }
        self.face_order.push_back(id.clone());
        self.faces.insert(id, card);
    }

    /// Turn everything that has happened since the last recording into moments: the log
    /// suffix past the cursor, then this settle's per-seat skipped paths.
    ///
    /// One batch id for the whole call, because one call is one causal group — an
    /// applied action together with the settle that followed it. The per-seat
    /// [`MomentKind::PhasesSkipped`] comes **last** in the batch: it is the "we moved
    /// without you" beat, and it reads as a summary of the events it accompanies rather
    /// than a prelude to them.
    pub(super) fn record(
        &mut self,
        state: &GameState,
        entries: &[GameLogEntry],
        auto_passed: &[Vec<AutoPassedStep>],
        reasons: &[AutoPassReason],
    ) {
        let mut position = self
            .opened_at
            .take()
            .unwrap_or_else(|| MomentPosition::of(state));
        let drafts = moments_from_log(
            entries,
            self.log_cursor,
            &mut position,
            RetainedObjects {
                faces: &self.faces,
                origins: &self.origins,
            },
        );
        self.origins.clear();
        if let Some(last) = entries.last() {
            self.log_cursor = self.log_cursor.max(last.sequence);
        }
        let skipped = auto_passed.iter().any(|steps| !steps.is_empty());
        if drafts.is_empty() && !skipped {
            return;
        }
        let batch = self.next_batch;
        self.next_batch += 1;

        // Resolve each draft's cause from indices to ids as we go: a draft can only name
        // an earlier one, so the id it needs is always already assigned — and if that
        // earlier moment was folded into an aggregate, the cause names the survivor.
        let mut ids: Vec<u64> = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let cause = draft.cause.and_then(|at| ids.get(at).copied());
            let id = self.push(batch, None, draft.turn, draft.phase, draft.kind, cause);
            ids.push(id);
        }

        for (seat, steps) in auto_passed.iter().enumerate() {
            if steps.is_empty() {
                continue;
            }
            // The path is one moment, never one per priority window (issue #455): a
            // settle that passed a seat six times skipped one stretch, and saying so six
            // times reads as six events. Labelled where the stretch *began*, since that
            // is the position the seat last held priority at under its own control.
            let (turn, phase) = steps
                .first()
                .map_or((position.turn, position.phase), |step| {
                    (step.turn, step.phase)
                });
            let reason = reasons
                .get(seat)
                .copied()
                .unwrap_or(AutoPassReason::NoResponseAvailable);
            self.push(
                batch,
                Some(seat),
                turn,
                phase,
                MomentKind::PhasesSkipped {
                    steps: steps.clone(),
                    reason,
                },
                None,
            );
        }
    }

    /// Append one moment, aggregating it into its predecessor when the two are
    /// indistinguishable, and trimming the retained history.
    ///
    /// **Aggregation** collapses *consecutive* public moments of an identical kind
    /// inside one batch into a single entry with `count` raised: six triggers of the
    /// same ability or four instances of the same damage cost one caption ("x6") instead
    /// of six dwells that would starve the window of anything worth watching. It is
    /// deliberately narrow — same batch (two causal groups are two events even when they
    /// look alike), public only (a per-seat moment must never absorb another seat's),
    /// and never for a moment that names a cause (a link in a causal chain is not a
    /// repetition). The surviving id is the *first* occurrence's, so ids stay strictly
    /// increasing and any cause pointing at it still resolves.
    fn push(
        &mut self,
        batch: u64,
        audience: Option<Seat>,
        turn: u32,
        phase: Phase,
        kind: MomentKind,
        cause: Option<u64>,
    ) -> u64 {
        if audience.is_none() && cause.is_none() {
            if let Some(last) = self.moments.back_mut() {
                if last.audience.is_none() && last.moment.batch == batch && last.moment.kind == kind
                {
                    last.moment.count = last.moment.count.saturating_add(1);
                    return last.moment.id;
                }
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.moments.push_back(Retained {
            audience,
            moment: PresentationMoment {
                id,
                batch,
                turn,
                phase,
                kind,
                cause,
                count: 1,
            },
        });
        while self.moments.len() > PRESENTATION_WINDOW * 2 {
            self.moments.pop_front();
        }
        id
    }

    /// The window `seat` receives: every public moment plus that seat's own, newest
    /// [`PRESENTATION_WINDOW`] last. Another seat's per-seat moments are absent, which
    /// is why a receiver's ids may skip values.
    pub(super) fn for_seat(&self, seat: Seat) -> Vec<PresentationMoment> {
        self.project(|retained| retained.audience.is_none_or(|other| other == seat))
    }

    /// The window a **spectator** receives: public moments only. A spectator owns no
    /// seat, so there is no `phases_skipped` that could be theirs (ADR 0022).
    pub(super) fn public(&self) -> Vec<PresentationMoment> {
        self.project(|retained| retained.audience.is_none())
    }

    /// The newest [`PRESENTATION_WINDOW`] moments matching `keep`, in order. Trimming
    /// the *oldest* is the only sane direction: a receiver that is behind has already
    /// missed moments and catches up by watching the newest, never by asking for the
    /// rest — there is no backfill in this protocol.
    fn project(&self, keep: impl Fn(&Retained) -> bool) -> Vec<PresentationMoment> {
        let mut window: Vec<PresentationMoment> = self
            .moments
            .iter()
            .filter(|retained| keep(retained))
            .map(|retained| retained.moment.clone())
            .collect();
        if window.len() > PRESENTATION_WINDOW {
            window.drain(..window.len() - PRESENTATION_WINDOW);
        }
        window
    }
}

impl Room {
    /// Cache the public faces of the state as it stands (issue #594).
    ///
    /// Called immediately **before** every applied action — the human's in
    /// [`Self::on_message`], each automatic one inside the settle, the timeout default,
    /// and once at room start — because a face has to be taken while its object still
    /// exists. A moment is shown after the board has moved past it.
    pub(super) fn observe_presentation(&mut self) {
        self.presentation.observe(&self.state, &self.db);
    }

    /// Turn everything the last settle did into moments (issue #594), reading the same
    /// receiver-safe log the view carries plus the settle's per-seat skipped paths.
    ///
    /// Called as the last statement of every settle, including the one that applied
    /// nothing and the [`AutoPassPolicy::Off`] early return — so no path can advance the
    /// game without the moments for it being recorded, and automation being off simply
    /// means no `phases_skipped` is ever produced (the accumulator stays empty).
    pub(super) fn record_presentation(&mut self, reasons: &[AutoPassReason]) {
        let entries = crate::view::log_entries(&self.state, &self.db);
        self.presentation
            .record(&self.state, &entries, &self.auto_passed_steps, reasons);
    }
}

#[cfg(test)]
mod tests;
