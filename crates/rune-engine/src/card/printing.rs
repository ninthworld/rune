//! Printing records and database.

use std::collections::HashMap;

use serde::Deserialize;

use super::database::CardDatabase;
use super::error::CatalogError;
use super::rarity::Rarity;
use crate::catalog::check_printings;
use crate::id::OracleId;

/// One embedded set snapshot: a set code paired with its printing records.
///
/// The set code comes from the file name (`FIX.json` → `FIX`), so adding a set is
/// adding a file — there is no hand-written list to forget to update.
pub(super) struct SetSnapshot {
    /// The set's code, used as the first half of every printing key.
    pub code: &'static str,
    /// The embedded JSON: an array of printing records for this set.
    pub json: &'static str,
}

/// A purely bibliographic printing record (ADR 0013 §1).
///
/// A printing is a specific appearance of a card in a set: the functional
/// definition it prints, a collector number, and a rarity. It carries **no** name,
/// cost, types, or abilities — everything mechanical is read through its
/// [`OracleId`] against the [`CardDatabase`] — and **no** art, frame, artist, or
/// branding. That prohibition is structural: the deserializer rejects unknown
/// fields, so an `image_uris`-style field fails to parse rather than being
/// silently ignored (ADR 0013 §6, `docs/brief.md` Legal Considerations).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Printing {
    /// The card this record prints, as the handle it interned to. All rules read
    /// through this id. The record itself names the card by [`FunctionalId`](crate::id::FunctionalId); the
    /// loader resolves that to this handle, so an unresolvable reference fails the
    /// load rather than surfacing as a `None` mid-game.
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
    /// The functional definition this printing prints, by its authored identity —
    /// the identity that survives a rebuild, unlike the interned handle (ADR 0018 §3).
    functional_id: crate::id::FunctionalId,
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
/// compile-time [`SET_MANIFEST`](super::SET_MANIFEST) via [`PrintingDatabase::bundled`], or from a
/// single set's JSON via [`PrintingDatabase::from_json`].
#[derive(Clone, Debug, Default)]
pub struct PrintingDatabase {
    printings: HashMap<PrintingKey, Printing>,
}

impl PrintingDatabase {
    /// Load every printing in the compile-time-embedded [`SET_MANIFEST`](super::SET_MANIFEST), resolving
    /// each record's [`FunctionalId`](crate::id::FunctionalId) against `cards`.
    ///
    /// # Errors
    /// Returns a [`CatalogError`] if any embedded set file fails to parse or
    /// references a definition `cards` does not hold. The snapshots are committed
    /// and tested, so this is not expected in practice; it is surfaced rather than
    /// panicked on because the engine forbids panicking APIs.
    pub fn bundled(cards: &CardDatabase) -> Result<Self, CatalogError> {
        let mut db = Self::default();
        for set in super::catalog::SET_MANIFEST {
            db.load_set(set.code, set.json, cards)?;
        }
        Ok(db)
    }

    /// Parse one set's JSON (an array of printing records) into a fresh database
    /// under `set_code`, resolving each record's [`FunctionalId`](crate::id::FunctionalId) against `cards`.
    ///
    /// # Errors
    /// Returns a [`CatalogError`] if `json` is not a valid set snapshot — including
    /// when a record carries a field beyond the bibliographic three, which is
    /// rejected by `deny_unknown_fields` — or when a record references a functional
    /// definition the catalog does not contain.
    pub fn from_json(
        set_code: &str,
        json: &str,
        cards: &CardDatabase,
    ) -> Result<Self, CatalogError> {
        let mut db = Self::default();
        db.load_set(set_code, json, cards)?;
        Ok(db)
    }

    /// Parse `json` and insert every printing under `set_code`, resolving authored
    /// identities to this build's interned handles.
    fn load_set(
        &mut self,
        set_code: &str,
        json: &str,
        cards: &CardDatabase,
    ) -> Result<(), CatalogError> {
        let entries: Vec<PrintingEntry> = serde_json::from_str(json)?;
        // Printings are keyed by (set, collector number), so a repeat would silently
        // shadow the earlier record instead of failing (ADR 0018 §5).
        check_printings(
            set_code,
            entries.iter().map(|e| e.collector_number.as_str()),
        )?;
        for entry in entries {
            let oracle = cards.card_id(&entry.functional_id).ok_or_else(|| {
                CatalogError::UnknownFunctionalId {
                    set_code: set_code.to_string(),
                    collector_number: entry.collector_number.clone(),
                    functional_id: entry.functional_id.clone(),
                }
            })?;
            let key = PrintingKey {
                set_code: set_code.to_string(),
                collector_number: entry.collector_number.clone(),
            };
            let printing = Printing {
                oracle,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_printings_load_from_the_set_manifest() {
        let cards = CardDatabase::bundled().unwrap();
        let printings = PrintingDatabase::bundled(&cards).unwrap();
        // M19 prints fifty-nine cards; PM19 reprints one — sixty printings total.
        assert_eq!(printings.len(), 60);
        assert!(!printings.is_empty());
        let ogre = printings.printing("M19", "15").unwrap();
        // The record names onakke_ogre; the loader resolved that to its handle.
        assert_eq!(
            ogre.oracle,
            crate::card::tests::id_of(&cards, "onakke_ogre")
        );
        assert_eq!(ogre.rarity, Rarity::Common);
        // A collector number absent from a set does not resolve.
        assert!(printings.printing("M19", "999").is_none());
        // Neither does an unknown set code.
        assert!(printings.printing("ZZZ", "1").is_none());
    }

    #[test]
    fn adding_a_reprint_changes_no_logic() {
        // Skyscanner is printed in M19 (#19) and reprinted in PM19 (#1). The
        // two printings differ only bibliographically; everything the engine
        // reasons about is read through the shared OracleId, so it is identical.
        let cards = CardDatabase::bundled().unwrap();
        let printings = PrintingDatabase::bundled(&cards).unwrap();

        let first = printings.printing("M19", "19").unwrap();
        let reprint = printings.printing("PM19", "1").unwrap();

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
            crate::card::abilities_of(&cards, first.oracle),
            crate::card::abilities_of(&cards, reprint.oracle),
        );
        // ...and it is the real ETB-draw behavior, not an empty coincidence.
        use crate::ability::{Ability, Effect, TriggerCondition};
        assert_eq!(
            crate::card::abilities_of(&cards, first.oracle),
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }],
        );
    }

    #[test]
    fn printing_deserializes_only_bibliographic_fields() {
        let cards = CardDatabase::bundled().unwrap();
        let json = r#"[{"functional_id":"onakke_ogre","collector_number":"1","rarity":"common"}]"#;
        let db = PrintingDatabase::from_json("TST", json, &cards).unwrap();
        assert_eq!(db.len(), 1);
        let p = db.printing("TST", "1").unwrap();
        assert_eq!(p.oracle, crate::card::tests::id_of(&cards, "onakke_ogre"));
        assert_eq!(p.rarity, Rarity::Common);
    }

    #[test]
    fn printing_rejects_art_and_branding_fields() {
        // An image_uris-style field must fail to parse: the art/branding
        // prohibition is structural via deny_unknown_fields (ADR 0013 §6).
        let cards = CardDatabase::bundled().unwrap();
        let json = r#"[{"functional_id":"onakke_ogre","collector_number":"1","rarity":"common","image_uris":{"small":"x"}}]"#;
        assert!(PrintingDatabase::from_json("TST", json, &cards).is_err());
        // An artist credit is likewise rejected.
        let json = r#"[{"functional_id":"onakke_ogre","collector_number":"1","rarity":"common","artist":"Someone"}]"#;
        assert!(PrintingDatabase::from_json("TST", json, &cards).is_err());
    }

    #[test]
    fn printing_rejects_malformed_input() {
        let cards = CardDatabase::bundled().unwrap();
        assert!(PrintingDatabase::from_json("TST", "not json", &cards).is_err());
    }

    #[test]
    fn printing_referencing_an_absent_card_fails_the_load() {
        // ADR 0018 §3: a printing names a card by its authored identity, and an
        // unresolvable reference is a load-time error — never a database that
        // resolves to None mid-game.
        let cards = CardDatabase::bundled().unwrap();
        let json = r#"[{"functional_id":"no_such_card","collector_number":"1","rarity":"common"}]"#;
        let err = PrintingDatabase::from_json("TST", json, &cards).unwrap_err();
        assert!(
            matches!(&err, CatalogError::UnknownFunctionalId { functional_id, set_code, .. }
                if functional_id.as_str() == "no_such_card" && set_code == "TST"),
            "expected an unresolved-reference error, got {err:?}"
        );
        assert!(err.to_string().contains("not in the catalog"), "{err}");
    }
}
