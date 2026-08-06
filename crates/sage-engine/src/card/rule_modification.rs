//! Continuous effects that modify a **rule** rather than a characteristic (CR 613.1,
//! CR 609.4).

use serde::Deserialize;

/// A continuous effect that changes how a rule reads a permanent, without changing any
/// characteristic of it.
///
/// **The third vocabulary, and the one that is in no layer.** [`Keyword`](super::Keyword)
/// and [`CombatRestriction`](super::CombatRestriction) are both CR 613 layer 6: they are
/// folded into the permanent's computed
/// [`Characteristics`](crate::Characteristics) and read back from there. Nothing here is.
/// CR 613 orders effects that change *characteristics*, and a rule modification changes
/// none — so it is applied nowhere and simply **read where the rule it modifies is
/// asked**, exactly as an [`Ability::PlayerStatic`](crate::Ability::PlayerStatic) is.
///
/// That distinction is the whole point of the enum rather than a nicety:
///
/// - [`Self::AssignsCombatDamageBy`] is **not** a power-setting effect. A creature under
///   it has exactly the power it had — every evasion rule, every selector, and the
///   projected view keep reading that power — and only the one line of CR 510.1a that
///   asks *how much combat damage does this creature assign* reads anything else. A
///   layer-7b `set power to toughness` would be a different card, visible to all of
///   them.
/// - [`Self::AttacksAsThoughNoDefender`] is **not**
///   [`Modification::LoseKeyword`](crate::Modification::LoseKeyword). CR 609.4's "as
///   though" lets one rule pretend, and changes nothing else: the creature still *has*
///   defender, so a card that asks for "each creature you control with defender" still
///   finds it — including the very card that granted the permission, which is what makes
///   Arcades work at all. Removing the keyword would take the creature out of its own
///   benefactor's class.
///
/// A closed, plain-data enum (ADR 0003). It grows by adding a variant, and each variant
/// names the single rule it modifies and the single place that rule is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleModification {
    /// CR 510.1a, modified: the creature assigns combat damage equal to the named
    /// characteristic instead of to its power — Arcades' `assigns combat damage equal to
    /// its toughness rather than its power`.
    ///
    /// Read in exactly one place,
    /// [`assigned_combat_damage`](crate::combat::assigned_combat_damage), which is the
    /// one function the combat-damage step asks "how much does this creature assign".
    /// Because it is that number the step spreads across blockers, both consumers of the
    /// assignment follow with nothing to say about it: trample's excess (CR 702.19e) is
    /// what is left of it after each blocker's lethal, and the marked damage the
    /// lethal-damage state-based action reads (CR 704.5g) is what of it was dealt.
    ///
    /// Nothing here reads *another* object: the characteristic named is the assigning
    /// creature's own, computed through the same [`Characteristics`](crate::Characteristics)
    /// its power would have been read from, so counters and anthems fold into the
    /// substitute exactly as they fold into the original.
    AssignsCombatDamageBy {
        /// Which of the creature's own characteristics the amount is read from.
        characteristic: DamageCharacteristic,
    },
    /// CR 702.3b, applied **as though** it were absent (CR 609.4): the creature may be
    /// declared as an attacker even though it has defender — Novice Knight's `can attack
    /// as though it didn't have defender`.
    ///
    /// Read in exactly one place,
    /// [`defender_stops_attacking`](crate::combat::defender_stops_attacking), which is
    /// the gate [`attacker_candidates`](crate::combat::attacker_candidates) asks about
    /// the keyword. Every other reader of the keyword — a selector naming creatures with
    /// defender, the printed keyword line, the computed
    /// [`Characteristics::keywords`](crate::Characteristics::keywords) — is untouched,
    /// because the keyword is untouched.
    ///
    /// It permits nothing else. [`CombatRestriction::CantAttack`](super::CombatRestriction::CantAttack)
    /// forbids attacking for a different reason and is not lifted by this; nor is
    /// summoning sickness, nor being tapped.
    AttacksAsThoughNoDefender,
}

/// Which characteristic a creature's combat damage is read from (CR 510.1a).
///
/// [`Self::Power`] is what the rule says with nothing modifying it, and is therefore the
/// answer [`assigned_combat_damage`](crate::combat::assigned_combat_damage) gives when no
/// [`RuleModification::AssignsCombatDamageBy`] applies — a default rather than a
/// sentinel, so the one reader has a single code path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageCharacteristic {
    /// Power — the unmodified rule (CR 510.1a).
    #[default]
    Power,
    /// Toughness — Arcades' substitution.
    Toughness,
}
