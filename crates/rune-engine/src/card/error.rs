//! Catalog loading errors.

use std::fmt;

use crate::catalog::Violation;
use crate::id::FunctionalId;

/// Everything that can go wrong loading the catalog or a set (ADR 0018 §2, §5).
///
/// Every variant is a *load-time* failure: a malformed or inconsistent catalog
/// never half-loads into a database the engine would then query and find `None` in
/// mid-game. Errors are returned, not panicked on — the engine forbids panicking
/// APIs (`docs/coding-standards.md`).
#[derive(Debug)]
pub enum CatalogError {
    /// The snapshot is not valid JSON, or an entry violates the schema — including
    /// an unknown field (a presentation asset) rejected by `deny_unknown_fields`,
    /// and an effect that needs a [`TargetSpec`](crate::ability::TargetSpec) and has none, which the IR makes
    /// unrepresentable rather than merely invalid.
    Json(serde_json::Error),
    /// A definition breaks one of the authored-schema rules in [`crate::Violation`] —
    /// the same checks `build.rs` runs over `data/` at compile time (ADR 0018 §5), run
    /// here over whatever snapshot was handed to the loader.
    Schema(Violation),
    /// Two definitions claim the same [`FunctionalId`]; an authored identity is
    /// never reused (ADR 0018 §3).
    DuplicateFunctionalId {
        /// The identity claimed twice.
        functional_id: FunctionalId,
    },
    /// Two definitions intern to the same [`CardId`](crate::id::CardId), so one would shadow the other.
    DuplicateCardId {
        /// The handle claimed twice.
        id: crate::id::CardId,
    },
    /// A definition declares `scripted: true` but [`crate::scripted`] holds no arm for
    /// it — so its behavior would be missing and no rules text could be generated for
    /// it (ADR 0018 §5, §7).
    ScriptedWithoutCode {
        /// The definition that declared the escape hatch.
        functional_id: FunctionalId,
    },
    /// [`crate::scripted`] holds an arm for a card whose definition does not declare
    /// `scripted: true` — the other direction of the same rule: the data tier and the
    /// code tier may not disagree about which cards are scripted (ADR 0018 §5).
    UndeclaredScriptedCard {
        /// The definition with a code arm it does not admit to.
        functional_id: FunctionalId,
    },
    /// A printing references a functional definition the catalog does not contain.
    UnknownFunctionalId {
        /// The set the printing was loaded from.
        set_code: String,
        /// The printing's collector number within that set.
        collector_number: String,
        /// The identity it references.
        functional_id: FunctionalId,
    },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(err) => write!(f, "card data does not match the schema: {err}"),
            Self::Schema(violation) => write!(f, "{violation}"),
            Self::DuplicateFunctionalId { functional_id } => {
                write!(f, "two definitions claim the functional id {functional_id}")
            }
            Self::DuplicateCardId { id } => {
                write!(f, "two definitions intern to {id:?}")
            }
            Self::ScriptedWithoutCode { functional_id } => write!(
                f,
                "{functional_id} declares scripted: true, but crates/rune-engine/src/scripted.rs \
                 has no arm for it (it needs both its abilities and its rules text)"
            ),
            Self::UndeclaredScriptedCard { functional_id } => write!(
                f,
                "crates/rune-engine/src/scripted.rs has an arm for {functional_id}, \
                 which does not declare scripted: true"
            ),
            Self::UnknownFunctionalId {
                set_code,
                collector_number,
                functional_id,
            } => write!(
                f,
                "printing {set_code} #{collector_number} references {functional_id}, \
                 which is not in the catalog"
            ),
        }
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for CatalogError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<Violation> for CatalogError {
    fn from(violation: Violation) -> Self {
        Self::Schema(violation)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::card::CardDatabase;

    #[test]
    fn from_json_rejects_malformed_input() {
        assert!(CardDatabase::from_json("not json").is_err());
    }

    #[test]
    fn no_definition_carries_a_hand_written_handle() {
        // ADR 0018 §3: `CardId`s are interned, never authored. The `id` field the old
        // monolithic `oracle.json` carried is now an unknown field, so a definition that
        // tries to pin its own handle fails to parse rather than being quietly honored.
        let json = r#"[{"schema_version":1,"id":7,"functional_id":"test_boar","name":"Test Boar",
                        "types":["creature"],"mana_cost":"{G}","colors":["green"],
                        "power":1,"toughness":1}]"#;
        let err = CardDatabase::from_json(json).unwrap_err();
        assert!(
            matches!(err, CatalogError::Json(_)),
            "a hand-written `id` should be rejected as an unknown field, got {err:?}"
        );
    }

    #[test]
    fn an_ill_formed_functional_id_fails_the_load() {
        let json = r#"[{"schema_version":1,"functional_id":"Thornback Boar","name":"Test Boar",
                        "types":["creature"],"mana_cost":"{G}","colors":["green"],
                        "power":1,"toughness":1}]"#;
        assert!(matches!(
            CardDatabase::from_json(json).unwrap_err(),
            CatalogError::Schema(Violation::MalformedFunctionalId { .. })
        ));
    }

    #[test]
    fn definition_rejects_presentation_assets() {
        // ADR 0018 §2: the functional schema is closed. Upstream presentation data
        // is structurally rejected, so it cannot enter the catalog by accident.
        for field in [
            r#""flavor_text":"A boar with a bad temper.""#,
            r#""image_uris":{"small":"https://example.test/boar.png"}"#,
            r#""artist":"Someone""#,
            r#""frame":"2015""#,
            r#""watermark":"guild""#,
        ] {
            let json = format!(
                r#"[{{"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
                     "types":["creature"],"mana_cost":"{{G}}","colors":["green"],
                     "power":1,"toughness":1,{field}}}]"#
            );
            let err = CardDatabase::from_json(&json).unwrap_err();
            assert!(
                matches!(err, CatalogError::Json(_)),
                "{field} should be rejected as an unknown field, got {err:?}"
            );
        }
    }

    #[test]
    fn unrecognized_schema_version_fails_loudly() {
        // ADR 0018 §2: an unknown version is a hard error naming the offender, not a
        // silent skip that would leave the card missing from a running game. The check
        // is the same code `build.rs` runs over `data/` (ADR 0018 §5).
        let json = r#"[{"schema_version":99,"functional_id":"test_boar","name":"Test Boar",
                        "types":["creature"],"mana_cost":"{G}","colors":["green"],
                        "power":1,"toughness":1}]"#;
        let err = CardDatabase::from_json(json).unwrap_err();
        assert!(
            matches!(&err, CatalogError::Schema(Violation::UnsupportedSchemaVersion { functional_id, found })
                if functional_id == "test_boar" && *found == 99),
            "expected an unsupported-version error, got {err:?}"
        );
        let message = err.to_string();
        assert!(
            message.contains("test_boar") && message.contains("99"),
            "{message}"
        );
        // Every bundled definition declares the version this engine understands.
        let db = CardDatabase::bundled().unwrap();
        assert!(crate::card::tests::every_id()
            .all(|id| db.card(id).unwrap().schema_version == crate::card::SCHEMA_VERSION));
    }

    #[test]
    fn a_duplicated_identity_fails_the_load() {
        // Two definitions claiming one authored identity would make the catalog
        // ambiguous; the second is an error, not a silent overwrite. (In the sharded
        // catalog this is also impossible by construction — the identity is the file
        // name — but a snapshot handed to `from_json` has no filesystem to enforce it.)
        let entry = |functional_id: &str| {
            format!(
                r#"{{"schema_version":1,"functional_id":"{functional_id}","name":"Test Boar",
                    "types":["creature"],"mana_cost":"{{G}}","colors":["green"],
                    "power":1,"toughness":1}}"#
            )
        };
        let json = format!("[{},{}]", entry("test_boar"), entry("test_boar"));
        assert!(matches!(
            CardDatabase::from_json(&json).unwrap_err(),
            CatalogError::DuplicateFunctionalId { .. }
        ));
        // Two *distinct* identities cannot collide on a handle, because nobody assigns
        // one: they intern to consecutive integers in sorted order.
        let json = format!("[{},{}]", entry("test_boar"), entry("other_boar"));
        let db = CardDatabase::from_json(&json).unwrap();
        assert_eq!(db.len(), 2);
        assert_eq!(
            crate::card::tests::id_of(&db, "other_boar"),
            crate::id::CardId(0)
        );
        assert_eq!(
            crate::card::tests::id_of(&db, "test_boar"),
            crate::id::CardId(1)
        );
    }
}
