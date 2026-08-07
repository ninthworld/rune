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
    /// CR 502.4, modified: the permanent **does not untap** during its controller's
    /// untap step — Waterknot's `enchanted creature doesn't untap during its controller's
    /// untap step`.
    ///
    /// Read in exactly one place, the untap step's turn-based action, and it is not a
    /// characteristic of any kind: the permanent is tapped or untapped exactly as it was,
    /// every selector that asks reads that, and only the one rule that would have untapped
    /// it reads this.
    ///
    /// Distinct from [`Permanent::skips_untap`](crate::Permanent::skips_untap), which is a
    /// one-shot flag a resolution sets and the next untap step spends. This is continuous:
    /// it lasts as long as whatever grants it, so a creature freed from the Aura untaps in
    /// the very next untap step with nothing to clear.
    DoesNotUntap,
    /// CR 121.2 / CR 614.1b, applied as a prohibition: counters **can't be put on** the
    /// object this effect applies to — Suncleanser's `it can't have counters put on it for
    /// as long as this creature remains on the battlefield`, and the same clause aimed at
    /// a player.
    ///
    /// Read in exactly one place per kind of thing it can be about:
    /// [`cannot_have_counters_put_on`](crate::characteristics::cannot_have_counters_put_on)
    /// for a permanent, and
    /// [`player_cannot_get_counters`](crate::characteristics::player_cannot_get_counters)
    /// for a player. Both readers sit behind the single counter seam
    /// (`GameState::put_counters_on_permanent` and its player-side twin), which is what
    /// makes this one fact rather than one per effect that puts a counter somewhere.
    ///
    /// A prohibition, not a removal: counters already there stay exactly where they are,
    /// and a permanent under it keeps whatever its counters were doing to its
    /// characteristics. It also does not stop counters being *removed* — the rule it
    /// modifies is the one that would have put them on.
    CannotHaveCountersPut,
    /// CR 614.1a, applied as a redirection: this permanent is **exiled instead of being
    /// put anywhere else** when it would leave the battlefield — Isareth the Awakener's
    /// `if that creature would leave the battlefield, exile it instead of putting it
    /// anywhere else`.
    ///
    /// Read at every zone seam that takes a permanent *off* the battlefield — the
    /// graveyard, the hand, the top of a library, a shuffle — so it covers dying, being
    /// destroyed, being bounced, and being tucked with one answer rather than four. The
    /// exile seam itself does not ask: a permanent already on its way to exile is going
    /// where this would send it.
    ///
    /// It replaces the *destination*, not the leaving. The permanent still leaves the
    /// battlefield, so everything that watches a permanent leave — a dies trigger, an
    /// Aura falling off — sees exactly what it saw before.
    ExiledInsteadOfLeavingBattlefield,
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
