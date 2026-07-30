//! Aura ability and static power/toughness grant.

use serde::Deserialize;

use super::keyword::Keyword;
use crate::ability::TargetSpec;

/// The enchant ability and static power/toughness grant of an Aura (CR 303.4).
///
/// An Aura is an Enchantment that enters the battlefield attached to another
/// object (CR 303.4). This value bundles the two things the engine needs to model
/// one at the scope of issue #152: its **enchant restriction** (CR 303.4a) — the
/// [`TargetSpec`] the Aura chooses a target for as it is cast (CR 601.2c) and the
/// class of object it may legally stay attached to — and the continuous
/// power/toughness modification it applies to that object at CR 613 layer 7c.
///
/// The modification is stored as raw signed printed data; the *contribution* to a
/// host's current characteristics is derived on demand from the attachment via
/// [`characteristics`](crate::characteristics::characteristics), never stored
/// (ADR 0010). Enchant-creature Auras that grant power/toughness (CR 613.7c) and/or
/// keyword abilities (CR 613.1f, layer 6) are modeled here; enchant-player/land and
/// Aura movement are out of scope.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AuraGrant {
    /// The enchant restriction (CR 303.4a): what this Aura may be attached to,
    /// expressed as the [`TargetSpec`] a target is chosen for at cast (CR 601.2c)
    /// and re-checked by the CR 704.5m state-based action while it stays attached.
    pub enchant: TargetSpec,
    /// The signed amount this Aura adds to the enchanted object's power at CR 613
    /// layer 7c. Negative shrinks (e.g. a `-2/-2` Aura). Defaults to `0`.
    #[serde(default)]
    pub power: i32,
    /// The signed amount this Aura adds to the enchanted object's toughness at CR
    /// 613 layer 7c. Negative shrinks — enough can drop toughness to 0 or less and
    /// let the CR 704.5f state-based action put the host into its graveyard.
    /// Defaults to `0`.
    #[serde(default)]
    pub toughness: i32,
    /// The keyword abilities this Aura grants the enchanted object at CR 613 layer 6
    /// (CR 613.1f) — e.g. an Aura granting flying. Empty for a P/T-only Aura. Each
    /// granted keyword is folded into the host's computed keyword set while the Aura
    /// is attached and is indistinguishable from a printed keyword; the grant
    /// vanishes the instant the Aura leaves. Redundant grants are idempotent.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::card_type::CardType;

    #[test]
    fn issue_152_aura_fixtures_carry_their_enchant_and_pt_grant() {
        // CR 303.4: an Aura is an Enchantment — Aura card carrying an enchant-creature
        // restriction and a static P/T grant. P/T-only Auras have no clean M19
        // representative, so they are exercised inline (ADR 0026): one buffs (+2/+2),
        // one shrinks (-2/-2); both surface their enchant slot via cast_target_specs.
        let json = r#"[
            {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
             "aura":{"enchant":"any_creature","power":2,"toughness":2}},
            {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
             "aura":{"enchant":"any_creature","power":-2,"toughness":-2}}
        ]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();

        let aegis = crate::card::tests::card_named(&db, "test_aegis");
        assert_eq!(aegis.types, vec![CardType::Enchantment]);
        assert!(aegis.has_subtype("Aura"));
        assert_eq!(
            aegis.aura,
            Some(AuraGrant {
                enchant: TargetSpec::AnyCreature,
                power: 2,
                toughness: 2,
                keywords: vec![],
            })
        );
        // An Aura chooses its enchant target as it is cast (CR 601.2c): one slot.
        assert_eq!(aegis.cast_target_specs(), vec![TargetSpec::AnyCreature]);

        let curse = crate::card::tests::card_named(&db, "test_curse");
        assert!(curse.has_subtype("Aura"));
        assert_eq!(
            curse.aura,
            Some(AuraGrant {
                enchant: TargetSpec::AnyCreature,
                power: -2,
                toughness: -2,
                keywords: vec![],
            })
        );

        // A non-Aura card has no aura ability and no cast target slots.
        let bundled = crate::card::CardDatabase::bundled().unwrap();
        assert!(crate::card::tests::card_named(&bundled, "onakke_ogre")
            .aura
            .is_none());
        assert!(crate::card::tests::card_named(&bundled, "onakke_ogre")
            .cast_target_specs()
            .is_empty());
    }
}
