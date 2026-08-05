//! CR 613 **layer 7**: power and toughness. Counters fold in at 7c, then simple static
//! `+X/+Y` modifications apply at that same layer in timestamp order (CR 613.7).

use super::*;

/// The net power/toughness shift from `perm`'s `+1/+1` and `-1/-1` counters at
/// CR 613 layer 7c: one `+1/+1` counter contributes `+1`, one `-1/-1` counter
/// `-1`, and the kinds sum independently (they do not annihilate here — that is
/// the `+1/+1`/`-1/-1` state-based action, out of this slice's scope).
///
/// Counts are `u32`; conversion saturates at [`i32::MAX`] rather than panic
/// (the engine forbids panicking APIs), which no realistic game ever reaches.
pub(super) fn pt_counter_delta(perm: &Permanent) -> i32 {
    let plus = i32::try_from(perm.counter_count(CounterKind::PlusOnePlusOne)).unwrap_or(i32::MAX);
    let minus =
        i32::try_from(perm.counter_count(CounterKind::MinusOneMinusOne)).unwrap_or(i32::MAX);
    plus.saturating_sub(minus)
}

/// The net layer-7c power/toughness shift on `perm` from continuous static
/// effects (anthems, pumps), applied **after** counters in timestamp order
/// (CR 613.7, ADR 0005 §3–§4). Returns `(power_delta, toughness_delta)`.
///
/// These modifiers are additive, so their sum is order-independent
/// arithmetically; the engine still folds them in ascending timestamp order so
/// the pipeline is deterministic and stays correct as order-sensitive effects
/// (set P/T, characteristic-defining abilities) land in later slices.
/// `is_creature` gates the anthem-style "creatures you control" selector.
///
/// Overflow saturates rather than panicking, matching
/// [`pt_counter_delta`] and the engine's no-panic rule.
pub(super) fn static_pt_delta(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> (i32, i32) {
    let mut power = 0_i32;
    let mut toughness = 0_i32;
    for effect in ordered_pt_modifiers(state, perm, is_creature, db) {
        // Only layer-7c P/T modifications adjust power/toughness; a layer-6
        // keyword grant that also happens to affect `perm` is skipped here.
        if let Modification::PowerToughness {
            power: dp,
            toughness: dt,
        } = effect.modification
        {
            power = power.saturating_add(dp);
            toughness = toughness.saturating_add(dt);
        }
    }
    (power, toughness)
}

/// The layer-7c static P/T effects that apply to `perm`, sorted by timestamp
/// (ascending [`StaticEffect::timestamp`], i.e. source object id).
///
/// Two sources feed this one list, folded through the same timestamp-ordered path
/// (ADR 0005 §4): the stored [`GameState::static_effects`] (anthems and pumps) and,
/// synthesized fresh, each Aura currently attached to `perm` (CR 303.4 / 613.7c) —
/// see [`aura_pt_effect`]. The Aura contributions are **derived, never stored**: an
/// Aura's P/T grant follows its attachment, so it appears here exactly while the
/// Aura is attached and vanishes the instant it leaves, with nothing to prune
/// (unlike a keyed pump, which the SBA loop must clean up). Object ids are unique,
/// so timestamps do not tie; the sort is stable regardless.
pub(super) fn ordered_pt_modifiers(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(effect, perm, is_creature))
        .copied()
        .collect();
    // CR 303.4 / 613.7c: each Aura attached to `perm` contributes its static P/T
    // modifier, timestamped by the Aura's own object id (CR 613.7).
    for aura in &state.battlefield {
        if aura.attached_to == Some(perm.id) {
            if let Some(effect) = aura_pt_effect(aura, db) {
                if affects(&effect, perm, is_creature) {
                    effects.push(effect);
                }
            }
        }
    }
    // CR 604.3 / 613.7c: every printed static ability in force, from its source's
    // battlefield presence alone.
    effects.extend(static_ability_effects(state, perm, is_creature, db));
    effects.sort_by_key(StaticEffect::timestamp);
    effects
}
