//! The deterministic card-compatibility report (issue #258).
//!
//! M3's exit criterion asks for "a deterministic compatibility report naming every
//! supported and excluded card". This module is the single source of truth for that
//! artifact: [`render_report`] turns the interned catalog plus a curated exclusion
//! list into the Markdown committed at `docs/generated/compatibility.md`. The binary
//! `src/bin/gen-compat.rs` writes it; a `#[test]` in `tests/compat.rs` regenerates it
//! in memory and fails if the committed copy has drifted, so the report can never go
//! stale the way the hand-maintained coverage ledger did (#252).
//!
//! **This does not weaken "zero I/O in the engine."** Rendering is a pure function of
//! its inputs — no clock, no randomness, no filesystem. The exclusion list is baked in
//! at compile time with `include_str!` (the ADR 0006 pattern the catalog already
//! uses), not read at runtime; only the *generator* binary and the *test* touch the
//! filesystem, and neither ships in the running engine.
//!
//! Legal posture: the report and the exclusions data carry **names and blockers only**
//! — never Oracle text, flavor text, or official branding (ADR 0018's schema posture
//! extends here). [`Exclusion`] uses `deny_unknown_fields`, so a stray `text` field is
//! a parse error, not a leak.

use std::fmt;

use serde::Deserialize;

use crate::card::CardDatabase;
use crate::id::CardId;

/// One curated exclusion: a card or mechanic that was considered and is deliberately
/// out of scope, named alongside the single blocker that keeps it out.
///
/// Authored by hand in `data/exclusions.json`. **Names and blockers only** — the
/// `deny_unknown_fields` guard makes any attempt to add rules prose a compile/parse
/// error rather than a silent legal-posture violation.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    /// The excluded card or mechanic, e.g. `"Planeswalkers"`.
    pub name: String,
    /// The named blocker, e.g. `"no loyalty counter system or loyalty abilities"`.
    pub blocker: String,
}

/// The curated exclusion list, embedded from `data/exclusions.json` at compile time.
///
/// A malformed file is a hard error surfaced by [`bundled_exclusions`], never a silent
/// empty list — an empty exclusions section would falsely imply "everything considered
/// is supported".
const EXCLUSIONS_JSON: &str = include_str!("../data/exclusions.json");

/// Parse the embedded exclusion list. Pure — `include_str!` resolves at compile time,
/// so this reads no file at runtime.
pub fn bundled_exclusions() -> Result<Vec<Exclusion>, CompatError> {
    serde_json::from_str(EXCLUSIONS_JSON).map_err(|err| CompatError::Exclusions(err.to_string()))
}

/// Why report generation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompatError {
    /// The embedded `exclusions.json` could not be parsed (message included).
    Exclusions(String),
    /// A name appears both as a supported catalog card and in the exclusion list — the
    /// two sections must be disjoint, or the report contradicts itself.
    SupportedAndExcluded(String),
    /// A `CardId` in `0..len` was not present in the catalog — an internal invariant
    /// violation that would leave a catalog card out of the report. Reported rather
    /// than silently dropped so "every card appears" cannot fail quietly.
    MissingCard(u64),
}

impl fmt::Display for CompatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exclusions(err) => write!(f, "cannot parse data/exclusions.json: {err}"),
            Self::SupportedAndExcluded(name) => write!(
                f,
                "`{name}` appears as both a supported card and an exclusion; \
                 the two sections must be disjoint"
            ),
            Self::MissingCard(id) => {
                write!(
                    f,
                    "CardId({id}) is missing from the catalog it was counted in"
                )
            }
        }
    }
}

impl std::error::Error for CompatError {}

/// The header stamped at the top of the generated file, warning humans off editing it
/// and naming the one command that regenerates it. Kept in the artifact itself so the
/// instruction travels with the file.
const HEADER: &str = "\
<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p rune-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/rune-engine/data/catalog/ + crates/rune-engine/data/exclusions.json (issue #258). -->\n";

/// Escape the Markdown table cell delimiter so a `|` in a name never breaks a row.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// Render the compatibility report as deterministic Markdown.
///
/// Determinism (issue #258 acceptance): supported cards are emitted in the catalog's
/// **interned order** (`FunctionalId`s sorted by byte value — the order `CardId(0..n)`
/// already imposes, identical on every platform), and exclusions are sorted by name.
/// There are no timestamps. Running this twice over the same inputs yields byte-
/// identical output.
///
/// Consistency guard: every catalog card appears exactly once as supported (by
/// construction — the loop walks the whole catalog), and a name that is both a
/// supported card and an exclusion is a [`CompatError::SupportedAndExcluded`] rather
/// than a self-contradicting report.
pub fn render_report(db: &CardDatabase, exclusions: &[Exclusion]) -> Result<String, CompatError> {
    // Collect supported cards in interned order (CardId is the array index).
    let mut supported: Vec<(&str, &str, bool)> = Vec::with_capacity(db.len());
    for index in 0..db.len() as u64 {
        let card = db
            .card(CardId(index))
            .ok_or(CompatError::MissingCard(index))?;
        supported.push((
            card.functional_id.as_str(),
            card.name.as_str(),
            card.scripted,
        ));
    }

    // The two sections must be disjoint: an excluded name that is actually supported
    // would make the report contradict itself.
    for exclusion in exclusions {
        if supported.iter().any(|(_, name, _)| *name == exclusion.name) {
            return Err(CompatError::SupportedAndExcluded(exclusion.name.clone()));
        }
    }

    let mut sorted_exclusions: Vec<&Exclusion> = exclusions.iter().collect();
    sorted_exclusions.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str("# Card compatibility report\n\n");
    out.push_str(
        "RUNE supports only the verified slice of cards in its catalog, never a full set. \
         This report is generated from the catalog and the curated exclusion list — the \
         checkable artifact behind that claim (issue #258).\n\n",
    );

    out.push_str(&format!("## Supported cards ({})\n\n", supported.len()));
    out.push_str(
        "Every functional definition in `crates/rune-engine/data/catalog/`, in interned \
         order. \"Implementation\" is whether the card's behavior lives in its data \
         definition or (also) in the `scripted` code escape hatch (ADR 0018 §2).\n\n",
    );
    out.push_str("| Functional ID | Name | Implementation |\n");
    out.push_str("| --- | --- | --- |\n");
    for (functional_id, name, scripted) in &supported {
        let implementation = if *scripted {
            "scripted"
        } else {
            "functional definition"
        };
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            cell(functional_id),
            cell(name),
            implementation,
        ));
    }
    out.push('\n');

    out.push_str(&format!("## Excluded ({})\n\n", sorted_exclusions.len()));
    out.push_str(
        "Cards and mechanics considered and deliberately left out of scope, each with the \
         blocker that keeps it out. Names and blockers only — no rules text. Curated by \
         hand in `crates/rune-engine/data/exclusions.json`.\n\n",
    );
    out.push_str("| Excluded | Blocker |\n");
    out.push_str("| --- | --- |\n");
    for exclusion in &sorted_exclusions {
        out.push_str(&format!(
            "| {} | {} |\n",
            cell(&exclusion.name),
            cell(&exclusion.blocker),
        ));
    }

    Ok(out)
}

/// Render the report from the bundled catalog + bundled exclusions — the exact bytes
/// the generator writes and the freshness test checks. Fails if the catalog cannot be
/// loaded or the two sections overlap.
pub fn bundled_report() -> Result<String, Box<dyn std::error::Error>> {
    let db = CardDatabase::bundled()?;
    let exclusions = bundled_exclusions()?;
    Ok(render_report(&db, &exclusions)?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let db = CardDatabase::bundled().unwrap();
        let exclusions = bundled_exclusions().unwrap();
        let a = render_report(&db, &exclusions).unwrap();
        let b = render_report(&db, &exclusions).unwrap();
        assert_eq!(a, b, "the report must be byte-identical across runs");
    }

    #[test]
    fn every_catalog_card_appears_exactly_once() {
        let db = CardDatabase::bundled().unwrap();
        let report = render_report(&db, &bundled_exclusions().unwrap()).unwrap();
        for index in 0..db.len() as u64 {
            let card = db.card(CardId(index)).unwrap();
            let needle = format!("| `{}` |", card.functional_id.as_str());
            assert_eq!(
                report.matches(&needle).count(),
                1,
                "{} must appear exactly once as supported",
                card.functional_id.as_str()
            );
        }
    }

    #[test]
    fn exclusions_render_with_their_blockers() {
        let db = CardDatabase::bundled().unwrap();
        let exclusions = bundled_exclusions().unwrap();
        assert!(
            !exclusions.is_empty(),
            "the curated exclusion list is non-empty"
        );
        let report = render_report(&db, &exclusions).unwrap();
        for exclusion in &exclusions {
            assert!(report.contains(&exclusion.name), "excluded name is listed");
            assert!(report.contains(&exclusion.blocker), "its blocker is listed");
        }
    }

    #[test]
    fn a_supported_card_cannot_also_be_excluded() {
        let db = CardDatabase::bundled().unwrap();
        // Take a real catalog card's name and try to exclude it.
        let name = db.card(CardId(0)).unwrap().name.clone();
        let exclusions = vec![Exclusion {
            name: name.clone(),
            blocker: "contrived".into(),
        }];
        assert_eq!(
            render_report(&db, &exclusions),
            Err(CompatError::SupportedAndExcluded(name)),
        );
    }

    #[test]
    fn exclusions_reject_rules_prose_fields() {
        // The legal posture: only `name` + `blocker`. An extra field (e.g. Oracle text)
        // is a parse error, not a silent leak.
        let with_text = r#"[{"name": "X", "blocker": "y", "text": "Flying"}]"#;
        assert!(serde_json::from_str::<Vec<Exclusion>>(with_text).is_err());
    }
}
