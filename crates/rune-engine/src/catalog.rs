//! Catalog schema validation, shared verbatim by `build.rs` and the loader.
//!
//! This module is compiled **twice**: once as part of the engine (`mod catalog`), and
//! once by `crates/rune-engine/build.rs`, which pulls this exact file in with
//! `#[path = "src/catalog.rs"] mod catalog;`. That is why it depends on nothing but
//! `std` and `serde_json`, and never names a `crate::` path: `build.rs` is compiled
//! *before* the engine exists, so it cannot borrow the engine's types.
//!
//! Compiling one file in both places is what makes ADR 0018 §5's promise —
//! "the same validators run under `#[cfg(test)]`" — literally true rather than
//! aspirational. A rule stated here is enforced when the catalog is assembled
//! (`build.rs`, so a bad card file fails `cargo build`), again when a snapshot is
//! loaded ([`crate::CardDatabase`]), and again by this module's own unit tests, with
//! no second copy to drift out of step.
//!
//! Everything here works on [`serde_json::Value`] rather than the typed
//! [`CardData`](crate::CardData) precisely because `build.rs` cannot see that type.
//! The division of labor is deliberate:
//!
//! - **Here**: rules about a definition's *shape* that hold before the IR is known —
//!   the schema version, the authored identity, the type/P&T and Aura invariants.
//! - **In the type system**: rules serde already makes unrepresentable. Every
//!   targeting [`Effect`](crate::Effect) variant declares `target: TargetSpec` as a
//!   required field, as does [`AuraGrant::enchant`](crate::AuraGrant::enchant), so
//!   "an effect that needs a target spec but has none" cannot be written down — it is
//!   a parse error, not a validation failure. No check here re-states it.
//! - **In the loader**: the one rule that is impossible to check here — whether a
//!   definition's `scripted` flag agrees with `crates/rune-engine/src/scripted.rs`.
//!   That answer lives in compiled Rust, which does not exist yet when `build.rs`
//!   runs, so [`CardDatabase::from_json`](crate::CardDatabase::from_json) owns it (in
//!   both directions — ADR 0018 §5).

use std::fmt;

/// The functional-definition schema version this engine understands (ADR 0018 §2).
///
/// Re-exported as `rune_engine::SCHEMA_VERSION`. A definition declaring any other
/// version is a hard error ([`Violation::UnsupportedSchemaVersion`]), never a silent
/// skip: a breaking change to the schema's shape bumps this, so the whole catalog is
/// migrated under one forcing function instead of half-loading.
pub const SCHEMA_VERSION: u32 = 1;

/// The subtype that makes a card an Aura (CR 303.4), and therefore the only kind of
/// card an `aura` grant may appear on.
const AURA_SUBTYPE: &str = "Aura";

/// The card type that requires printed power and toughness.
const CREATURE_TYPE: &str = "creature";

/// A catalog file that does not satisfy the authored schema (ADR 0018 §5).
///
/// Returned by [`validate_definition`] and [`check_printings`]. `build.rs` turns one
/// of these into a build failure; the loader turns it into a
/// [`CatalogError`](crate::CatalogError). Each variant names the offending card so the
/// message points at the file to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A definition file holds something other than a single JSON object — most
    /// likely the old monolithic array, which ADR 0018 §4 replaced with one file per
    /// card.
    NotAnObject,
    /// A required field is missing, or holds the wrong JSON type.
    MalformedField {
        /// The card the field belongs to, or the file stem if identity itself is the
        /// problem.
        functional_id: String,
        /// The field at fault.
        field: &'static str,
    },
    /// A definition declares a `schema_version` this engine does not understand.
    UnsupportedSchemaVersion {
        /// The definition that declared it.
        functional_id: String,
        /// The version it declared.
        found: u64,
    },
    /// A `functional_id` is not a well-formed slug (see [`is_well_formed_slug`]).
    MalformedFunctionalId {
        /// The ill-formed slug.
        slug: String,
    },
    /// A definition's `functional_id` does not match the file it is stored in. The
    /// file name *is* the identity (ADR 0018 §4), so the two may not disagree.
    FileNameMismatch {
        /// The identity the file declares.
        functional_id: String,
        /// The file it was found in, without its `.json` extension.
        file_stem: String,
    },
    /// A `Creature` carries no printed power/toughness, or a non-creature carries
    /// them (ADR 0018 §5).
    PowerToughnessMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the card is a creature — which is to say, which way it is wrong.
        creature: bool,
    },
    /// An `aura` grant appears on a card whose `subtypes` do not include `"Aura"`
    /// (CR 303.4).
    AuraOnNonAura {
        /// The definition at fault.
        functional_id: String,
    },
    /// Two printings in one set claim the same collector number, so one would shadow
    /// the other.
    DuplicatePrinting {
        /// The set the collision is in.
        set_code: String,
        /// The collector number claimed twice.
        collector_number: String,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnObject => write!(
                f,
                "a functional definition must be a single JSON object, one card per file"
            ),
            Self::MalformedField {
                functional_id,
                field,
            } => write!(f, "{functional_id}: `{field}` is missing or the wrong type"),
            Self::UnsupportedSchemaVersion {
                functional_id,
                found,
            } => write!(
                f,
                "{functional_id} declares schema_version {found}; \
                 this engine understands {SCHEMA_VERSION}"
            ),
            Self::MalformedFunctionalId { slug } => write!(
                f,
                "`{slug}` is not a well-formed functional id: expected a lowercase \
                 snake_case slug (e.g. `thornback_boar`)"
            ),
            Self::FileNameMismatch {
                functional_id,
                file_stem,
            } => write!(
                f,
                "{file_stem}.json declares functional_id `{functional_id}`; \
                 a definition's file name must match its identity"
            ),
            Self::PowerToughnessMismatch {
                functional_id,
                creature: true,
            } => write!(f, "{functional_id} is a Creature with no power/toughness"),
            Self::PowerToughnessMismatch {
                functional_id,
                creature: false,
            } => write!(
                f,
                "{functional_id} is not a Creature but carries power/toughness"
            ),
            Self::AuraOnNonAura { functional_id } => write!(
                f,
                "{functional_id} carries an `aura` grant but is not an Aura \
                 (its subtypes do not include `{AURA_SUBTYPE}`)"
            ),
            Self::DuplicatePrinting {
                set_code,
                collector_number,
            } => write!(
                f,
                "two printings in {set_code} claim collector number {collector_number}"
            ),
        }
    }
}

/// Whether `slug` is a well-formed [`FunctionalId`](crate::FunctionalId): a non-empty
/// lowercase `snake_case` identifier starting with a letter, with no doubled or
/// trailing underscore (e.g. `thornback_boar`).
///
/// The single definition of the rule. `FunctionalId::try_from` enforces it on the
/// typed side and `build.rs` enforces it on catalog files, both through this function,
/// so an identity cannot be legal in one place and illegal in the other.
#[must_use]
pub(crate) fn is_well_formed_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.starts_with(|c: char| c.is_ascii_lowercase())
        && !slug.ends_with('_')
        && !slug.contains("__")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Validate one functional definition, returning its `functional_id`.
///
/// `file_stem` is the name of the file the definition came from, without its `.json`
/// extension — `Some` when validating the sharded catalog (where the file name *is*
/// the identity, ADR 0018 §4), and `None` when validating a snapshot that has no file
/// behind it, such as a test fixture or an in-memory array.
///
/// # Errors
/// Returns the first [`Violation`] found. Checks run identity-first, so every later
/// message can name the card it is complaining about.
pub(crate) fn validate_definition(
    file_stem: Option<&str>,
    value: &serde_json::Value,
) -> Result<String, Violation> {
    let object = value.as_object().ok_or(Violation::NotAnObject)?;

    // Identity first: everything below reports against it.
    let functional_id = object
        .get("functional_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Violation::MalformedField {
            functional_id: file_stem.unwrap_or("<unknown>").to_string(),
            field: "functional_id",
        })?
        .to_string();

    if !is_well_formed_slug(&functional_id) {
        return Err(Violation::MalformedFunctionalId {
            slug: functional_id,
        });
    }
    if let Some(stem) = file_stem {
        if stem != functional_id {
            return Err(Violation::FileNameMismatch {
                functional_id,
                file_stem: stem.to_string(),
            });
        }
    }

    let version = object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| Violation::MalformedField {
            functional_id: functional_id.clone(),
            field: "schema_version",
        })?;
    if version != u64::from(SCHEMA_VERSION) {
        return Err(Violation::UnsupportedSchemaVersion {
            functional_id,
            found: version,
        });
    }

    let types = object
        .get("types")
        .and_then(serde_json::Value::as_array)
        .filter(|types| !types.is_empty())
        .ok_or_else(|| Violation::MalformedField {
            functional_id: functional_id.clone(),
            field: "types",
        })?;

    // A Creature carries printed power and toughness; nothing else may (ADR 0018 §5).
    // Checked as a pair: half a P/T is as wrong as none at all on a creature.
    let is_creature = types.iter().any(|t| t.as_str() == Some(CREATURE_TYPE));
    let has_power = object.contains_key("power");
    let has_toughness = object.contains_key("toughness");
    if is_creature != (has_power && has_toughness) || has_power != has_toughness {
        return Err(Violation::PowerToughnessMismatch {
            functional_id,
            creature: is_creature,
        });
    }

    // An `aura` grant is the Aura ability (CR 303.4), so it belongs only on an Aura.
    if object.contains_key("aura") {
        let is_aura = object
            .get("subtypes")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|subtypes| subtypes.iter().any(|s| s.as_str() == Some(AURA_SUBTYPE)));
        if !is_aura {
            return Err(Violation::AuraOnNonAura { functional_id });
        }
    }

    Ok(functional_id)
}

/// Reject two printings in one set claiming the same collector number.
///
/// A set's printings are keyed by `(set_code, collector_number)`, so a repeat would
/// silently shadow the earlier record rather than fail. Shared by `build.rs` and
/// [`PrintingDatabase`](crate::PrintingDatabase) so both reject it identically.
///
/// # Errors
/// Returns [`Violation::DuplicatePrinting`] naming the first repeated number.
pub(crate) fn check_printings<'a>(
    set_code: &str,
    collector_numbers: impl IntoIterator<Item = &'a str>,
) -> Result<(), Violation> {
    let mut seen = std::collections::HashSet::new();
    for collector_number in collector_numbers {
        if !seen.insert(collector_number) {
            return Err(Violation::DuplicatePrinting {
                set_code: set_code.to_string(),
                collector_number: collector_number.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// A minimal valid definition, as parsed JSON, that each test then breaks in one way.
    fn definition(extra: &str) -> serde_json::Value {
        let json = format!(
            r#"{{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                 "types": ["creature"], "mana_cost": "{{G}}", "power": 1, "toughness": 1{extra}}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn a_well_formed_definition_validates_and_yields_its_identity() {
        let id = validate_definition(Some("test_card"), &definition("")).unwrap();
        assert_eq!(id, "test_card");
    }

    #[test]
    fn a_definition_must_be_one_object_not_the_old_monolithic_array() {
        let array = serde_json::from_str(r#"[{"functional_id": "test_card"}]"#).unwrap();
        assert_eq!(
            validate_definition(Some("test_card"), &array).unwrap_err(),
            Violation::NotAnObject
        );
    }

    #[test]
    fn an_unrecognized_schema_version_is_rejected() {
        let mut card = definition("");
        card["schema_version"] = serde_json::json!(SCHEMA_VERSION + 1);
        assert_eq!(
            validate_definition(Some("test_card"), &card).unwrap_err(),
            Violation::UnsupportedSchemaVersion {
                functional_id: "test_card".to_string(),
                found: u64::from(SCHEMA_VERSION) + 1,
            }
        );
    }

    #[test]
    fn a_functional_id_that_does_not_match_its_file_name_is_rejected() {
        assert_eq!(
            validate_definition(Some("some_other_file"), &definition("")).unwrap_err(),
            Violation::FileNameMismatch {
                functional_id: "test_card".to_string(),
                file_stem: "some_other_file".to_string(),
            }
        );
    }

    #[test]
    fn a_snapshot_with_no_file_behind_it_skips_the_file_name_check() {
        assert!(validate_definition(None, &definition("")).is_ok());
    }

    #[test]
    fn an_ill_formed_slug_is_rejected() {
        for slug in [
            "Thornback_Boar",
            "thornback boar",
            "9lives",
            "trailing_",
            "double__bar",
        ] {
            let mut card = definition("");
            card["functional_id"] = serde_json::json!(slug);
            assert_eq!(
                validate_definition(None, &card).unwrap_err(),
                Violation::MalformedFunctionalId {
                    slug: slug.to_string()
                },
                "expected `{slug}` to be rejected"
            );
        }
    }

    #[test]
    fn well_formed_slugs_are_accepted() {
        for slug in [
            "forest",
            "thornback_boar",
            "cleric_of_the_sunwell",
            "b2_bomber",
        ] {
            assert!(
                is_well_formed_slug(slug),
                "expected `{slug}` to be accepted"
            );
        }
    }

    #[test]
    fn a_creature_without_power_and_toughness_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["creature"], "mana_cost": "{G}"}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::PowerToughnessMismatch {
                functional_id: "test_card".to_string(),
                creature: true,
            }
        );
    }

    #[test]
    fn a_creature_with_only_half_a_power_toughness_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["creature"], "mana_cost": "{G}", "power": 2}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(validate_definition(None, &card).is_err());
    }

    #[test]
    fn a_non_creature_carrying_power_and_toughness_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["instant"], "mana_cost": "{R}", "power": 1, "toughness": 1}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::PowerToughnessMismatch {
                functional_id: "test_card".to_string(),
                creature: false,
            }
        );
    }

    #[test]
    fn an_aura_grant_on_a_card_that_is_not_an_aura_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["enchantment"], "subtypes": ["Shrine"], "mana_cost": "{G}",
                       "aura": {"enchant": "any_creature", "power": 1, "toughness": 1}}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::AuraOnNonAura {
                functional_id: "test_card".to_string()
            }
        );
    }

    #[test]
    fn an_aura_grant_on_an_aura_is_accepted() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["enchantment"], "subtypes": ["Aura"], "mana_cost": "{G}",
                       "aura": {"enchant": "any_creature", "power": 1, "toughness": 1}}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(validate_definition(None, &card).is_ok());
    }

    #[test]
    fn a_definition_with_no_types_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": [], "mana_cost": "{G}"}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::MalformedField {
                functional_id: "test_card".to_string(),
                field: "types",
            }
        );
    }

    #[test]
    fn duplicate_collector_numbers_in_one_set_are_rejected() {
        assert!(check_printings("FIX", ["1", "2", "3"]).is_ok());
        assert_eq!(
            check_printings("FIX", ["1", "2", "1"]).unwrap_err(),
            Violation::DuplicatePrinting {
                set_code: "FIX".to_string(),
                collector_number: "1".to_string(),
            }
        );
    }
}
