//! Card data: resolving a [`CardId`] to its immutable characteristics.
//!
//! The engine performs no I/O. The card snapshot is a JSON file embedded at
//! compile time with [`include_str!`] and parsed in memory; there is no
//! filesystem or network access here. serde is permitted for exactly this
//! purpose — see `docs/decisions/0006-serde-in-engine.md`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::ability::Ability;
use crate::card_type::{CardType, Supertype};
use crate::id::CardId;
use crate::scripted::scripted_abilities;

/// The bundled card snapshot, embedded at compile time.
///
/// Deliberately tiny and hand-authored: a handful of vanilla creatures and one
/// basic land. Only non-infringing data — names, type lines, mana costs, oracle
/// text, power/toughness — with no card images, official frames, or WotC
/// branding (crate `AGENTS.md`, `docs/brief.md` Legal Considerations).
const BUNDLED_SNAPSHOT: &str = include_str!("../data/cards.json");

/// The static, printing-independent characteristics of a card.
///
/// This is the immutable "oracle" data the engine reasons about today. It holds
/// no zone, no battlefield identity, and no per-game state — those live on
/// [`crate::GameState`]. Current characteristics (after continuous effects) are
/// computed by the layer system, never stored here.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CardData {
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
    /// The rules text as printed. Empty for vanilla cards.
    pub oracle_text: String,
    /// Printed power, for creatures; `None` for non-creatures.
    #[serde(default)]
    pub power: Option<i32>,
    /// Printed toughness, for creatures; `None` for non-creatures.
    #[serde(default)]
    pub toughness: Option<i32>,
    /// The card's abilities as declarative data. Empty for vanilla cards. Cards
    /// whose behavior the data IR cannot express instead register abilities in
    /// [`crate::scripted`]; use [`abilities_of`] to read both sources together.
    #[serde(default)]
    pub abilities: Vec<Ability>,
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
}

/// One entry in the JSON snapshot: a [`CardId`] paired with its [`CardData`].
#[derive(Deserialize)]
struct CardEntry {
    /// The id this entry is keyed by.
    id: u64,
    /// The card's characteristics.
    #[serde(flatten)]
    data: CardData,
}

/// An immutable, `CardId`-keyed database of card characteristics.
///
/// Built from a JSON snapshot (the bundled one via [`CardDatabase::bundled`], or
/// any snapshot via [`CardDatabase::from_json`]). Lookups are pure: the database
/// never mutates and holds no game state.
#[derive(Clone, Debug, Default)]
pub struct CardDatabase {
    cards: HashMap<CardId, CardData>,
}

impl CardDatabase {
    /// Load the database from the compile-time-embedded snapshot.
    ///
    /// # Errors
    /// Returns the underlying [`serde_json::Error`] if the embedded snapshot
    /// fails to parse. The snapshot is committed and tested, so this is not
    /// expected in practice; it is surfaced rather than panicked on because the
    /// engine forbids panicking APIs.
    pub fn bundled() -> Result<Self, serde_json::Error> {
        Self::from_json(BUNDLED_SNAPSHOT)
    }

    /// Parse a JSON snapshot (an array of card entries) into a database.
    ///
    /// # Errors
    /// Returns the underlying [`serde_json::Error`] if `json` is not a valid
    /// snapshot.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let entries: Vec<CardEntry> = serde_json::from_str(json)?;
        let cards = entries
            .into_iter()
            .map(|entry| (CardId(entry.id), entry.data))
            .collect();
        Ok(Self { cards })
    }

    /// Resolve a [`CardId`] to its characteristics, or `None` if the id is not
    /// in the database.
    #[must_use]
    pub fn card(&self, id: CardId) -> Option<&CardData> {
        self.cards.get(&id)
    }

    /// The number of cards in the database.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// Whether the database holds no cards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

/// All abilities of a card: its data-driven [`CardData::abilities`] plus any
/// code-defined ones from [`crate::scripted`].
///
/// Returns an empty list if the id is unknown and has no scripted abilities. This
/// is the single accessor the pipeline uses so both authoring tiers are always
/// considered together.
#[must_use]
pub fn abilities_of(db: &CardDatabase, card: CardId) -> Vec<Ability> {
    let mut abilities = db
        .card(card)
        .map(|c| c.abilities.clone())
        .unwrap_or_default();
    abilities.extend(scripted_abilities(card));
    abilities
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::ability::{Effect, TriggerCondition};
    use crate::card_type::{CardType, Supertype};

    #[test]
    fn bundled_snapshot_parses() {
        let db = CardDatabase::bundled().unwrap();
        assert!(!db.is_empty());
        assert_eq!(db.len(), 6);
    }

    #[test]
    fn known_id_resolves_to_expected_characteristics() {
        let db = CardDatabase::bundled().unwrap();
        let boar = db.card(CardId(1)).unwrap();
        assert_eq!(boar.name, "Thornback Boar");
        assert_eq!(boar.types, vec![CardType::Creature]);
        assert_eq!(boar.subtypes, vec!["Boar".to_string()]);
        assert_eq!(boar.type_line(), "Creature — Boar");
        assert_eq!(boar.mana_cost, "{2}{G}");
        assert_eq!(boar.oracle_text, "");
        assert_eq!(boar.power, Some(3));
        assert_eq!(boar.toughness, Some(2));
    }

    #[test]
    fn basic_land_has_no_power_or_toughness() {
        let db = CardDatabase::bundled().unwrap();
        let forest = db.card(CardId(5)).unwrap();
        assert_eq!(forest.name, "Forest");
        assert_eq!(forest.supertypes, vec![Supertype::Basic]);
        assert_eq!(forest.types, vec![CardType::Land]);
        assert_eq!(forest.type_line(), "Basic Land — Forest");
        assert_eq!(forest.mana_cost, "");
        assert_eq!(forest.power, None);
        assert_eq!(forest.toughness, None);
    }

    #[test]
    fn unknown_id_resolves_to_none() {
        let db = CardDatabase::bundled().unwrap();
        assert!(db.card(CardId(9999)).is_none());
    }

    #[test]
    fn from_json_rejects_malformed_input() {
        assert!(CardDatabase::from_json("not json").is_err());
    }

    #[test]
    fn from_json_parses_a_minimal_snapshot() {
        let json = r#"[{"id":42,"name":"Test Wisp","types":["creature"],"subtypes":["Spirit"],"mana_cost":"{U}","oracle_text":"","power":1,"toughness":1}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        assert_eq!(db.len(), 1);
        let wisp = db.card(CardId(42)).unwrap();
        assert_eq!(wisp.name, "Test Wisp");
        assert_eq!(wisp.type_line(), "Creature — Spirit");
    }

    #[test]
    fn type_line_renders_supertypes_types_and_subtypes() {
        let db = CardDatabase::bundled().unwrap();
        // Multiple subtypes are space-joined after the em dash.
        assert_eq!(
            db.card(CardId(6)).unwrap().type_line(),
            "Creature — Elf Scout"
        );
        // A supertype precedes the card type; the land subtype follows the dash.
        assert_eq!(
            db.card(CardId(5)).unwrap().type_line(),
            "Basic Land — Forest"
        );
    }

    #[test]
    fn has_type_and_has_subtype_query_structured_types() {
        let db = CardDatabase::bundled().unwrap();
        let scout = db.card(CardId(6)).unwrap();
        assert!(scout.has_type(CardType::Creature));
        assert!(!scout.has_type(CardType::Land));
        assert!(scout.has_subtype("Elf"));
        assert!(!scout.has_subtype("Goblin"));
    }

    #[test]
    fn is_permanent_splits_permanent_types_from_instants_and_sorceries() {
        let db = CardDatabase::bundled().unwrap();
        // Creature and land are permanent cards.
        assert!(db.card(CardId(1)).unwrap().is_permanent());
        assert!(db.card(CardId(5)).unwrap().is_permanent());
        // An instant-only card is not.
        let json = r#"[{"id":100,"name":"Test Bolt","types":["instant"],"mana_cost":"{R}","oracle_text":""}]"#;
        let bolt = CardDatabase::from_json(json).unwrap();
        assert!(!bolt.card(CardId(100)).unwrap().is_permanent());
    }

    #[test]
    fn vanilla_cards_deserialize_with_no_abilities() {
        let db = CardDatabase::bundled().unwrap();
        assert!(db.card(CardId(1)).unwrap().abilities.is_empty());
    }

    #[test]
    fn forest_has_one_activated_mana_ability() {
        let db = CardDatabase::bundled().unwrap();
        let forest = db.card(CardId(5)).unwrap();
        assert_eq!(forest.abilities.len(), 1);
        assert!(crate::ability::is_mana_ability(&forest.abilities[0]));
    }

    #[test]
    fn verdant_scout_has_an_etb_draw_trigger() {
        let db = CardDatabase::bundled().unwrap();
        let scout = db.card(CardId(6)).unwrap();
        assert_eq!(
            scout.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }]
        );
    }

    #[test]
    fn abilities_of_unions_data_and_scripted_sources() {
        let db = CardDatabase::bundled().unwrap();
        // Forest's ability comes from data; no scripted card is registered, so
        // the accessor returns exactly the data-driven ability.
        assert_eq!(
            abilities_of(&db, CardId(5)),
            db.card(CardId(5)).unwrap().abilities
        );
        // An unknown id with no scripted abilities yields nothing.
        assert!(abilities_of(&db, CardId(9999)).is_empty());
    }
}
