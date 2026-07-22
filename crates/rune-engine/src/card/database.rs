//! Card database for functional definitions.

use std::collections::HashMap;

use super::card_data::CardData;
use super::catalog::{parse_definition, parse_value};
use super::error::CatalogError;
use crate::id::{CardId, FunctionalId};
use crate::scripted::is_scripted;

/// One functional definition embedded at compile time (ADR 0018 §4).
///
/// The build script pairs each `data/catalog/<functional_id>.json` with the [`CardId`]
/// it interned for it, so the handle is assigned by the catalog rather than written
/// into the card file by hand (ADR 0018 §3).
pub(super) struct CatalogEntry {
    /// The handle this definition interned to in *this build* — its index in
    /// [`CATALOG`], which is sorted by [`FunctionalId`] byte value.
    pub id: CardId,
    /// The definition's authored identity, which is also its file name. Carried here
    /// so the loader can re-check the file-name rule that `build.rs` enforced.
    pub functional_id: &'static str,
    /// The embedded JSON: one functional definition.
    pub json: &'static str,
}

/// An immutable, `CardId`-keyed database of functional definitions.
///
/// Built from a JSON snapshot (the bundled one via [`CardDatabase::bundled`], or
/// any snapshot via [`CardDatabase::from_json`]). Lookups are pure: the database
/// never mutates and holds no game state. It also indexes each definition's
/// authored [`FunctionalId`], which is how a printing (and, later, a decklist)
/// resolves to the interned [`CardId`] every rules read goes through.
#[derive(Clone, Debug, Default)]
pub struct CardDatabase {
    cards: HashMap<CardId, CardData>,
    interned: HashMap<FunctionalId, CardId>,
}

impl CardDatabase {
    /// Load the database from the compile-time-embedded catalog (ADR 0018 §4).
    ///
    /// Reads the manifest `build.rs` generated: one entry per
    /// `data/catalog/<functional_id>.json`, already interned in sorted
    /// [`FunctionalId`] order. No filesystem access happens here — every definition is
    /// an `include_str!`ed `&'static str` by the time this runs.
    ///
    /// # Errors
    /// Returns a [`CatalogError`] if an embedded definition does not parse or does not
    /// validate. `build.rs` ran the same schema checks at compile time, so the only
    /// failure this can still surface is the one it cannot see: a `scripted` flag that
    /// disagrees with [`crate::scripted`] (ADR 0018 §5). It is returned rather than
    /// panicked on because the engine forbids panicking APIs.
    pub fn bundled() -> Result<Self, CatalogError> {
        let mut db = Self::default();
        for entry in super::catalog::CATALOG {
            let data = parse_definition(Some(entry.functional_id), entry.json)?;
            db.insert(entry.id, data)?;
        }
        Ok(db)
    }

    /// Parse a JSON snapshot (an array of functional definitions) into a database,
    /// interning a [`CardId`] for each exactly as `build.rs` does.
    ///
    /// The in-memory counterpart of [`Self::bundled`], for tests and any caller holding
    /// a snapshot rather than the bundled catalog. It applies the *same* interning rule
    /// — sort by [`FunctionalId`] byte value, assign `CardId(0..n)` (ADR 0018 §3) — so a
    /// snapshot and a build agree on handles, and no caller hand-writes one.
    ///
    /// # Errors
    /// Returns a [`CatalogError`] if `json` is not a valid snapshot: malformed JSON, an
    /// unknown (presentation) field, a schema violation ([`crate::Violation`]), a
    /// duplicated identity, or a `scripted` flag that disagrees with the code.
    pub fn from_json(json: &str) -> Result<Self, CatalogError> {
        let entries: Vec<serde_json::Value> = serde_json::from_str(json)?;
        let mut cards = Vec::with_capacity(entries.len());
        for entry in entries {
            cards.push(parse_value(None, entry)?);
        }

        // The one interning rule, shared with `build.rs`: sorted authored identity in,
        // `CardId(0..n)` out. Nothing hand-assigns a handle (ADR 0018 §3).
        cards.sort_by(|a, b| a.functional_id.cmp(&b.functional_id));

        let mut db = Self::default();
        for (index, data) in cards.into_iter().enumerate() {
            db.insert(CardId(index as u64), data)?;
        }
        Ok(db)
    }

    /// Validate one definition against the code tier and index it under both its handle
    /// and its authored identity.
    ///
    /// The schema itself was already checked by [`validate_definition`](crate::catalog::validate_definition); what is left is
    /// the rule that needs compiled Rust to answer, and so cannot live in `build.rs`.
    fn insert(&mut self, id: CardId, data: CardData) -> Result<(), CatalogError> {
        // The escape hatch is declared in data and implemented in code; the two tiers
        // must agree in both directions, or a card silently loses its behavior (and its
        // generated rules text) or silently gains behavior nobody declared (ADR 0018 §5).
        // Keyed on the authored identity, so the check does not depend on how this build
        // happened to intern the handle.
        match (data.scripted, is_scripted(&data.functional_id)) {
            (true, false) => {
                return Err(CatalogError::ScriptedWithoutCode {
                    functional_id: data.functional_id,
                })
            }
            (false, true) => {
                return Err(CatalogError::UndeclaredScriptedCard {
                    functional_id: data.functional_id,
                })
            }
            (true, true) | (false, false) => {}
        }
        if self.cards.contains_key(&id) {
            return Err(CatalogError::DuplicateCardId { id });
        }
        if self.interned.contains_key(&data.functional_id) {
            return Err(CatalogError::DuplicateFunctionalId {
                functional_id: data.functional_id,
            });
        }
        self.interned.insert(data.functional_id.clone(), id);
        self.cards.insert(id, data);
        Ok(())
    }

    /// Resolve a [`CardId`] to its characteristics, or `None` if the id is not
    /// in the database.
    #[must_use]
    pub fn card(&self, id: CardId) -> Option<&CardData> {
        self.cards.get(&id)
    }

    /// Resolve an authored [`FunctionalId`] to the [`CardId`] it is interned under
    /// in this build, or `None` if the catalog holds no such definition.
    ///
    /// The one direction that crosses from authored identity to engine handle:
    /// printings resolve through it at load time, so no runtime lookup can find a
    /// dangling reference.
    #[must_use]
    pub fn card_id(&self, functional_id: &FunctionalId) -> Option<CardId> {
        self.interned.get(functional_id).copied()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::catalog;

    #[test]
    fn bundled_snapshot_parses() {
        let db = CardDatabase::bundled().unwrap();
        assert!(!db.is_empty());
        assert_eq!(db.len(), crate::card::tests::CATALOG_SIZE);
    }

    #[test]
    fn unknown_id_resolves_to_none() {
        let db = CardDatabase::bundled().unwrap();
        assert!(db.card(CardId(9999)).is_none());
    }

    #[test]
    fn handles_are_interned_in_sorted_functional_id_order() {
        // ADR 0018 §3: `build.rs` sorts every `FunctionalId` by byte value and assigns
        // CardId(0..n) in that order. This is the whole contract between the build
        // script and the loader, so it is asserted directly rather than assumed.
        let db = CardDatabase::bundled().unwrap();

        let mut slugs: Vec<String> = crate::card::tests::every_id()
            .map(|id| db.card(id).unwrap().functional_id.to_string())
            .collect();
        let interned_order = slugs.clone();
        slugs.sort();
        assert_eq!(
            interned_order, slugs,
            "the catalog's handles are not in sorted functional-id order"
        );

        // The mapping is a bijection: every handle round-trips through its identity.
        for id in crate::card::tests::every_id() {
            assert_eq!(db.card_id(&db.card(id).unwrap().functional_id), Some(id));
        }
    }

    #[test]
    fn from_json_parses_a_minimal_snapshot() {
        let json = r#"[{"schema_version":1,"functional_id":"test_wisp","name":"Test Wisp","types":["creature"],"subtypes":["Spirit"],"mana_cost":"{U}","power":1,"toughness":1}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        assert_eq!(db.len(), 1);
        let wisp = crate::card::tests::card_named(&db, "test_wisp");
        assert_eq!(wisp.name, "Test Wisp");
        assert_eq!(wisp.type_line(), "Creature — Spirit");
    }

    #[test]
    fn every_fixture_carries_a_unique_functional_id_matching_its_name() {
        // ADR 0018 §3: each definition's authored identity is a lowercase snake_case
        // slug of its name, unique across the catalog, and resolves back to the
        // handle the engine interned it under.
        let db = CardDatabase::bundled().unwrap();
        let mut seen = std::collections::HashSet::new();
        for id in crate::card::tests::every_id() {
            let card = db.card(id).unwrap();
            let expected: String = card
                .name
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            assert_eq!(
                card.functional_id.as_str(),
                expected,
                "{} should be slugged from its name",
                card.name
            );
            assert!(
                seen.insert(card.functional_id.clone()),
                "{} is claimed twice",
                card.functional_id
            );
            assert_eq!(db.card_id(&card.functional_id), Some(id));
        }
        assert_eq!(seen.len(), crate::card::tests::CATALOG_SIZE);
        // An identity no definition claims resolves to nothing.
        let absent = crate::id::FunctionalId::try_from("nonexistent_card".to_string()).unwrap();
        assert!(db.card_id(&absent).is_none());
    }

    #[test]
    fn every_definition_lives_in_a_file_named_for_its_identity() {
        // ADR 0018 §4: one card per file, the file name *is* the identity. `build.rs`
        // enforces this over `data/`; asserting it here means a stale or mis-generated
        // manifest cannot slip through `cargo test` either.
        let db = CardDatabase::bundled().unwrap();
        for entry in catalog::CATALOG {
            let functional_id = crate::id::FunctionalId::try_from(entry.functional_id.to_string())
                .unwrap_or_else(|err| panic!("{err}"));
            let card = db.card(entry.id).unwrap();
            assert_eq!(
                card.functional_id, functional_id,
                "the manifest files {} under the wrong identity",
                entry.functional_id
            );
        }
        assert_eq!(
            catalog::CATALOG.len(),
            crate::card::tests::CATALOG_SIZE
        );
    }

    #[test]
    fn scripted_is_an_explicit_flag_defaulting_to_false() {
        // ADR 0018 §2: the escape hatch is declared on the card, not inferred. No
        // bundled card is scripted today, and every bundled card is therefore fully
        // describable from its IR alone.
        let db = CardDatabase::bundled().unwrap();
        assert!(crate::card::tests::every_id().all(|id| !db.card(id).unwrap().scripted));
        assert!(crate::card::tests::every_id()
            .all(|id| crate::scripted_rules_text(&db.card(id).unwrap().functional_id).is_none()));
    }

    /// A definition under `functional_id`, scripted or not, in the schema's current shape.
    fn scripted_fixture(functional_id: &str, scripted: bool) -> String {
        format!(
            r#"[{{"schema_version":1,"functional_id":"{functional_id}","name":"Bespoke Thing",
                 "types":["creature"],"mana_cost":"{{B}}","colors":["black"],
                 "power":1,"toughness":1,"scripted":{scripted}}}]"#
        )
    }

    #[test]
    fn the_scripted_flag_and_the_code_arm_must_agree_in_both_directions() {
        // ADR 0018 §5: the data tier and the code tier cannot silently disagree about
        // which cards are scripted. This is the one catalog rule `build.rs` cannot check
        // — the code tier is compiled Rust, which does not exist when it runs — so the
        // loader owns it, and these are the tests that hold it up.
        //
        // A card that claims the escape hatch without a code arm would lose its behavior
        // *and* have no rules text to show...
        let err = CardDatabase::from_json(&scripted_fixture("no_such_arm", true)).unwrap_err();
        assert!(
            matches!(&err, CatalogError::ScriptedWithoutCode { functional_id }
                if functional_id.as_str() == "no_such_arm"),
            "expected a scripted-without-code error, got {err:?}"
        );
        assert!(err.to_string().contains("scripted.rs"), "{err}");

        // ...and a card with a code arm that does not declare it would gain behavior
        // nobody authored in the catalog.
        let scripted_card = crate::scripted::TEST_SCRIPTED_CARD;
        let err = CardDatabase::from_json(&scripted_fixture(scripted_card, false)).unwrap_err();
        assert!(
            matches!(&err, CatalogError::UndeclaredScriptedCard { functional_id }
                if functional_id.as_str() == scripted_card),
            "expected an undeclared-scripted error, got {err:?}"
        );

        // Declared on both sides, it loads, and its hand-authored text is available for
        // the server to present in place of generated text (ADR 0018 §7).
        let db = CardDatabase::from_json(&scripted_fixture(scripted_card, true)).unwrap();
        let card = crate::card::tests::card_named(&db, scripted_card);
        assert!(card.scripted);
        assert!(crate::scripted_rules_text(&card.functional_id).is_some());
    }
}
