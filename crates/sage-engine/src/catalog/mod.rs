//! Catalog schema validation, shared verbatim by `build.rs` and the loader.
//!
//! This module is compiled **twice**: once as part of the engine (`mod catalog`), and
//! once by `crates/sage-engine/build.rs`, which pulls these exact files in with
//! `#[path = "src/catalog/mod.rs"] mod catalog;`. That is why it depends on nothing but
//! `std` and `serde_json`, and never names a `crate::` path: `build.rs` is compiled
//! *before* the engine exists, so it cannot borrow the engine's types. **Anything added
//! to a submodule here inherits both constraints** — and the `mod.rs` file name is what
//! makes the two compilations agree on where the submodules are (see `build.rs`).
//!
//! Compiling one module in both places is what makes ADR 0008 §5's promise —
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
//!   the schema version, the authored identity, the type/P&T and attachment invariants.
//! - **In the type system**: rules serde already makes unrepresentable. Every
//!   targeting [`Effect`](crate::Effect) variant declares `target: TargetSpec` as a
//!   required field, as does [`Attachment::attach_to`](crate::Attachment::attach_to), so
//!   "an effect that needs a target spec but has none" cannot be written down — it is
//!   a parse error, not a validation failure. No check here re-states it.
//! - **In the loader**: the one rule that is impossible to check here — whether a
//!   definition's `scripted` flag agrees with `crates/sage-engine/src/scripted.rs`.
//!   That answer lives in compiled Rust, which does not exist yet when `build.rs`
//!   runs, so [`CardDatabase::from_json`](crate::CardDatabase::from_json) owns it (in
//!   both directions — ADR 0008 §5).

use std::fmt;

/// The functional-definition schema version this engine understands (ADR 0008 §2).
///
/// Re-exported as `sage_engine::SCHEMA_VERSION`. A definition declaring any other
/// version is a hard error ([`Violation::UnsupportedSchemaVersion`]), never a silent
/// skip: a breaking change to the schema's shape bumps this, so the whole catalog is
/// migrated under one forcing function instead of half-loading.
pub const SCHEMA_VERSION: u32 = 1;

/// The subtype that makes a card an Aura (CR 303.4), and therefore the only kind of
/// card an `attachment` of kind `aura` may appear on.
const AURA_SUBTYPE: &str = "Aura";

/// The subtype that makes a card an Equipment (CR 301.5) — the Equipment counterpart of
/// [`AURA_SUBTYPE`], and the only kind of card an `attachment` of kind `equipment` may
/// appear on.
const EQUIPMENT_SUBTYPE: &str = "Equipment";

/// The card type that requires printed power and toughness.
const CREATURE_TYPE: &str = "creature";

/// The card type that requires a printed starting loyalty (CR 306.5b).
const PLANESWALKER_TYPE: &str = "planeswalker";

/// The one card type that is **played** rather than cast (CR 116.2a), and therefore
/// the one an additional *cast* cost could never apply to.
const LAND_TYPE: &str = "land";

/// The card types a permanent may have (CR 110.1) — and therefore the only types a
/// **token** may have, a token existing nowhere but the battlefield (CR 111.7).
const PERMANENT_TYPES: [&str; 6] = [
    "land",
    "creature",
    "artifact",
    "enchantment",
    "planeswalker",
    "battle",
];

mod effects;
mod violation;

use effects::*;
pub use violation::*;

/// Whether `slug` is a well-formed [`FunctionalId`](crate::FunctionalId): a non-empty
/// lowercase `snake_case` identifier starting with a letter, with no doubled or
/// trailing underscore (e.g. `onakke_ogre`).
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
/// the identity, ADR 0008 §4), and `None` when validating a snapshot that has no file
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

    // A Creature carries printed power and toughness; nothing else may (ADR 0008 §5).
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

    // A Planeswalker carries a printed starting loyalty and nothing else does
    // (CR 306.5b) — the same both-directions pairing the P/T check above makes, and for
    // the same reason: a planeswalker with none would die to CR 704.5i on arrival.
    let is_planeswalker = types.iter().any(|t| t.as_str() == Some(PLANESWALKER_TYPE));
    if is_planeswalker != object.contains_key("loyalty") {
        return Err(Violation::LoyaltyMismatch {
            functional_id,
            planeswalker: is_planeswalker,
        });
    }

    // An additional cast cost belongs only on a card that is cast, and must actually
    // cost something. A land is played rather than cast (CR 116.2a), so no cast gate
    // would ever consult the cost; a zero-count discard is a cost in name only.
    if let Some(cost) = object.get("additional_cost") {
        let is_land = types.iter().any(|t| t.as_str() == Some(LAND_TYPE));
        let count = cost.get("count").and_then(serde_json::Value::as_u64);
        if is_land || count == Some(0) {
            return Err(Violation::AdditionalCostIsUnpayable { functional_id });
        }
    }

    // A printed combat restriction only ever restricts attacking or blocking, so it
    // belongs only on a creature. An Aura *grants* restrictions to its host through
    // `aura.restrictions` instead, which is why this looks only at the printed list.
    if object.contains_key("restrictions") && !is_creature {
        return Err(Violation::RestrictionsOnNonCreature { functional_id });
    }

    // An optional effect forwards the target group of the one effect it wraps, so it may
    // wrap one targeting effect and no more (see [`Violation::TwoTargetsInsideOptional`]).
    // Walked to any depth, so a `may` inside a `may` is counted too.
    if every_effect(object)
        .into_iter()
        .any(optional_declares_two_targets)
    {
        return Err(Violation::TwoTargetsInsideOptional { functional_id });
    }

    // A conditional's branches get no such forwarding: two branches share one flat
    // target list, so a group named in either could not be paired back onto the branch
    // that was taken.
    if every_effect(object)
        .into_iter()
        .any(conditional_wraps_a_target)
    {
        return Err(Violation::TargetInsideConditional { functional_id });
    }

    // A power bound is the one selector field read from computed characteristics, which
    // the layer system cannot ask for from inside itself without recursing. Two sites are
    // evaluated there: a static ability's condition, and an attachment's counted grant.
    if static_condition_counts_by_power(object) {
        return Err(Violation::PowerInStaticCondition { functional_id });
    }
    if attachment_counts_by_power(object) {
        return Err(Violation::PowerInAttachmentCount { functional_id });
    }

    // CR 114.1: an emblem has no characteristics but its abilities, and only the two
    // kinds that need neither an activation nor an entry event can function on one.
    if every_effect(object).into_iter().any(emblem_ability_is_bad) {
        return Err(Violation::EmblemAbilityIsNotStaticOrTriggered { functional_id });
    }

    // A layer-6 change that neither subtracts nor adds is no effect at all, and every
    // field of one defaults — so the empty clause is the shape a typo lands on.
    if every_effect(object)
        .into_iter()
        .any(ability_change_is_empty)
    {
        return Err(Violation::AbilityChangeIsEmpty { functional_id });
    }

    // At most one "up to N" target group per ability or spell, so the flat stored target
    // list pairs back onto effects unambiguously.
    if effect_lists(object)
        .into_iter()
        .any(|effects| variable_target_groups(&effects) > 1)
    {
        return Err(Violation::TwoVariableTargetGroups { functional_id });
    }

    // Every token a definition creates must be an object that could exist: a permanent
    // (CR 110.1/111.7), with power and toughness exactly when it is a creature. Walked
    // to any depth, so a `create_token` nested inside a `may` is checked too.
    for effect in every_effect(object) {
        if effect.get("kind").and_then(serde_json::Value::as_str) != Some("create_token") {
            continue;
        }
        validate_token(&functional_id, effect.get("token"))?;
    }

    // An `attachment` block is the Aura ability (CR 303.4) or the Equipment's equip
    // ability (CR 301.5), so the card must actually bear the subtype it claims — and, for
    // an Equipment, must name the cost its derived equip ability charges (CR 702.6a).
    if let Some(attachment) = object.get("attachment") {
        let kind = attachment.get("kind").and_then(serde_json::Value::as_str);
        let subtype = match kind {
            Some("equipment") => EQUIPMENT_SUBTYPE,
            // Anything else is either `aura` or a value serde will reject when the
            // definition is deserialized; either way the Aura subtype is what to demand.
            _ => AURA_SUBTYPE,
        };
        let bears_subtype = object
            .get("subtypes")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|subtypes| subtypes.iter().any(|s| s.as_str() == Some(subtype)));
        if !bears_subtype {
            return Err(Violation::AttachmentSubtypeMismatch {
                functional_id,
                subtype,
            });
        }
        // The both-directions pairing the P/T and loyalty checks make, for the same
        // reason: an Equipment with no equip cost could never be attached to anything,
        // and an equip cost on an Aura names an ability it does not have.
        let is_equipment = kind == Some("equipment");
        if is_equipment != attachment.get("equip").is_some_and(|c| !c.is_null()) {
            return Err(Violation::EquipCostMismatch {
                functional_id,
                equipment: is_equipment,
            });
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
mod tests;
