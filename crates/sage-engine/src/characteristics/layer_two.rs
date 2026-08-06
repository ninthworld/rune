//! CR 613 **layer 2**: who controls a permanent right now.
//!
//! The earliest layer the engine models, and the only one whose answer is not a
//! *characteristic* (CR 109.3 — control is not one). It lives here anyway because it is
//! a continuous effect applied in a layer, ordered by the same CR 613.7 timestamps, and
//! because every later layer depends on it: an anthem's "creatures you control" is read
//! against the layer-2 answer, not against the stored field.
//!
//! Deliberately **not** routed through [`characteristics`](super::characteristics).
//! Layer 2 reads only [`GameState::static_effects`] and the permanent's own base
//! controller, so it cannot recurse — which is what lets the layer-6 and layer-7c
//! selectors call it from *inside* the computation of a permanent's characteristics.

use super::*;

/// The player who controls `perm` right now (CR 613 layer 2) — its stored
/// [`Permanent::controller`] with any control-changing continuous effect applied.
///
/// **The single read path** for the question. Every rule that asks who controls a
/// permanent goes through here: the attacker and blocker candidate sets, activation and
/// mana-source eligibility, the untap step, summoning sickness, combat damage
/// attribution, the legend rule, every `you control` / `an opponent controls` selector,
/// a triggered ability's controller, and the seat a projected view files the permanent
/// under. Reading [`Permanent::controller`] directly is correct only where the question
/// is about **ownership** — the four battlefield-departure seams, which send a card to
/// its owner's zone (CR 400.7).
///
/// Computed fresh on every call, caching nothing (ADR 0005). Effects apply in ascending
/// timestamp order (CR 613.7), so the latest control change in force is the one that
/// answers; when it ends the effect under it applies again with nothing to invalidate.
#[must_use]
pub fn controller_of(state: &GameState, perm: &Permanent) -> PlayerId {
    state
        .static_effects
        .iter()
        .filter(|effect| effect.affects == EffectAffects::SpecificPermanent(perm.id))
        .filter_map(|effect| match effect.modification {
            Modification::GainControl(player) => Some((effect.timestamp(), player)),
            Modification::PowerToughness { .. }
            | Modification::GrantKeyword(_)
            | Modification::GrantRestriction(_)
            // Losing abilities is layer 6 and never touches control: a permanent with
            // no abilities at all is still controlled by whoever controls it.
            | Modification::LoseKeyword(_)
            | Modification::LoseAllAbilities => None,
        })
        .max_by_key(|(timestamp, _)| *timestamp)
        .map_or(perm.controller, |(_, player)| player)
}

/// [`controller_of`] for a permanent named by id, or `None` when no permanent with that
/// id is on the battlefield — the shape a caller holding only a [`PermanentId`] wants.
#[must_use]
pub fn controller_of_id(state: &GameState, permanent: PermanentId) -> Option<PlayerId> {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == permanent)
        .map(|perm| controller_of(state, perm))
}
