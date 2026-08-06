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

/// The most modes a modal card may print (CR 700.2).
///
/// A rules-free bound with a presentation reason, and it is stated here rather than
/// worked around later: a mode is a numbered row in a dock band of fixed height, sized
/// to hold four of them at the text floor (`docs/client-design.md` §6.7). A fifth is not
/// a layout to degrade at render time — degrading would mean truncating a mode a player
/// has to read before choosing it — so it is a card the catalog refuses. Every modal
/// card in the catalog has two.
pub const MAX_MODES: usize = 4;

/// The fewest a modal card may print. One mode is not a choice, and a card whose single
/// bullet was authored as a mode would pose a question with one answer.
const MIN_MODES: usize = 2;

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
mod face;
mod violation;

use effects::*;
use face::{transforms_itself, validate_back_face, validate_face};
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

    // Everything a *face* must obey, asked once per face (CR 712.2) — the printed
    // characteristics and the effects it authors. A single-faced card asks it once; a
    // transforming double-faced card asks it of both sides, so a back face's types,
    // loyalty, and abilities are held to exactly the rules its front's are.
    validate_face(&functional_id, object)?;

    // The card types are needed once more below, for the one rule that is about the
    // *card* rather than about a face: a land is played, never cast.
    let types = object
        .get("types")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    // An additional cast cost belongs only on a card that is cast, and must actually
    // cost something. A land is played rather than cast (CR 116.2a), so no cast gate
    // would ever consult the cost; a zero-count discard is a cost in name only.
    if let Some(cost) = object.get("additional_cost") {
        let is_land = types.iter().any(|t| t.as_str() == Some(LAND_TYPE));
        // A discard states its count as a bare number; a sacrifice states it as
        // `{"exactly": n}` or `"any"`. Zero is no cost either way, and `"any"` — whose
        // legal payments *include* zero — is a real cost because the player may pay more.
        let count = cost
            .get("count")
            .and_then(|count| count.as_u64().or_else(|| count.get("exactly")?.as_u64()));
        if is_land || count == Some(0) {
            return Err(Violation::AdditionalCostIsUnpayable { functional_id });
        }
    }

    // CR 700.2: a modal card's effects live in its modes and nowhere else, it prints
    // between two and MAX_MODES of them, and each of its modes does something. All three
    // directions matter: loose spell effects beside modes would resolve whichever mode
    // was chosen, one mode is a question with a single answer, and a fifth is a row the
    // dock has no band for.
    if let Some(modes) = object.get("modes").and_then(serde_json::Value::as_array) {
        let empty_mode = modes.iter().any(|mode| {
            mode.get("effects")
                .and_then(serde_json::Value::as_array)
                .is_none_or(Vec::is_empty)
        });
        let loose_effects = object
            .get("spell_effects")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|effects| !effects.is_empty());
        if modes.len() < MIN_MODES || modes.len() > MAX_MODES || empty_mode || loose_effects {
            return Err(Violation::MalformedModes {
                functional_id,
                modes: modes.len(),
            });
        }
    }

    // CR 107.3: `{X}` is announced as a spell is cast, so it belongs in a card's mana
    // cost and nowhere else. An activation pays out of a pool with no announcement step
    // to fix a value in, so an `{X}` in an ability's cost is a symbol nothing would ever
    // charge for — it would simply be ignored and the ability activated for free.
    if let Some(cost) = ability_cost_with_x(object) {
        return Err(Violation::XOutsideAManaCost {
            functional_id,
            cost,
        });
    }

    // A spell trait that names a threshold for X is a sentence about a value the card
    // never asks for unless its cost prints `{X}` — "if X is 5 or more" on a fixed cost
    // is a clause that can never be true.
    if spell_trait_needs_x(object) && !mana_cost_has_x(object) {
        return Err(Violation::SpellTraitNeedsX { functional_id });
    }

    // An amount read off a cost payment needs a cost that pays it, or the card reads as
    // doing something and always does nothing.
    if reads_an_unpaid_amount(object) {
        return Err(Violation::PaymentAmountIsNeverPaid { functional_id });
    }

    // The same pairing for the other half of CR 614.12: "permanents with the chosen name"
    // is a selector whose referent is the card's own naming ability, and a card that
    // selects on it without one has written a class nothing can ever join.
    if selects_the_named_card(object) && !object_names_a_card(object) {
        return Err(Violation::ChosenNameIsNeverNamed { functional_id });
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

    // CR 712.2: a transforming double-faced card's second face, held to every rule its
    // first is held to, plus the one that is only about a back face — it has no mana
    // cost and can never be cast (CR 712.4a).
    validate_back_face(&functional_id, object)?;

    // CR 701.28d: turning a permanent over needs somewhere to turn it to. A card that
    // says so with no `back_face` has written an ability that can never do anything, and
    // the engine's answer to it — leave the permanent as it is — is silent by design.
    if transforms_itself(object) && !object.contains_key("back_face") {
        return Err(Violation::TransformWithoutABackFace { functional_id });
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

/// Whether this definition's printed mana cost contains an `{X}` (CR 107.3).
fn mana_cost_has_x(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    object
        .get("mana_cost")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|cost| cost.contains("{X}"))
}

/// The first activation cost string in this definition that contains an `{X}`, if any.
///
/// Walks the costs of every authored ability, including the abilities an emblem hands
/// out — the same reach [`every_effect`] has, for the same reason.
fn ability_cost_with_x(object: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    fn costs_of(ability: &serde_json::Value) -> Vec<&serde_json::Value> {
        ability
            .get("cost")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .collect()
    }
    let printed = object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter();
    let granted: Vec<&serde_json::Value> = every_effect(object)
        .into_iter()
        .flat_map(|effect| {
            effect
                .get("abilities")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
        })
        .collect();
    printed
        .chain(granted)
        .flat_map(costs_of)
        .filter_map(|cost| cost.get("mana").and_then(serde_json::Value::as_str))
        .find(|mana| mana.contains("{X}"))
        .map(str::to_string)
}

/// Whether any declared spell trait names an X threshold (`if_x_at_least`).
fn spell_trait_needs_x(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    object
        .get("spell_traits")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .any(|declared| declared.get("if_x_at_least").is_some_and(|v| !v.is_null()))
}
