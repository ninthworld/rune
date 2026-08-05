//! Evaluating an [`Effect::Conditional`](crate::Effect)'s [`Condition`] — the
//! intervening-if clause of `If you control three or more artifacts, draw two cards
//! instead.`
//!
//! Two kinds of question live here, and they read different things:
//!
//! - a question about the **board** ([`Condition::ControlsAtLeast`]) counts permanents,
//!   the way every mass selector does;
//! - a question about **what this resolution has already done**
//!   ([`Condition::MilledThisWay`], [`Condition::DiscardedThisWay`]) reads the events the
//!   resolution recorded, over a window that begins where the resolution did.
//!
//! The second kind is why this is a module rather than a match arm. A snapshot cannot
//! answer it: a Zombie already in a graveyard is indistinguishable from one milled a
//! moment ago, and a hand that is one card lighter is indistinguishable from one that was
//! never that big. Reading the recorded events is the same discipline the life-gain and
//! cast trigger conditions already follow (ADR 0007), applied to a narrower window.

use crate::ability::{CardFilter, Condition, CountScope, PermanentCount};
use crate::id::PlayerId;
use crate::state::{GameEvent, GameState};
use crate::CardDatabase;

/// Whether `condition` holds for an object controlled by `controller`, resolving in a
/// window that began at log sequence `resolution_start`.
#[must_use]
pub(crate) fn condition_holds(
    state: &GameState,
    condition: &Condition,
    controller: PlayerId,
    resolution_start: u64,
    db: &CardDatabase,
) -> bool {
    match condition {
        Condition::ControlsAtLeast { permanents, count } => {
            count_permanents(state, permanents, controller, db) >= *count
        }
        Condition::MilledThisWay { filter } => events_since(state, resolution_start).any(|event| {
            matches!(event, GameEvent::CardsMilled { player, cards, .. }
                if *player == controller
                    && cards
                        .iter()
                        .any(|card| card_matches(db, card.card, filter)))
        }),
        Condition::DiscardedThisWay => events_since(state, resolution_start).any(|event| {
            matches!(event, GameEvent::CardsDiscarded { player, count }
                if *player == controller && *count > 0)
        }),
    }
}

/// How many permanents `wanted` names, for an object controlled by `controller`.
///
/// Reads **printed** types and subtypes, consistent with every other selector in the
/// engine: the type-changing layers are not implemented, so printed types are current
/// types, and a count taken from computed characteristics would recurse through the very
/// layer system a static ability's count would be feeding.
#[must_use]
pub(crate) fn count_permanents(
    state: &GameState,
    wanted: &PermanentCount,
    controller: PlayerId,
    db: &CardDatabase,
) -> u32 {
    let matching = state.battlefield.iter().filter(|perm| {
        let scope_ok = match wanted.scope {
            CountScope::YouControl => perm.controller == controller,
            // A seat that has lost is no longer an opponent (CR 102.1) — the same
            // exclusion every other controller-relative selector makes.
            CountScope::OpponentsControl => {
                perm.controller != controller
                    && state
                        .players
                        .get(perm.controller.0)
                        .is_some_and(|player| !player.has_lost)
            }
            CountScope::Any => true,
        };
        if !scope_ok {
            return false;
        }
        let Some(face) = perm.printed.face(db) else {
            return false;
        };
        wanted
            .card_type
            .is_none_or(|card_type| face.has_type(card_type))
            && wanted
                .subtype
                .as_deref()
                .is_none_or(|subtype| face.has_subtype(subtype))
            && wanted
                .color
                .is_none_or(|color| face.colors().contains(&color))
    });
    u32::try_from(matching.count()).unwrap_or(u32::MAX)
}

/// The events recorded at or after log sequence `from` — the window a
/// "…this way" condition reads.
///
/// The log is a bounded ring, so a resolution that recorded more events than the window
/// holds would lose its earliest ones. A single resolution records a handful.
fn events_since(state: &GameState, from: u64) -> impl Iterator<Item = &GameEvent> {
    state
        .log
        .iter()
        .filter(move |entry| entry.sequence >= from)
        .map(|entry| &entry.event)
}

/// Whether the printed card `card` satisfies `filter`.
///
/// Delegates to the one filter predicate a mid-resolution card choice uses
/// ([`crate::choice::card_matches_filter`]), so "a Zombie card was milled this way" and
/// "you may pick a Zombie card" can never disagree about what a Zombie card is.
#[must_use]
fn card_matches(db: &CardDatabase, card: crate::id::CardId, filter: &CardFilter) -> bool {
    crate::choice::card_matches_filter(db, card, filter, None)
}
