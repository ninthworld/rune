//! Continuous effects that modify a **rule** rather than a characteristic — the ones CR
//! 613 does not order, because there is no characteristic for a layer to be about.
//!
//! Deliberately **not** routed through [`characteristics`](super::characteristics), and
//! for a sharper reason than [`controller_of`](super::controller_of) is not: control is
//! at least a fact about the permanent that every later layer reads. A rule modification
//! is a fact about *one rule*. Folding it into [`Characteristics`](super::Characteristics)
//! would mean every reader of a permanent could see it, and the whole content of
//! [`RuleModification`] is that they cannot — a creature assigning combat damage by its
//! toughness has the power it always had, and a creature attacking as though it had no
//! defender has defender.
//!
//! So each rule asks its own question here, at the one place the rule is read. The two
//! questions share [`rule_modifications`], which gathers from the same two sources every
//! layer does: the stored [`GameState::static_effects`] and the printed static abilities
//! in force ([`static_ability_effects`]). Attachments are not a third source, because
//! [`Attachment`](crate::Attachment) has no field that could carry one; when a card prints
//! `equipped creature assigns combat damage by its toughness`, that is where it goes.
//!
//! Nothing here can recurse. `static_ability_effects` reads printed faces, stored
//! effects, and [`controller_of`](super::controller_of) — never a computed
//! characteristic — which is what makes it safe for the layer folds to call, and safe
//! here for the same reason.

use super::*;
use crate::card::{DamageCharacteristic, RuleModification};

/// Which characteristic the permanent identified by `permanent` assigns combat damage by
/// (CR 510.1a, as modified by [`RuleModification::AssignsCombatDamageBy`]).
///
/// [`DamageCharacteristic::Power`] unless a continuous effect says otherwise, which is
/// the unmodified rule rather than a fallback. When more than one effect applies the
/// **latest timestamp** wins (CR 613.7, the ordering every other continuous effect
/// follows); today no card can produce a second one, so the sort exists to make that
/// answer determinate rather than incidental.
///
/// A permanent not on the battlefield assigns by power, which is what the combat-damage
/// step's `0` for a missing permanent already means.
#[must_use]
pub fn assigns_combat_damage_by(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> DamageCharacteristic {
    rule_modifications(state, permanent, db)
        .into_iter()
        .filter_map(|modification| match modification {
            RuleModification::AssignsCombatDamageBy { characteristic } => Some(characteristic),
            RuleModification::AttacksAsThoughNoDefender => None,
        })
        .next_back()
        .unwrap_or_default()
}

/// Whether the permanent identified by `permanent` may be declared as an attacker as
/// though it did not have defender (CR 609.4 over CR 702.3b).
///
/// **An as-though permission, not a keyword removal**, and every consequence of that is
/// in what this function does *not* do: it does not touch
/// [`Characteristics::keywords`](super::Characteristics::keywords), so the creature still
/// has defender for the selector that granted the permission, for a card that counts
/// creatures with defender, and for the keyword line a client prints. The single reader
/// is [`defender_stops_attacking`](crate::combat::defender_stops_attacking).
///
/// It also permits exactly one thing. A creature that is tapped, summoning sick, or under
/// a [`CombatRestriction::CantAttack`](crate::CombatRestriction::CantAttack) still cannot
/// attack: those are other rules, and an as-though clause modifies the rule it names.
#[must_use]
pub fn attacks_as_though_no_defender(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> bool {
    rule_modifications(state, permanent, db).contains(&RuleModification::AttacksAsThoughNoDefender)
}

/// Every [`RuleModification`] currently applying to the permanent identified by
/// `permanent`, in ascending timestamp order (CR 613.7 — the source object's id), so a
/// reader that has to pick one picks the last.
///
/// Empty for a permanent that is not on the battlefield, or whose card is absent from
/// `db` — the same unknown-id fallback [`characteristics`](super::characteristics) gives,
/// and the same reason: there is no answer to compute and the engine forbids panicking.
fn rule_modifications(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> Vec<RuleModification> {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return Vec::new();
    };
    let Some(face) = perm.printed.face(db) else {
        return Vec::new();
    };
    // The same gate the layer folds pass to the selector: an "each creature you control"
    // scope is about creatures, and current type is printed type until layers 1–5 land.
    let is_creature = face.has_type(CardType::Creature);
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(state, effect, perm, is_creature))
        .cloned()
        .collect();
    effects.extend(static_ability_effects(state, perm, is_creature, db));
    effects.sort_by_key(StaticEffect::timestamp);
    effects
        .into_iter()
        .filter_map(|effect| match effect.modification {
            Modification::ModifyRule(modification) => Some(modification),
            // Every other modification is in a layer, and is read from the computed
            // characteristics rather than from here.
            Modification::PowerToughness { .. }
            | Modification::GrantKeyword(_)
            | Modification::LoseKeyword(_)
            | Modification::LoseAllAbilities
            | Modification::GrantRestriction(_)
            | Modification::GainControl(_) => None,
        })
        .collect()
}
