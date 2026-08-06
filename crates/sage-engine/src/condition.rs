//! Evaluating an [`Effect::Conditional`](crate::Effect)'s [`Condition`] — the
//! intervening-if clause of `If you control three or more artifacts, draw two cards
//! instead.` — and the [`DerivedAmount`] behind the `X` of `where X is …`.
//!
//! The two belong together because they ask the same questions and differ only in what
//! they do with the answer: `if you gained life this turn` and `X is the amount of life
//! you gained this turn` read one number, once, and one of them compares it while the
//! other uses it. Sharing the readers is what keeps them from ever disagreeing.
//!
//! Two kinds of question live here, and they read different things:
//!
//! - a question about the **board** ([`Condition::ControlsAtLeast`],
//!   [`DerivedAmount::GreatestManaValue`]) reads the permanents on it, the way every
//!   mass selector does;
//! - a question about **what has already happened**
//!   ([`Condition::MilledThisWay`], [`Condition::DiscardedThisWay`],
//!   [`Condition::GainedLifeThisTurn`], [`DerivedAmount::MilledThisWay`],
//!   [`DerivedAmount::LifeGainedThisTurn`]) reads the events that were recorded, over a
//!   window that is either the resolution or the turn.
//!
//! The second kind is why this is a module rather than a match arm. A snapshot cannot
//! answer it: a Zombie already in a graveyard is indistinguishable from one milled a
//! moment ago, a hand that is one card lighter is indistinguishable from one that was
//! never that big, and a life total that did not move is indistinguishable from a life
//! total that went up and came back down. Reading the recorded events is the same
//! discipline the life-gain and cast trigger conditions already follow (ADR 0007),
//! applied to a window this module chooses.

use crate::ability::{CardFilter, Condition, CountScope, DerivedAmount, PermanentCount};
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
        Condition::GainedLifeThisTurn { amount } => {
            life_gained_this_turn(state, controller) >= *amount
        }
    }
}

/// How many permanents `wanted` names, for an object controlled by `controller`.
///
/// Reads **printed** types and subtypes, consistent with every other selector in the
/// engine: the type-changing layers are not implemented, so printed types are current
/// types, and a count taken from computed characteristics would recurse through the very
/// layer system a static ability's count would be feeding.
///
/// [`PermanentCount::min_power`] is the one exception and is deliberately narrow: power
/// *is* changed by layers that exist, so a printed reading would be wrong on every
/// pumped creature. The computed reading is taken **lazily** — nothing calls
/// [`crate::characteristics`] unless a bound is authored — and the catalog validator
/// refuses the field inside a static ability's condition, which is the only caller that
/// would recurse. Together those two facts are why this function is safe to call from
/// inside the layer system.
#[must_use]
pub(crate) fn count_permanents(
    state: &GameState,
    wanted: &PermanentCount,
    controller: PlayerId,
    db: &CardDatabase,
) -> u32 {
    let matching = permanents_matching(state, wanted, controller, db);
    u32::try_from(matching.count()).unwrap_or(u32::MAX)
}

/// The permanents `wanted` names, in battlefield order, for an object controlled by
/// `controller`.
///
/// The shared body of every question asked *about* a counted class: how many there are
/// ([`count_permanents`]) and the greatest mana value among them
/// ([`DerivedAmount::GreatestManaValue`]). One predicate, so "artifacts you control"
/// cannot mean one set when it is counted and another when it is measured.
fn permanents_matching<'a>(
    state: &'a GameState,
    wanted: &'a PermanentCount,
    controller: PlayerId,
    db: &'a CardDatabase,
) -> impl Iterator<Item = &'a crate::state::Permanent> {
    state.battlefield.iter().filter(move |perm| {
        // CR 613 layer 2, read through the one control path: a creature someone has
        // gained control of counts for its *new* controller and not for its old one.
        let perm_controller = crate::characteristics::controller_of(state, perm);
        let scope_ok = match wanted.scope {
            CountScope::YouControl => perm_controller == controller,
            // A seat that has lost is no longer an opponent (CR 102.1) — the same
            // exclusion every other controller-relative selector makes.
            CountScope::OpponentsControl => {
                perm_controller != controller
                    && state
                        .players
                        .get(perm_controller.0)
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
        let printed_ok = wanted
            .card_type
            .is_none_or(|card_type| face.has_type(card_type))
            && wanted
                .subtype
                .as_deref()
                .is_none_or(|subtype| face.has_subtype(subtype))
            && wanted
                .color
                .is_none_or(|color| face.colors().contains(&color));
        if !printed_ok {
            return false;
        }
        // Computed, and only when a bound is authored (see this function's docs). A
        // permanent with no power — a land, an enchantment — satisfies no bound.
        wanted.min_power.is_none_or(|min| {
            crate::characteristics::characteristics(state, perm.id, db)
                .power
                .is_some_and(|power| power >= min)
        })
    })
}

/// The number a [`DerivedAmount`] names, for an object controlled by `controller`
/// resolving as described by `resolution` — its log window, the X it announced, and what
/// its cost payment recorded.
///
/// Taken **once**, where it is called (CR 608.2): every caller turns the answer into a
/// fixed number — cards drawn, damage dealt, or a timestamped power/toughness modifier —
/// and nothing re-reads the source afterwards.
///
/// Three of the sources read the game: the board, or the events a window recorded. The
/// two that read the object's own [`PaidCost`](crate::PaidCost) read neither, and could
/// not: their answer was settled at announcement and the objects it was about are gone.
#[must_use]
pub(crate) fn derived_amount(
    state: &GameState,
    amount: &DerivedAmount,
    controller: PlayerId,
    resolution: crate::resolve::Resolution,
    db: &CardDatabase,
) -> u32 {
    match amount {
        // CR 601.2b: the value its controller announced, already fixed. Zero for an
        // object that announced none — an effect asking for an X it never had should do
        // nothing rather than guess at one.
        DerivedAmount::AnnouncedX => resolution.announced_x.unwrap_or(0),
        DerivedAmount::LifeGainedThisTurn => life_gained_this_turn(state, controller),
        // Every card this resolution milled, whoever's library it came from — see
        // [`DerivedAmount::MilledThisWay`] for why the seat is not a field. A resolution
        // that milled nothing counts nothing, which is the zero a card that draws "for
        // each land milled this way" wants off an empty library.
        DerivedAmount::MilledThisWay { filter } => {
            let milled = events_since(state, resolution.start)
                .filter_map(|event| match event {
                    GameEvent::CardsMilled { cards, .. } => Some(cards),
                    _ => None,
                })
                .flatten()
                .filter(|card| card_matches(db, card.card, filter))
                .count();
            u32::try_from(milled).unwrap_or(u32::MAX)
        }
        // CR 202.3b: mana value is a number off the printed face, so a token — which has
        // no mana cost at all — is a zero rather than an exclusion. An empty class is
        // zero for the same reason, which is what makes such a spell castable with
        // nothing on the board.
        DerivedAmount::GreatestManaValue { among } => {
            permanents_matching(state, among, controller, db)
                .filter_map(|perm| perm.printed.face(db))
                .map(|face| face.mana_value())
                .max()
                .unwrap_or(0)
        }
        // Read off the payment, not off the game (CR 601.2h). Zero when the cost took
        // nothing, which is what makes `Sacrifice any number of lands` a legal cast on an
        // empty board and a search for nothing.
        DerivedAmount::SacrificedToCost => resolution.paid.sacrificed,
        // CR 608.2h last-known information, and clamped at zero because damage is never
        // negative (CR 120.1): a creature sacrificed at −1/−1 throws nothing.
        DerivedAmount::SacrificedCreaturePower => resolution
            .paid
            .sacrificed_power
            .and_then(|power| u32::try_from(power).ok())
            .unwrap_or(0),
    }
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

/// How much life `player` has **gained** so far this turn (CR 118.3).
///
/// A sum of the turn's gains, not a net and not a maximum: three gained twice is six,
/// and three gained and then lost is still three. Only a positive
/// [`GameEvent::LifeChanged`] is a gain — a payment or a loss is a negative one, and
/// damage to a player is recorded as damage rather than as a life change, so neither
/// ever reaches this.
#[must_use]
fn life_gained_this_turn(state: &GameState, player: PlayerId) -> u32 {
    events_since(state, turn_start(state))
        .filter_map(|event| match event {
            GameEvent::LifeChanged {
                player: gained_by,
                amount,
            } if *gained_by == player && *amount > 0 => u32::try_from(*amount).ok(),
            _ => None,
        })
        .fold(0, u32::saturating_add)
}

/// The log sequence this turn's events begin at — the window a "…this turn" condition
/// reads.
///
/// Found by walking **backwards** to the last step the *previous* turn recorded, rather
/// than forwards to the first step this one did. The log is a bounded ring, and the two
/// differ exactly when it has dropped entries: a forward search would then find the
/// earliest surviving step of this turn and wrongly exclude everything the ring still
/// holds from before it. Walking backwards degrades the other way — with no previous
/// turn left in the window every surviving entry belongs to this turn, which is what
/// `0` says. A turn that overflowed the ring outright can therefore under-count its
/// earliest gains and can never over-count.
#[must_use]
fn turn_start(state: &GameState) -> u64 {
    state
        .log
        .iter()
        .rev()
        .find(|entry| {
            matches!(entry.event, GameEvent::StepChanged { turn, .. } if turn != state.turn)
        })
        .map_or(0, |entry| entry.sequence + 1)
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
