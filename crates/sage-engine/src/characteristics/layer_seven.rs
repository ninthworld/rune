//! CR 613 **layer 7**: power and toughness. A characteristic-defining ability sets the
//! base power at 7a, counters fold in at 7c, then simple static `+X/+Y` modifications
//! apply at that same layer in timestamp order (CR 613.7).
//!
//! The sublayers run in the order CR 613.4 gives them, which is the only order that makes
//! a `*` creature behave: 7a *replaces* the seed the printed card supplied, and 7c *adds*
//! to whatever 7a left. Nothing in between is implemented — no layer 7b effect sets a
//! base power, and no layer 7d switches power with toughness — so the two that exist are
//! applied one after the other with a hole where the other two will go.

use super::*;

/// The **layer 7a** power of `perm`: the number its characteristic-defining ability
/// (CR 604.3) says it is, or `None` when it has none and the printed seed stands.
///
/// Read from the permanent's *current* abilities rather than its printed ones, which is
/// what puts CR 613.4a in the right order relative to layer 6: an effect that made this
/// creature lose all its abilities took the defining ability with them, and a creature
/// with no defining ability is back to what its card printed. `abilities` is the list the
/// caller already computed for exactly that layer, passed in so it is not walked twice.
///
/// The count is taken **here, on this call**, and never stored — that is the whole
/// difference between a defining ability and an amount an effect fixed on resolution. A
/// card put into the graveyard changes the answer with no event in between.
pub(super) fn defined_power(
    state: &GameState,
    perm: &Permanent,
    abilities: &[Ability],
    db: &CardDatabase,
) -> Option<i32> {
    // "Your graveyard" is the graveyard of whoever controls the permanent *now*
    // (CR 613 layer 2), so a stolen Drake reads its thief's graveyard.
    let controller = controller_of(state, perm);
    abilities.iter().find_map(|ability| match ability {
        Ability::DefinedPower { count_of } => {
            let count = crate::condition::count_graveyard_cards(state, count_of, controller, db);
            Some(i32::try_from(count).unwrap_or(i32::MAX))
        }
        _ => None,
    })
}

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
/// synthesized fresh, each **attachment** currently on `perm` — an Aura (CR 303.4) or an
/// Equipment (CR 301.5), which contribute through the one
/// [`attachment_pt_effect`] because at this layer they are the same thing
/// (CR 613.7c). Those contributions are **derived, never stored**: the grant follows the
/// attachment, so it appears here exactly while the permanent is attached and vanishes the
/// instant it leaves *or is equipped onto someone else*, with nothing to prune (unlike a
/// keyed pump, which the SBA loop must clean up). A grant that scales with a count is
/// derived in the same breath — the count is read here, on this call, so it tracks the
/// board rather than the moment the Aura resolved. Object ids are unique, so timestamps do
/// not tie; the sort is stable regardless.
pub(super) fn ordered_pt_modifiers(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(state, effect, perm, is_creature))
        .cloned()
        .collect();
    // CR 303.4 / 301.5 / 613.7c: each attachment on `perm` contributes its static P/T
    // modifier, timestamped by the attachment's own object id (CR 613.7).
    for attachment in &state.battlefield {
        if attachment.attached_to == Some(perm.id) {
            if let Some(effect) = attachment_pt_effect(state, attachment, db) {
                if affects(state, &effect, perm, is_creature) {
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

/// The **base power and toughness** a continuous effect sets on `perm` (CR 613 layer 7b),
/// or `None` when none does.
///
/// The latest timestamp wins (CR 613.7): two effects each saying what this permanent's
/// base P/T *is* do not add up, and the later one is the answer. Today no card can produce
/// a second, so the ordering exists to make that determinate rather than incidental.
///
/// Read from the **stored** effects alone, for the reason layer 4 is: this runs from
/// inside the computation of `perm`'s characteristics, and a walk that asked every source
/// permanent for its abilities would not terminate.
#[must_use]
pub(super) fn set_base_pt(
    state: &GameState,
    perm: &Permanent,
    db: &CardDatabase,
) -> Option<(i32, i32)> {
    let mut ordered: Vec<&StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| stored_affects(state, effect, perm, db))
        .collect();
    ordered.sort_by_key(|effect| effect.timestamp());
    ordered
        .into_iter()
        .rev()
        .find_map(|effect| match effect.modification {
            Modification::SetBasePowerToughness { power, toughness } => Some((power, toughness)),
            _ => None,
        })
}

/// The card types and subtypes continuous effects **add** to `perm` (CR 613 layer 4).
///
/// Additive and order-independent — every printed card in this catalog says "in addition
/// to its other types", and nothing removes one — so unlike layer 7b there is no last-one-
/// wins to apply. Gathered from the stored effects and from the attachments on `perm`,
/// which is where an Equipment that makes its bearer a Knight lives.
#[must_use]
pub(super) fn added_types(
    state: &GameState,
    perm: &Permanent,
    db: &CardDatabase,
) -> (
    Vec<crate::card_type::CardType>,
    Vec<String>,
    Vec<crate::mana::Color>,
) {
    let mut types = Vec::new();
    let mut subtypes = Vec::new();
    let mut colors = Vec::new();
    let mut take = |modification: &Modification| {
        if let Modification::AddTypes {
            types: added,
            subtypes: added_subtypes,
            colors: added_colors,
        } = modification
        {
            types.extend(added.iter().copied());
            subtypes.extend(added_subtypes.iter().cloned());
            colors.extend(added_colors.iter().copied());
        }
    };
    for effect in &state.static_effects {
        if stored_affects(state, effect, perm, db) {
            take(&effect.modification);
        }
    }
    for attachment in &state.battlefield {
        if attachment.attached_to != Some(perm.id) {
            continue;
        }
        let Some(grant) = attachment
            .printed
            .face(db)
            .and_then(|face| face.attachment())
        else {
            continue;
        };
        if !grant.types.is_empty() || !grant.subtypes.is_empty() {
            take(&Modification::AddTypes {
                types: grant.types.clone(),
                subtypes: grant.subtypes.clone(),
                // No attachment in this catalog changes a colour; the field exists on the
                // modification rather than on the grant for that reason.
                colors: Vec::new(),
            });
        }
    }
    (types, subtypes, colors)
}

/// Whether a **stored** effect applies to `perm`, for the two layers that are folded
/// before the current types are known.
///
/// Only the keyed form is honoured — an effect naming this one permanent — because those
/// are the only ones these two layers are ever created as: a card animates *a* permanent
/// it targeted, never a class of them. A class-scoped effect is skipped rather than
/// guessed at, which is the conservative direction.
fn stored_affects(
    state: &GameState,
    effect: &StaticEffect,
    perm: &Permanent,
    db: &CardDatabase,
) -> bool {
    let _ = (state, db);
    matches!(
        effect.affects,
        crate::state::EffectAffects::SpecificPermanent(id) if id == perm.id
    )
}
