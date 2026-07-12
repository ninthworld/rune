//! Card data: resolving a [`CardId`] to its immutable characteristics.
//!
//! The engine performs no I/O. The card snapshot is a JSON file embedded at
//! compile time with [`include_str!`] and parsed in memory; there is no
//! filesystem or network access here. serde is permitted for exactly this
//! purpose — see `docs/decisions/0006-serde-in-engine.md`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::ability::{Ability, Effect, TargetSpec};
use crate::card_type::{CardType, Supertype};
use crate::id::{CardId, OracleId};
use crate::scripted::scripted_abilities;

/// The bundled oracle snapshot, embedded at compile time.
///
/// One record per distinct card — its printing-independent characteristics and
/// ability IR — regardless of how many sets print it (ADR 0013 §2). Deliberately
/// tiny and hand-authored: a handful of vanilla creatures and one basic land.
/// Only non-infringing data — names, type lines, mana costs, oracle text,
/// power/toughness — with no card images, official frames, or WotC branding
/// (crate `AGENTS.md`, `docs/brief.md` Legal Considerations).
const ORACLE_SNAPSHOT: &str = include_str!("../data/oracle.json");

/// One embedded set snapshot: a set code paired with its printing records.
///
/// Set files are enumerated in [`SET_MANIFEST`] as a `const` list of
/// [`include_str!`]ed snapshots, never a directory walk — the engine embeds card
/// data at compile time and does zero I/O at runtime (crate `AGENTS.md`, ADR 0013
/// §2). Adding a set means adding one entry here by hand.
struct SetSnapshot {
    /// The set's code, used as the first half of every printing key.
    code: &'static str,
    /// The embedded JSON: an array of printing records for this set.
    json: &'static str,
}

/// The compile-time manifest of embedded set files (ADR 0013 §2).
///
/// `FIX` prints every oracle fixture; `FIX2` reprints one of them, proving a
/// reprint is one printing entry and zero rules-logic changes. These are engine
/// test fixtures, not a shipped set (ADR 0013 §5).
const SET_MANIFEST: &[SetSnapshot] = &[
    SetSnapshot {
        code: "FIX",
        json: include_str!("../data/sets/FIX.json"),
    },
    SetSnapshot {
        code: "FIX2",
        json: include_str!("../data/sets/FIX2.json"),
    },
];

/// A keyword ability printed on a card (CR 702). Closed set, deserialized from
/// lowercase names (e.g. `"flying"`, `"first_strike"`).
///
/// This is the printed keyword representation the combat and layer systems read;
/// keyword-granting continuous effects are future work, so a permanent's keywords
/// are its card's printed [`CardData::keywords`] today. All eight variants are
/// enforced: [`Flying`](Keyword::Flying), [`Reach`](Keyword::Reach),
/// [`Vigilance`](Keyword::Vigilance), and [`Haste`](Keyword::Haste) at
/// combat-declaration time (keywords I), and
/// [`FirstStrike`](Keyword::FirstStrike), [`Trample`](Keyword::Trample),
/// [`Deathtouch`](Keyword::Deathtouch), and [`Lifelink`](Keyword::Lifelink) at
/// combat-damage time (keywords II — see [`crate::combat::combat_damage`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keyword {
    /// Flying (CR 702.9): can be blocked only by creatures with flying or reach.
    Flying,
    /// Reach (CR 702.17): can block creatures with flying.
    Reach,
    /// Vigilance (CR 702.20): attacking doesn't cause the creature to tap.
    Vigilance,
    /// Haste (CR 702.10): ignores the summoning-sickness restriction on attacking.
    Haste,
    /// First strike (CR 702.7): deals combat damage in a first combat-damage step.
    FirstStrike,
    /// Trample (CR 702.19): a blocked creature assigns excess combat damage to the
    /// player it is attacking.
    Trample,
    /// Deathtouch (CR 702.2): any nonzero damage it deals is lethal.
    Deathtouch,
    /// Lifelink (CR 702.15): damage it deals also gains its controller that much
    /// life.
    Lifelink,
}

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
/// host's current P/T is derived on demand from the attachment via
/// [`characteristics`](crate::characteristics::characteristics), never stored
/// (ADR 0010). Only P/T-granting, enchant-creature Auras are modeled here;
/// keyword-granting Auras, enchant-player/land, and Aura movement are out of scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
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
}

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
    /// The effects this card's **spell ability** produces on resolution — the
    /// instant/sorcery analogue of an ability's effects (CR 608.2c). Empty for a
    /// vanilla card and for a permanent spell whose only "effect" is entering the
    /// battlefield. A targeting spell effect declares its [`crate::TargetSpec`]
    /// here exactly as an ability's effect does, so a spell chooses targets as it
    /// is cast (CR 601.2c); read them with [`spell_effects_of`].
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
    /// for a card with none. Read with [`CardData::has_keyword`]; the combat and
    /// summoning-sickness code consults these directly, since keyword-granting
    /// continuous effects are not modeled yet.
    #[serde(default)]
    pub keywords: Vec<Keyword>,
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
        let mut specs: Vec<TargetSpec> = self.aura.map(|a| a.enchant).into_iter().collect();
        specs.extend(self.spell_effects.iter().filter_map(Effect::target_spec));
        specs
    }

    /// Whether the card has printed keyword ability `keyword` (CR 702). Reads the
    /// printed [`CardData::keywords`]; keyword-granting continuous effects are
    /// future work, so this is authoritative for a permanent's keywords today.
    #[must_use]
    pub fn has_keyword(&self, keyword: Keyword) -> bool {
        self.keywords.contains(&keyword)
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
        Self::from_json(ORACLE_SNAPSHOT)
    }

    /// Parse a JSON snapshot (an array of oracle-card entries) into a database.
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

/// A card's rarity in a given printing (ADR 0013 §1).
///
/// A purely bibliographic property of the printing, not a rule the engine reasons
/// about. Serialized lowercase to mirror Scryfall's data shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    /// Common.
    Common,
    /// Uncommon.
    Uncommon,
    /// Rare.
    Rare,
    /// Mythic rare.
    Mythic,
}

/// A purely bibliographic printing record (ADR 0013 §1).
///
/// A printing is a specific appearance of an oracle card in a set: its
/// [`OracleId`], a collector number, and a rarity. It carries **no** name, cost,
/// types, or abilities — everything mechanical is read through its [`OracleId`]
/// against the oracle [`CardDatabase`] — and **no** art, frame, artist, or
/// branding. That prohibition is structural: the deserializer rejects unknown
/// fields, so an `image_uris`-style field fails to parse rather than being
/// silently ignored (ADR 0013 §6, `docs/brief.md` Legal Considerations).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Printing {
    /// The oracle card this record prints. All rules read through this id.
    pub oracle: OracleId,
    /// The collector number within its set (a string, e.g. `"12"` or `"100a"`).
    pub collector_number: String,
    /// The printing's rarity.
    pub rarity: Rarity,
}

/// The wire form of a [`Printing`]: strictly the bibliographic fields.
///
/// `deny_unknown_fields` is what makes the art/branding prohibition structural —
/// any field beyond these three (e.g. `image_uris`, `artist`, `frame`) is a parse
/// error (ADR 0013 §1, §6).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrintingEntry {
    /// The [`OracleId`] this printing references, as its raw integer.
    oracle_id: u64,
    /// The collector number within its set.
    collector_number: String,
    /// The printing's rarity.
    rarity: Rarity,
}

/// The key identifying a single printing: its set code and collector number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PrintingKey {
    /// The set code (the [`SetSnapshot::code`] the printing was loaded from).
    set_code: String,
    /// The collector number within that set.
    collector_number: String,
}

/// An immutable database of printing records, keyed by set code + collector
/// number, each referencing an [`OracleId`] (ADR 0013 §2).
///
/// The parallel of [`CardDatabase`] for bibliographic data. It holds **no** rules
/// logic: a printing resolves to characteristics only by looking its
/// [`Printing::oracle`] up in the oracle [`CardDatabase`]. Built from the
/// compile-time [`SET_MANIFEST`] via [`PrintingDatabase::bundled`], or from a
/// single set's JSON via [`PrintingDatabase::from_json`].
#[derive(Clone, Debug, Default)]
pub struct PrintingDatabase {
    printings: HashMap<PrintingKey, Printing>,
}

impl PrintingDatabase {
    /// Load every printing in the compile-time-embedded [`SET_MANIFEST`].
    ///
    /// # Errors
    /// Returns the underlying [`serde_json::Error`] if any embedded set file
    /// fails to parse. The snapshots are committed and tested, so this is not
    /// expected in practice; it is surfaced rather than panicked on because the
    /// engine forbids panicking APIs.
    pub fn bundled() -> Result<Self, serde_json::Error> {
        let mut db = Self::default();
        for set in SET_MANIFEST {
            db.load_set(set.code, set.json)?;
        }
        Ok(db)
    }

    /// Parse one set's JSON (an array of printing records) into a fresh database
    /// under `set_code`.
    ///
    /// # Errors
    /// Returns the underlying [`serde_json::Error`] if `json` is not a valid set
    /// snapshot — including when a record carries a field beyond the bibliographic
    /// three, which is rejected by `deny_unknown_fields`.
    pub fn from_json(set_code: &str, json: &str) -> Result<Self, serde_json::Error> {
        let mut db = Self::default();
        db.load_set(set_code, json)?;
        Ok(db)
    }

    /// Parse `json` and insert every printing under `set_code`.
    fn load_set(&mut self, set_code: &str, json: &str) -> Result<(), serde_json::Error> {
        let entries: Vec<PrintingEntry> = serde_json::from_str(json)?;
        for entry in entries {
            let key = PrintingKey {
                set_code: set_code.to_string(),
                collector_number: entry.collector_number.clone(),
            };
            let printing = Printing {
                oracle: CardId(entry.oracle_id),
                collector_number: entry.collector_number,
                rarity: entry.rarity,
            };
            self.printings.insert(key, printing);
        }
        Ok(())
    }

    /// Resolve a set code + collector number to its [`Printing`], or `None` if no
    /// such printing is embedded.
    #[must_use]
    pub fn printing(&self, set_code: &str, collector_number: &str) -> Option<&Printing> {
        self.printings.get(&PrintingKey {
            set_code: set_code.to_string(),
            collector_number: collector_number.to_string(),
        })
    }

    /// The number of printings in the database.
    #[must_use]
    pub fn len(&self) -> usize {
        self.printings.len()
    }

    /// Whether the database holds no printings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.printings.is_empty()
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

/// The effects a spell of printed card `card` produces on resolution
/// ([`CardData::spell_effects`]), or an empty list for an unknown id or a card
/// with no spell ability.
///
/// The spell-side counterpart of [`abilities_of`]: the resolve path reads these
/// to apply a spell's effects (pairing targeting effects with the targets chosen
/// at cast), and [`crate::valid_actions`] reads them to enumerate a targeted
/// cast's requirement slots — the same effect IR, whether it rides an ability or
/// a spell.
#[must_use]
pub(crate) fn spell_effects_of(db: &CardDatabase, card: CardId) -> Vec<Effect> {
    db.card(card)
        .map(|c| c.spell_effects.clone())
        .unwrap_or_default()
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
        assert_eq!(db.len(), 30);
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
    fn runic_negation_carries_a_counter_spell_effect() {
        // The counterspell fixture (id 11) is a vanilla-abilities instant whose
        // spell effect counters a spell on the stack (CR 701.5).
        use crate::ability::TargetSpec;
        let db = CardDatabase::bundled().unwrap();
        let negation = db.card(CardId(11)).unwrap();
        assert_eq!(negation.name, "Runic Negation");
        assert_eq!(negation.types, vec![CardType::Instant]);
        assert!(negation.abilities.is_empty());
        assert_eq!(
            negation.spell_effects,
            vec![Effect::CounterSpell {
                target: TargetSpec::SpellOnStack,
            }]
        );
        assert_eq!(spell_effects_of(&db, CardId(11)), negation.spell_effects);
        // A card with no spell ability reports none.
        assert!(spell_effects_of(&db, CardId(1)).is_empty());
    }

    #[test]
    fn issue_149_effect_ir_wave_fixtures_carry_their_verbs() {
        use crate::ability::{Ability, PlayerRef, TargetSpec, TriggerCondition};
        use crate::state::CounterKind;
        let db = CardDatabase::bundled().unwrap();

        // A burn instant: deal 2 to any target.
        let shock = db.card(CardId(12)).unwrap();
        assert_eq!(shock.name, "Cinder Shock");
        assert_eq!(
            shock.spell_effects,
            vec![Effect::DealDamage {
                target: TargetSpec::AnyTarget,
                amount: 2
            }]
        );
        // A destroy sorcery.
        let ray = db.card(CardId(13)).unwrap();
        assert_eq!(
            ray.spell_effects,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature
            }]
        );
        // A counters-ETB creature: its ETB trigger puts a +1/+1 counter on a
        // target creature.
        let sprite = db.card(CardId(14)).unwrap();
        assert_eq!(
            sprite.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::PutCounters {
                    target: TargetSpec::AnyCreature,
                    counter: CounterKind::PlusOnePlusOne,
                    count: 1,
                }],
            }]
        );
        // Life gain/loss instants and a -1/-1 sorcery.
        assert_eq!(
            db.card(CardId(15)).unwrap().spell_effects,
            vec![Effect::GainLife {
                player_ref: PlayerRef::Controller,
                amount: 3
            }]
        );
        assert_eq!(
            db.card(CardId(16)).unwrap().spell_effects,
            vec![Effect::LoseLife {
                player_ref: PlayerRef::Controller,
                amount: 2
            }]
        );
        assert_eq!(
            db.card(CardId(17)).unwrap().spell_effects,
            vec![Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::MinusOneMinusOne,
                count: 1,
            }]
        );
    }

    #[test]
    fn issue_153_keyword_fixtures_carry_their_printed_keywords() {
        // CR 702: the four enforced-here keyword fixtures each print exactly one
        // keyword, deserialized from its snake_case name, and a vanilla creature
        // prints none.
        let db = CardDatabase::bundled().unwrap();

        let flyer = db.card(CardId(18)).unwrap();
        assert_eq!(flyer.name, "Skywhisker Drake");
        assert!(flyer.has_keyword(Keyword::Flying));
        assert!(!flyer.has_keyword(Keyword::Reach));

        assert!(db.card(CardId(19)).unwrap().has_keyword(Keyword::Reach));
        assert!(db.card(CardId(20)).unwrap().has_keyword(Keyword::Vigilance));
        assert!(db.card(CardId(21)).unwrap().has_keyword(Keyword::Haste));

        // A vanilla creature prints no keywords.
        assert!(db.card(CardId(1)).unwrap().keywords.is_empty());
        assert!(!db.card(CardId(1)).unwrap().has_keyword(Keyword::Flying));
    }

    #[test]
    fn issue_154_damage_keyword_fixtures_carry_their_printed_keywords() {
        // CR 702: the four combat-damage keyword fixtures each print exactly one
        // keyword, the ones keywords II enforces at combat-damage time.
        let db = CardDatabase::bundled().unwrap();

        let duelist = db.card(CardId(22)).unwrap();
        assert_eq!(duelist.name, "Dawnblade Duelist");
        assert!(duelist.has_keyword(Keyword::FirstStrike));
        assert!(!duelist.has_keyword(Keyword::Deathtouch));

        assert!(db.card(CardId(23)).unwrap().has_keyword(Keyword::Trample));
        assert!(db
            .card(CardId(24))
            .unwrap()
            .has_keyword(Keyword::Deathtouch));
        assert!(db.card(CardId(25)).unwrap().has_keyword(Keyword::Lifelink));

        // A creature can print more than one keyword (trample + deathtouch).
        let baneclaw = db.card(CardId(26)).unwrap();
        assert!(baneclaw.has_keyword(Keyword::Trample));
        assert!(baneclaw.has_keyword(Keyword::Deathtouch));
    }

    #[test]
    fn issue_151_dies_fixture_carries_a_self_dies_draw_trigger() {
        // The dies fixture (id 28) is a creature whose triggered ability fires when
        // it dies (CR 700.4 / 603.6c) and draws its controller a card.
        let db = CardDatabase::bundled().unwrap();
        let lurker = db.card(CardId(28)).unwrap();
        assert_eq!(lurker.name, "Cryptvine Lurker");
        assert_eq!(lurker.types, vec![CardType::Creature]);
        assert_eq!(lurker.power, Some(2));
        assert_eq!(lurker.toughness, Some(2));
        assert_eq!(
            lurker.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfDies,
                effects: vec![Effect::DrawCard { count: 1 }],
            }]
        );
    }

    #[test]
    fn issue_150_pump_fixture_carries_its_until_end_of_turn_verb() {
        // The Giant-Growth-style fixture (id 27) is a vanilla-abilities instant
        // whose spell effect pumps a target creature +3/+3 until end of turn.
        use crate::ability::TargetSpec;
        let db = CardDatabase::bundled().unwrap();
        let surge = db.card(CardId(27)).unwrap();
        assert_eq!(surge.name, "Titanroot Surge");
        assert_eq!(surge.types, vec![CardType::Instant]);
        assert!(surge.abilities.is_empty());
        assert_eq!(
            surge.spell_effects,
            vec![Effect::Pump {
                target: TargetSpec::AnyCreature,
                power: 3,
                toughness: 3,
            }]
        );
        assert_eq!(spell_effects_of(&db, CardId(27)), surge.spell_effects);
    }

    #[test]
    fn issue_152_aura_fixtures_carry_their_enchant_and_pt_grant() {
        // CR 303.4: the two Aura fixtures are Enchantment — Aura cards carrying an
        // enchant-creature restriction and a static P/T grant. One buffs (+2/+2),
        // one shrinks (-2/-2); both surface their enchant slot via cast_target_specs.
        use crate::ability::TargetSpec;
        let db = CardDatabase::bundled().unwrap();

        let aegis = db.card(CardId(29)).unwrap();
        assert_eq!(aegis.name, "Ironbark Aegis");
        assert_eq!(aegis.types, vec![CardType::Enchantment]);
        assert!(aegis.has_subtype("Aura"));
        assert_eq!(
            aegis.aura,
            Some(AuraGrant {
                enchant: TargetSpec::AnyCreature,
                power: 2,
                toughness: 2,
            })
        );
        // An Aura chooses its enchant target as it is cast (CR 601.2c): one slot.
        assert_eq!(aegis.cast_target_specs(), vec![TargetSpec::AnyCreature]);

        let curse = db.card(CardId(30)).unwrap();
        assert_eq!(curse.name, "Witherbrand Curse");
        assert!(curse.has_subtype("Aura"));
        assert_eq!(
            curse.aura,
            Some(AuraGrant {
                enchant: TargetSpec::AnyCreature,
                power: -2,
                toughness: -2,
            })
        );

        // A non-Aura card has no aura ability and no cast target slots.
        assert!(db.card(CardId(1)).unwrap().aura.is_none());
        assert!(db.card(CardId(1)).unwrap().cast_target_specs().is_empty());
    }

    #[test]
    fn all_eight_keyword_variants_deserialize_from_snake_case() {
        // The closed keyword set round-trips from its wire names, including the
        // four data-only variants keywords II will enforce (CR 702).
        let json = r#"[{"id":900,"name":"Every Keyword","types":["creature"],
            "mana_cost":"","oracle_text":"","power":1,"toughness":1,
            "keywords":["flying","reach","vigilance","haste","first_strike",
                        "trample","deathtouch","lifelink"]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let card = db.card(CardId(900)).unwrap();
        for kw in [
            Keyword::Flying,
            Keyword::Reach,
            Keyword::Vigilance,
            Keyword::Haste,
            Keyword::FirstStrike,
            Keyword::Trample,
            Keyword::Deathtouch,
            Keyword::Lifelink,
        ] {
            assert!(card.has_keyword(kw), "expected keyword {kw:?}");
        }
    }

    #[test]
    fn bundled_printings_load_from_the_set_manifest() {
        let printings = PrintingDatabase::bundled().unwrap();
        // FIX prints the thirty fixtures; FIX2 reprints one — thirty-one printings total.
        assert_eq!(printings.len(), 31);
        assert!(!printings.is_empty());
        let boar = printings.printing("FIX", "1").unwrap();
        assert_eq!(boar.oracle, CardId(1));
        assert_eq!(boar.rarity, Rarity::Common);
        // A collector number absent from a set does not resolve.
        assert!(printings.printing("FIX", "999").is_none());
        // Neither does an unknown set code.
        assert!(printings.printing("ZZZ", "1").is_none());
    }

    #[test]
    fn adding_a_reprint_changes_no_logic() {
        // Verdant Scout is printed in FIX (#6) and reprinted in FIX2 (#12). The
        // two printings differ only bibliographically; everything the engine
        // reasons about is read through the shared OracleId, so it is identical.
        let cards = CardDatabase::bundled().unwrap();
        let printings = PrintingDatabase::bundled().unwrap();

        let first = printings.printing("FIX", "6").unwrap();
        let reprint = printings.printing("FIX2", "12").unwrap();

        // The printings are distinct bibliographic records...
        assert_ne!(first.collector_number, reprint.collector_number);
        assert_ne!(first.rarity, reprint.rarity);
        // ...but they reference the same oracle identity.
        assert_eq!(first.oracle, reprint.oracle);

        // The oracle record is byte-identical between printings.
        let oracle_a = cards.card(first.oracle).unwrap();
        let oracle_b = cards.card(reprint.oracle).unwrap();
        assert_eq!(oracle_a, oracle_b);

        // The abilities IR (ADR 0007) is identical between printings...
        assert_eq!(
            abilities_of(&cards, first.oracle),
            abilities_of(&cards, reprint.oracle),
        );
        // ...and it is the real ETB-draw behavior, not an empty coincidence.
        assert_eq!(
            abilities_of(&cards, first.oracle),
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }],
        );
    }

    #[test]
    fn printing_deserializes_only_bibliographic_fields() {
        let json = r#"[{"oracle_id":1,"collector_number":"1","rarity":"common"}]"#;
        let db = PrintingDatabase::from_json("TST", json).unwrap();
        assert_eq!(db.len(), 1);
        let p = db.printing("TST", "1").unwrap();
        assert_eq!(p.oracle, CardId(1));
        assert_eq!(p.rarity, Rarity::Common);
    }

    #[test]
    fn printing_rejects_art_and_branding_fields() {
        // An image_uris-style field must fail to parse: the art/branding
        // prohibition is structural via deny_unknown_fields (ADR 0013 §6).
        let json = r#"[{"oracle_id":1,"collector_number":"1","rarity":"common","image_uris":{"small":"x"}}]"#;
        assert!(PrintingDatabase::from_json("TST", json).is_err());
        // An artist credit is likewise rejected.
        let json =
            r#"[{"oracle_id":1,"collector_number":"1","rarity":"common","artist":"Someone"}]"#;
        assert!(PrintingDatabase::from_json("TST", json).is_err());
    }

    #[test]
    fn printing_rejects_malformed_input() {
        assert!(PrintingDatabase::from_json("TST", "not json").is_err());
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
