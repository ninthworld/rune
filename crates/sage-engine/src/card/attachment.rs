//! Attachment: the one block an Aura and an Equipment share, and the one thing they
//! disagree about.

use serde::Deserialize;

use super::keyword::Keyword;
use super::restriction::CombatRestriction;
use crate::ability::{Ability, PermanentCount, TargetSpec};

/// Which kind of attachment a card is — the one field that is *not* shared between an
/// Aura and an Equipment (CR 303.4, CR 301.5).
///
/// Two things follow from it and nothing else does:
///
/// 1. **How it gets attached.** An Aura is cast at an object and enters already attached
///    to it (CR 303.4d), so its restriction is a slot on the *cast*; an Equipment enters
///    attached to nothing and is moved onto a creature by its equip ability (CR 702.6b),
///    so its restriction is a slot on that *activation*.
/// 2. **What happens when the host leaves.** An illegally-attached Aura is put into its
///    owner's graveyard (CR 704.5m); an Equipment merely becomes unattached and stays on
///    the battlefield (CR 704.5n).
///
/// Everything else — the CR 613 layer-6 and layer-7c grants — is the same for both, which
/// is exactly why they share one block rather than owning a field each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    /// An **Aura** (CR 303.4): an enchantment cast at the object it will enchant.
    Aura,
    /// An **Equipment** (CR 301.5): an artifact that equips a creature through its equip
    /// ability and outlives whatever it was attached to.
    Equipment,
}

impl AttachmentKind {
    /// The [`TargetSpec`] the CR 704.5m/704.5n state-based action judges a *host* by,
    /// given what the card authored as its [`Attachment::attach_to`].
    ///
    /// The two kinds differ, and the difference is a rule rather than an oversight. An
    /// Aura's enchant ability defines what it may legally *stay* attached to (CR 303.4a),
    /// so the authored spec answers both questions. An Equipment's `attach_to` is a
    /// restriction on the equip ability's target only — CR 301.5c says an Equipment may be
    /// attached to a creature and says nothing about whose — so "equip target creature you
    /// control" does **not** make an Equipment fall off a creature an opponent gains
    /// control of.
    #[must_use]
    pub fn host_legality(self, attach_to: TargetSpec) -> TargetSpec {
        match self {
            Self::Aura => attach_to,
            Self::Equipment => TargetSpec::AnyCreature,
        }
    }
}

/// The attachment ability and static grant of an Aura (CR 303.4) or an Equipment
/// (CR 301.5).
///
/// One block for both, because the *grant* is one thing: an attached permanent's
/// contribution to its host is read at CR 613 layer 6 (keywords and combat restrictions,
/// CR 613.1f) and layer 7c (power/toughness), and a creature holding an Equipment is
/// indistinguishable at both layers from one enchanted by an Aura. Splitting the grant
/// across two fields would mean every reader of a host's characteristics asked two
/// questions where the rules ask one, and could answer them differently.
///
/// The modification is stored as raw signed printed data; the *contribution* to a host's
/// current characteristics is derived on demand from the attachment via
/// [`characteristics`](crate::characteristics::characteristics), never stored (ADR 0005).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Attachment {
    /// Whether this is an Aura or an Equipment — see [`AttachmentKind`] for the two
    /// things that follow from it.
    pub kind: AttachmentKind,
    /// What this may be attached to: the [`TargetSpec`] a target is chosen for, at cast
    /// for an Aura (CR 303.4a/601.2c) and on the equip activation for an Equipment
    /// (CR 702.6b).
    ///
    /// It is *not* automatically the state-based action's legality test — that is
    /// [`AttachmentKind::host_legality`], which agrees with this for an Aura and
    /// deliberately does not for an Equipment.
    pub attach_to: TargetSpec,
    /// The **equip cost** (CR 702.6a), in the same curly-brace notation every other cost
    /// is written in — `"{2}"`. Present exactly on an Equipment, and the catalog
    /// validator enforces that in both directions
    /// ([`Violation::EquipCostMismatch`](crate::Violation)): an Equipment with no cost
    /// could never be attached to anything, and an Aura with one would advertise an
    /// activation the rules do not give it.
    ///
    /// The equip *ability* is derived from this rather than authored
    /// ([`equip_ability`](crate::card::equip_ability)), for the reason an Aura's cast
    /// target slot is derived: a card that could author the ability separately could
    /// author one that attaches something else.
    #[serde(default)]
    pub equip: Option<String>,
    /// The signed amount this adds to the attached object's power at CR 613 layer 7c.
    /// Negative shrinks (e.g. a `-2/-2` Aura). Defaults to `0`.
    ///
    /// With [`count_of`](Self::count_of) present this is the amount contributed **per
    /// counted permanent** rather than a flat one.
    #[serde(default)]
    pub power: i32,
    /// The signed amount this adds to the attached object's toughness at CR 613 layer 7c.
    /// Negative shrinks — enough can drop toughness to 0 or less and let the CR 704.5f
    /// state-based action put the host into its graveyard. Defaults to `0`.
    ///
    /// Per counted permanent when [`count_of`](Self::count_of) is present, exactly as
    /// [`Self::power`] is.
    #[serde(default)]
    pub toughness: i32,
    /// Which permanents [`power`](Self::power) and [`toughness`](Self::toughness) are
    /// multiplied by, if any — the `+1/+1 for each Forest you control` of a counted Aura.
    /// Absent is the ordinary flat grant.
    ///
    /// **This one is not fixed on resolution**, and that is the whole difference between
    /// it and [`Effect::PumpByCount`](crate::Effect::PumpByCount). A pump is a one-shot
    /// effect, so CR 608.2 takes its X once and the layer system folds a *fixed* modifier
    /// in forever after. This is a **static ability** (CR 604.3) whose continuous effect
    /// exists only while the attachment is attached, so its value is recalculated on every
    /// read of the host's characteristics: playing another Forest grows the grant, and
    /// losing one shrinks it. Deriving rather than storing is what the layer system
    /// already does with the flat grant — the count simply rides along.
    ///
    /// Because it *is* evaluated from inside the computation of a permanent's
    /// characteristics, it may not count by
    /// [`PermanentCount::min_power`](crate::PermanentCount::min_power): that field reads a
    /// computed power, which would ask the layer system for an answer it is in the middle
    /// of producing. The catalog validator refuses it
    /// ([`Violation::PowerInAttachmentCount`](crate::Violation::PowerInAttachmentCount)),
    /// as it refuses the same field in a static ability's condition.
    #[serde(default)]
    pub count_of: Option<PermanentCount>,
    /// The keyword abilities this grants the attached object at CR 613 layer 6
    /// (CR 613.1f) — e.g. an Aura granting flying, or an Equipment granting trample. Empty
    /// for a P/T-only attachment. Each granted keyword is folded into the host's computed
    /// keyword set while attached and is indistinguishable from a printed keyword; the
    /// grant vanishes the instant the attachment leaves *or is moved to another host*.
    /// Redundant grants are idempotent.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    /// Card types the attached object **gains** (CR 613 layer 4), in addition to its own.
    /// Empty for every attachment that changes only how big its host is.
    #[serde(default)]
    pub types: Vec<crate::card_type::CardType>,
    /// Subtypes the attached object gains — the `is a Knight in addition to its other
    /// types` of an Equipment that knights its bearer. Empty for most.
    #[serde(default)]
    pub subtypes: Vec<String>,
    /// The combat restrictions this imposes on the attached object at CR 613 layer 6
    /// (CR 613.1f) — the "can neither attack nor block" of a pacifism effect.
    /// Empty for an attachment that only pumps or only grants keywords.
    ///
    /// Folded into the host's computed restrictions exactly as [`Self::keywords`] are
    /// folded into its keywords, and derived from the attachment rather than stored, so
    /// the restriction ends the instant the attachment leaves — which is the whole way a
    /// pacified creature is freed by destroying the Aura.
    #[serde(default)]
    pub restrictions: Vec<CombatRestriction>,
    /// The **abilities** this grants the attached object at CR 613 layer 6 (CR 613.1f) —
    /// the `Enchanted land has "{T}: Add two mana of any one color."` of a land Aura, and
    /// the `Enchanted creature has "When this creature dies, draw a card."` of a creature
    /// one. Empty for an attachment that grants only keywords, restrictions, or P/T.
    ///
    /// **The same layer as [`Self::keywords`], and not a different kind of thing.** A
    /// keyword ability *is* an ability (CR 702.1); the two fields differ only in that a
    /// keyword is named by a word and this is written out in full. So the grant is folded
    /// into the host's computed ability set by the one accessor every collector reads
    /// ([`abilities_of_permanent`](crate::abilities_of_permanent)), which is what makes a
    /// granted activation offered by [`valid_actions`](crate::valid_actions), a granted
    /// mana ability still a mana ability that uses no stack (CR 605.1a), and a granted
    /// trigger fired by [`collect_triggers`](crate::collect_triggers) — each of them by
    /// the same code a printed ability goes through, with nothing that reads the list
    /// able to tell where a member came from.
    ///
    /// Derived on every read like the rest of the block, so the ability arrives with the
    /// attachment and is gone the instant it leaves or moves (ADR 0005). An ability the
    /// host already prints is granted again rather than collapsed: two copies of
    /// `{T}: Add {G}` are two activations, which is what a second Aura on the same land
    /// means.
    #[serde(default)]
    pub abilities: Vec<Ability>,
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
        // representative, so they are exercised inline (ADR 0009): one buffs (+2/+2),
        // one shrinks (-2/-2); both surface their enchant slot via cast_target_specs.
        let json = r#"[
            {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
             "attachment":{"kind":"aura","attach_to":"any_creature","power":2,"toughness":2}},
            {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
             "attachment":{"kind":"aura","attach_to":"any_creature","power":-2,"toughness":-2}}
        ]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();

        let aegis = crate::card::tests::card_named(&db, "test_aegis");
        assert_eq!(aegis.types, vec![CardType::Enchantment]);
        assert!(aegis.has_subtype("Aura"));
        assert_eq!(
            aegis.attachment,
            Some(Attachment {
                types: Vec::new(),
                subtypes: Vec::new(),
                kind: AttachmentKind::Aura,
                attach_to: TargetSpec::AnyCreature,
                equip: None,
                power: 2,
                toughness: 2,
                count_of: None,
                keywords: vec![],
                restrictions: vec![],
                abilities: vec![],
            })
        );
        // An Aura chooses its enchant target as it is cast (CR 601.2c): one slot.
        assert_eq!(
            aegis.cast_target_specs(None, 2),
            vec![TargetSpec::AnyCreature]
        );

        let curse = crate::card::tests::card_named(&db, "test_curse");
        assert!(curse.has_subtype("Aura"));
        assert_eq!(
            curse.attachment,
            Some(Attachment {
                types: Vec::new(),
                subtypes: Vec::new(),
                kind: AttachmentKind::Aura,
                attach_to: TargetSpec::AnyCreature,
                equip: None,
                power: -2,
                toughness: -2,
                count_of: None,
                keywords: vec![],
                restrictions: vec![],
                abilities: vec![],
            })
        );

        // A non-attachment card has no attachment block and no cast target slots.
        let bundled = crate::card::CardDatabase::bundled().unwrap();
        assert!(crate::card::tests::card_named(&bundled, "onakke_ogre")
            .attachment
            .is_none());
        assert!(crate::card::tests::card_named(&bundled, "onakke_ogre")
            .cast_target_specs(None, 2)
            .is_empty());
    }

    #[test]
    fn issue_728_an_equipment_carries_its_equip_cost_and_chooses_nothing_at_cast() {
        // CR 301.5 / 702.6b: an Equipment is cast like any other artifact — it enters
        // attached to nothing, so it fills no target slot at cast. Its restriction is a
        // slot on the equip *activation* instead, which is the whole difference between
        // the two kinds of attachment.
        let db = crate::card::CardDatabase::bundled().unwrap();
        let axe = crate::card::tests::card_named(&db, "marauder_s_axe");
        assert_eq!(axe.types, vec![CardType::Artifact]);
        assert!(axe.has_subtype("Equipment"));
        assert_eq!(
            axe.attachment,
            Some(Attachment {
                types: Vec::new(),
                subtypes: Vec::new(),
                kind: AttachmentKind::Equipment,
                attach_to: TargetSpec::AnyCreatureYouControl,
                equip: Some("{2}".to_string()),
                power: 2,
                toughness: 1,
                count_of: None,
                keywords: vec![],
                restrictions: vec![],
                abilities: vec![],
            })
        );
        assert!(
            axe.cast_target_specs(None, 2).is_empty(),
            "an Equipment names no target as it is cast"
        );
    }

    #[test]
    fn issue_728_only_an_aura_reads_its_authored_spec_as_the_host_test() {
        // CR 303.4a: an Aura's enchant ability says what it may *stay* on, so the authored
        // spec is the state-based action's test. CR 301.5c says only that an Equipment is
        // attached to a creature, so a possessive equip restriction does not follow the
        // Equipment onto the battlefield — a creature an opponent takes control of keeps
        // the sword.
        assert_eq!(
            AttachmentKind::Aura.host_legality(TargetSpec::AnyCreatureYouControl),
            TargetSpec::AnyCreatureYouControl
        );
        assert_eq!(
            AttachmentKind::Equipment.host_legality(TargetSpec::AnyCreatureYouControl),
            TargetSpec::AnyCreature
        );
    }

    #[test]
    fn issue_740_an_aura_may_name_a_land_as_its_host_and_grant_it_an_ability() {
        // CR 303.4a: the enchant restriction is whatever class the card names, so the
        // state-based action's host test is `any_land` and the cast slot is too. The
        // grant is a whole ability rather than a keyword, which the block carries beside
        // the keywords because a keyword ability is an ability (CR 702.1).
        use crate::ability::{Cost, Effect};
        let db = crate::card::CardDatabase::bundled().unwrap();

        let gift = crate::card::tests::card_named(&db, "gift_of_paradise");
        let grant = gift.attachment.as_ref().unwrap();
        assert_eq!(grant.attach_to, TargetSpec::AnyLand);
        assert_eq!(
            AttachmentKind::Aura.host_legality(grant.attach_to),
            TargetSpec::AnyLand
        );
        assert_eq!(gift.cast_target_specs(None, 2), vec![TargetSpec::AnyLand]);
        assert_eq!(
            grant.abilities,
            vec![Ability::Activated {
                once_each_turn: false,
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddManaAnyColor {
                    amount: 2,
                    same_color: true,
                    restriction: None,
                }],
                timing: crate::ability::ActivationTiming::AnyTime,
                condition: None,
            }]
        );
        assert!(
            crate::ability::is_mana_ability(&grant.abilities[0]),
            "what an Aura grants is judged by what it says, not by who granted it"
        );

        // The other half of the pair: a P/T grant and a triggered ability on one Aura,
        // which is the shape that proves the two fields are independent.
        let scarring = crate::card::tests::card_named(&db, "infernal_scarring");
        let grant = scarring.attachment.as_ref().unwrap();
        assert_eq!((grant.power, grant.toughness), (2, 0));
        assert_eq!(
            grant.abilities,
            vec![Ability::Triggered {
                event: crate::ability::TriggerCondition::SelfDies,
                effects: vec![Effect::DrawCard { count: 1 }],
            }]
        );
    }
}
