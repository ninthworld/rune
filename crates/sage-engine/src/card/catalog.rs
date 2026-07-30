//! Catalog loading and parsing.

use super::card_data::CardData;
use super::database::CatalogEntry;
use super::error::CatalogError;
use super::printing::SetSnapshot;
use crate::catalog::validate_definition;
use crate::id::CardId;

// The generated catalog manifest: `pub(crate) const CATALOG: &[CatalogEntry]` and
// `pub(crate) const SET_MANIFEST: &[SetSnapshot]`, both `include_str!`-embedding the files under
// `data/` (ADR 0018 §4). `build.rs` writes it; nothing here is hand-maintained, so
// adding a card edits zero existing lines. The engine still does zero runtime I/O:
// this is the same compile-time embedding ADR 0006 sanctioned, with the build script
// — not a human — authoring the `include_str!` list.
include!(concat!(env!("OUT_DIR"), "/catalog_manifest.rs"));

/// Parse one functional definition from its JSON text, validating it first.
///
/// `file_stem` is the catalog file the definition came from, so the file-name rule can
/// be checked; `None` for a snapshot with no file behind it.
pub(super) fn parse_definition(
    file_stem: Option<&str>,
    json: &str,
) -> Result<CardData, CatalogError> {
    parse_value(file_stem, serde_json::from_str(json)?)
}

/// Validate an already-parsed definition and deserialize it into [`CardData`].
///
/// Two tiers, and both are load-bearing:
///
/// 1. [`validate_definition`] — the schema rules, run from the same source file
///    `build.rs` runs them from, so build time and load time cannot disagree.
/// 2. `serde_json::from_value` — the type system. `deny_unknown_fields` rejects a
///    presentation asset, and every targeting [`Effect`](crate::ability::Effect) declares its [`TargetSpec`](crate::ability::TargetSpec) as
///    a required field, so an effect that needs a target and lacks one fails *here*, as
///    a parse error. That is why no validator restates it (ADR 0018 §5).
///
/// [`CardData`] is the direct deserialization target rather than a field of a wrapper,
/// because serde does not enforce `deny_unknown_fields` through a `flatten`ed field.
pub(super) fn parse_value(
    file_stem: Option<&str>,
    value: serde_json::Value,
) -> Result<CardData, CatalogError> {
    validate_definition(file_stem, &value)?;
    Ok(serde_json::from_value(value)?)
}
