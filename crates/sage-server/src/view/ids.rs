//! Opaque wire entity-id and small formatting helpers.

use super::*;

/// The opaque protocol id for a seat (an engine [`PlayerId`]).
pub(crate) fn player_id(seat: PlayerId) -> String {
    format!("p{}", seat.0)
}

/// The opaque protocol id for a card referenced from a hand or a public pile.
///
/// Keyed by the per-copy [`CardInstanceId`], so two copies of the same printing
/// get distinct entity ids (`card_5` vs `card_6`) and the action a client echoes
/// back names an unambiguous instance — the engine no longer falls back to "the
/// first matching copy".
pub(crate) fn card_entity_id(instance: CardInstanceId) -> String {
    format!("card_{}", instance.0)
}

/// The opaque protocol id for a permanent on the battlefield.
pub(crate) fn permanent_entity_id(id: PermanentId) -> String {
    format!("perm_{}", id.0)
}

/// The opaque protocol id for an emblem (CR 114).
///
/// Minted from the same monotonic object counter a permanent's id is, so it can never
/// collide with one — but prefixed distinctly anyway, because an emblem is not a
/// permanent and a client must never treat the two ids as interchangeable.
pub(crate) fn emblem_entity_id(id: u64) -> String {
    format!("emblem_{id}")
}

/// The wire name for an engine [`CounterKind`], as the client expects it in
/// [`Counter::kind`] (e.g. `"+1/+1"`). Kept exhaustive so a new engine variant
/// forces a matching wire string here rather than silently going unnamed.
pub(crate) fn counter_kind_str(kind: CounterKind) -> &'static str {
    match kind {
        CounterKind::PlusOnePlusOne => "+1/+1",
        CounterKind::MinusOneMinusOne => "-1/-1",
        CounterKind::Loyalty => "loyalty",
        CounterKind::Charge => "charge",
        CounterKind::Gold => "gold",
        CounterKind::Wish => "wish",
        CounterKind::Corpse => "corpse",
    }
}

/// The opaque protocol id for whatever an attacker is attacking (CR 508.1a): a seat id
/// for a player, a permanent id for a planeswalker.
///
/// The one place the two halves of an [`AttackTarget`] are flattened into a single wire
/// string, so the `defend_<id>` requirement slot can list players and planeswalkers in
/// one candidate list and [`bind_attackers`](super::bind_attackers) can resolve an
/// answer back by recomputing this over the freshly computed candidates — never by
/// parsing the id apart.
pub(crate) fn attack_target_entity_id(target: AttackTarget) -> String {
    match target {
        AttackTarget::Player(seat) => player_id(seat),
        AttackTarget::Planeswalker(id) => permanent_entity_id(id),
    }
}

/// The opaque protocol id for an object on the stack.
pub(crate) fn stack_entity_id(id: StackId) -> String {
    format!("stack_{}", id.0)
}

/// Saturating `usize`→`u32` for wire counts; avoids both a panic and a lossy cast.
pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
