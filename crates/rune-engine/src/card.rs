//! Card data: resolving a [`CardId`] to its immutable characteristics.
//!
//! The engine performs no I/O. The card snapshot is a JSON file embedded at
//! compile time with [`include_str!`] and parsed in memory; there is no
//! filesystem or network access here. serde is permitted for exactly this
//! purpose — see `docs/decisions/0006-serde-in-engine.md`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::ability::Ability;
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
    /// The full type line (e.g. `"Creature — Boar"`, `"Basic Land — Forest"`).
    pub type_line: String,
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
        assert_eq!(boar.type_line, "Creature — Boar");
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
        assert_eq!(forest.type_line, "Basic Land — Forest");
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
        let json = r#"[{"id":42,"name":"Test Wisp","type_line":"Creature — Spirit","mana_cost":"{U}","oracle_text":"","power":1,"toughness":1}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        assert_eq!(db.len(), 1);
        assert_eq!(db.card(CardId(42)).unwrap().name, "Test Wisp");
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
