//! Functional definition of a card.

use serde::Deserialize;

use super::aura::AuraGrant;
use super::keyword::Keyword;
use crate::ability::{Ability, Effect, TargetSpec};
use crate::card_type::{CardType, Supertype};
use crate::id::FunctionalId;
use crate::mana::Color;

/// One functional definition: the static, printing-independent rules object for a
/// card (ADR 0018 §2).
///
/// This is the immutable data the engine reasons about. It holds no zone, no
/// battlefield identity, and no per-game state — those live on
/// [`crate::GameState`]. Current characteristics (after continuous effects) are
/// computed by the layer system, never stored here.
///
/// `deny_unknown_fields` is what keeps the schema *functional*: an upstream
/// presentation asset — `flavor_text`, `image_uris`, `artist`, a frame or watermark
/// — is a parse error rather than a silently ignored field, so no such data can
/// enter the catalog by accident (ADR 0018 §2, `docs/brief.md` Legal
/// Considerations). It is also why this type, not a wrapper, is the direct
/// deserialization target: serde does not enforce `deny_unknown_fields` through a
/// `flatten`ed field.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardData {
    /// The schema version this definition is authored against; must be
    /// [`SCHEMA_VERSION`](super::super::SCHEMA_VERSION) (ADR 0018 §2).
    pub schema_version: u32,
    /// This definition's authored, stable identity (ADR 0018 §3) — what printings
    /// and decklists reference, and what survives a rebuild, unlike the [`CardId`](crate::id::CardId)
    /// it is interned to.
    pub functional_id: FunctionalId,
    /// The card's name (e.g. `"Thornback Boar"`).
    pub name: String,
    /// Printed supertypes (e.g. `Basic`, `Legendary`); empty for most cards. Part
    /// of the structured type line the engine reasons about — the display string
    /// is rendered by [`CardData::type_line`], never parsed back.
    #[serde(default)]
    pub supertypes: Vec<Supertype>,
    /// Printed card types (e.g. `Creature`, `Land`). Every card has at least one.
    pub types: Vec<CardType>,
    /// Printed subtypes (e.g. `"Elf"`, `"Scout"`, `"Forest"`); empty for many
    /// cards. Open-ended, so kept as strings rather than an enum.
    #[serde(default)]
    pub subtypes: Vec<String>,
    /// The mana cost in curly-brace notation (e.g. `"{2}{G}"`); empty for cards
    /// with no mana cost, such as basic lands.
    pub mana_cost: String,
    /// The card's colors (CR 105.2); empty for a colorless card.
    ///
    /// Authored explicitly rather than re-derived by parsing [`Self::mana_cost`]'s
    /// pips (ADR 0018 §2) — the same "structured, never parsed back" discipline
    /// [`CardData::type_line`] uses. A colorless-cost-but-colored card is therefore
    /// representable without the cost string having to imply it.
    #[serde(default)]
    pub colors: Vec<Color>,
    /// Printed power, for creatures; `None` for non-creatures.
    #[serde(default)]
    pub power: Option<i32>,
    /// Printed toughness, for creatures; `None` for non-creatures.
    #[serde(default)]
    pub toughness: Option<i32>,
    /// The card's abilities as declarative data. Empty for vanilla cards. Cards
    /// whose behavior the data IR cannot express instead register abilities in
    /// [`crate::scripted`]; use [`crate::card::abilities_of`] to read both sources together.
    #[serde(default)]
    pub abilities: Vec<Ability>,
    /// The effects this card's **spell ability** produces on resolution — the
    /// instant/sorcery analogue of an ability's effects (CR 608.2c). Empty for a
    /// vanilla card and for a permanent spell whose only "effect" is entering the
    /// battlefield. A targeting spell effect declares its [`TargetSpec`]
    /// here exactly as an ability's effect does, so a spell chooses targets as it
    /// is cast (CR 601.2c); read them with [`crate::card::spell_effects_of`].
    #[serde(default)]
    pub spell_effects: Vec<Effect>,
    /// The Aura ability of an Aura card (CR 303.4): its enchant restriction and
    /// static power/toughness grant. `None` for every non-Aura card. When present,
    /// the card is castable only with a legal enchant target (CR 303.4c/601.2c),
    /// enters attached to that target (CR 303.4d), and contributes its P/T grant to
    /// the host while attached (CR 613.7c) — see [`AuraGrant`].
    #[serde(default)]
    pub aura: Option<AuraGrant>,
    /// The card's printed keyword abilities (CR 702), e.g. flying or haste. Empty
    /// for a card with none. Read with [`CardData::has_keyword`] for the *printed*
    /// set; a permanent's *current* keywords fold these together with any granted by
    /// continuous effects (CR 613.1f, layer 6) via
    /// [`characteristics`](crate::characteristics::characteristics), which the combat
    /// and summoning-sickness code consults.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
    /// Whether this card's behavior is (also) defined in code rather than data
    /// (ADR 0018 §2; the escape hatch of ADR 0007).
    ///
    /// `true` means [`crate::scripted`] carries an arm for this definition's
    /// interned [`CardId`](crate::id::CardId). Authored explicitly so the two tiers are declared, not
    /// inferred: today the flag is `false` on every bundled card, and no card's
    /// behavior lives in code.
    #[serde(default)]
    pub scripted: bool,
}

impl CardData {
    /// Render the printed type line for display, e.g. `"Basic Land — Forest"` or
    /// `"Creature — Elf Scout"`. Supertypes and types are joined with spaces;
    /// subtypes, if any, follow an em dash. This is the single source for the
    /// display string — it is never parsed back into types.
    #[must_use]
    pub fn type_line(&self) -> String {
        let mut head: Vec<&str> = Vec::new();
        head.extend(self.supertypes.iter().map(|s| s.display()));
        head.extend(self.types.iter().map(|t| t.display()));
        let mut line = head.join(" ");
        if !self.subtypes.is_empty() {
            line.push_str(" — ");
            line.push_str(&self.subtypes.join(" "));
        }
        line
    }

    /// Whether the card has printed card type `card_type`.
    #[must_use]
    pub fn has_type(&self, card_type: CardType) -> bool {
        self.types.contains(&card_type)
    }

    /// Whether this is a permanent card — one that enters the battlefield when
    /// it resolves. True when any printed [`CardType`] is a permanent type
    /// (land, creature, artifact, enchantment, planeswalker, or battle); false
    /// for an instant/sorcery-only card, which resolves to a graveyard instead
    /// (CR 608.3). Keyed off the structured types, never a parsed string, and
    /// matched exhaustively so a new [`CardType`] must be classified here.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.types.iter().any(|t| match t {
            CardType::Land
            | CardType::Creature
            | CardType::Artifact
            | CardType::Enchantment
            | CardType::Planeswalker
            | CardType::Battle => true,
            CardType::Instant | CardType::Sorcery => false,
        })
    }

    /// Whether the card has printed subtype `subtype` (case-sensitive, as printed).
    #[must_use]
    pub fn has_subtype(&self, subtype: &str) -> bool {
        self.subtypes.iter().any(|s| s == subtype)
    }

    /// The ordered [`TargetSpec`]s a player chooses a target for when **casting**
    /// this card as a spell (CR 601.2c), in slot order.
    ///
    /// An Aura contributes its enchant restriction (CR 303.4a) as the first slot —
    /// the object it will be attached to — and every card contributes the target
    /// specs of its spell-ability effects ([`Self::spell_effects`]). The two are
    /// disjoint in practice (an Aura has no spell effects), but both are honored so
    /// a single accessor drives casting legality ([`crate::valid_actions`]), the
    /// per-slot candidate enumeration, and the on-resolution fizzle re-check
    /// (CR 608.2b). Empty for a spell that chooses no targets.
    #[must_use]
    pub fn cast_target_specs(&self) -> Vec<TargetSpec> {
        let mut specs: Vec<TargetSpec> =
            self.aura.as_ref().map(|a| a.enchant).into_iter().collect();
        specs.extend(self.spell_effects.iter().filter_map(Effect::target_spec));
        specs
    }

    /// Whether the card has printed keyword ability `keyword` (CR 702). Reads only
    /// the printed [`CardData::keywords`]. A permanent's *current* keywords also
    /// include those granted by continuous effects at CR 613 layer 6; read those
    /// through [`characteristics`](crate::characteristics::characteristics).
    #[must_use]
    pub fn has_keyword(&self, keyword: Keyword) -> bool {
        self.keywords.contains(&keyword)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::card_type::Supertype;

    #[test]
    fn known_id_resolves_to_expected_characteristics() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        let ogre = crate::card::tests::card_named(&db, "onakke_ogre");
        assert_eq!(ogre.name, "Onakke Ogre");
        assert_eq!(ogre.types, vec![CardType::Creature]);
        assert_eq!(
            ogre.subtypes,
            vec!["Ogre".to_string(), "Warrior".to_string()]
        );
        assert_eq!(ogre.type_line(), "Creature — Ogre Warrior");
        assert_eq!(ogre.mana_cost, "{2}{R}");
        assert_eq!(ogre.power, Some(4));
        assert_eq!(ogre.toughness, Some(2));
    }

    #[test]
    fn basic_land_has_no_power_or_toughness() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        let forest = crate::card::tests::card_named(&db, "forest");
        assert_eq!(forest.name, "Forest");
        assert_eq!(forest.supertypes, vec![Supertype::Basic]);
        assert_eq!(forest.types, vec![CardType::Land]);
        assert_eq!(forest.type_line(), "Basic Land — Forest");
        assert_eq!(forest.mana_cost, "");
        assert_eq!(forest.power, None);
        assert_eq!(forest.toughness, None);
    }

    #[test]
    fn type_line_renders_supertypes_types_and_subtypes() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        // Multiple subtypes are space-joined after the em dash.
        assert_eq!(
            crate::card::tests::card_named(&db, "tolarian_scholar").type_line(),
            "Creature — Human Wizard"
        );
        // A supertype precedes the card type; the land subtype follows the dash.
        assert_eq!(
            crate::card::tests::card_named(&db, "forest").type_line(),
            "Basic Land — Forest"
        );
    }

    #[test]
    fn has_type_and_has_subtype_query_structured_types() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        let elves = crate::card::tests::card_named(&db, "llanowar_elves");
        assert!(elves.has_type(CardType::Creature));
        assert!(!elves.has_type(CardType::Land));
        assert!(elves.has_subtype("Elf"));
        assert!(!elves.has_subtype("Goblin"));
    }

    #[test]
    fn is_permanent_splits_permanent_types_from_instants_and_sorceries() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        // Creature and land are permanent cards.
        assert!(crate::card::tests::card_named(&db, "onakke_ogre").is_permanent());
        assert!(crate::card::tests::card_named(&db, "forest").is_permanent());
        // An instant-only card is not.
        let json = r#"[{"schema_version":1,"functional_id":"test_bolt","name":"Test Bolt","types":["instant"],"mana_cost":"{R}"}]"#;
        let bolt = crate::card::CardDatabase::from_json(json).unwrap();
        assert!(!crate::card::tests::card_named(&bolt, "test_bolt").is_permanent());
    }

    #[test]
    fn vanilla_cards_deserialize_with_no_abilities() {
        let db = crate::card::CardDatabase::bundled().unwrap();
        assert!(crate::card::tests::card_named(&db, "onakke_ogre")
            .abilities
            .is_empty());
    }

    #[test]
    fn colors_are_authored_not_derived_from_the_cost() {
        // ADR 0018 §2: colors are an explicit field. For the current fixtures they
        // agree with the pips of their cost (this test is that authoring check), but
        // nothing derives them at runtime — so a card whose colors do not follow from
        // its cost is representable.
        let db = crate::card::CardDatabase::bundled().unwrap();

        for id in crate::card::tests::every_id() {
            let card = db.card(id).unwrap();
            let cost = crate::mana::parse_mana_cost(&card.mana_cost);
            let from_pips: Vec<Color> = [
                (cost.white, Color::White),
                (cost.blue, Color::Blue),
                (cost.black, Color::Black),
                (cost.red, Color::Red),
                (cost.green, Color::Green),
            ]
            .into_iter()
            .filter(|(pips, _)| *pips > 0)
            .map(|(_, color)| color)
            .collect();
            assert_eq!(
                card.colors, from_pips,
                "{}'s authored colors disagree with its cost",
                card.name
            );
        }

        // A colorless cost with an authored color — the case pip-parsing could not
        // express — round-trips.
        let json = r#"[{"schema_version":1,"functional_id":"void_thing","name":"Void Thing",
                        "types":["creature"],"mana_cost":"{2}","colors":["black"],
                        "power":2,"toughness":2}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        assert_eq!(
            crate::card::tests::card_named(&db, "void_thing").colors,
            vec![Color::Black]
        );
    }
}
