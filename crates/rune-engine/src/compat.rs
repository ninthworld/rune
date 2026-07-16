//! The card-compatibility report: a deterministic, generated inventory of every card
//! the engine supports and every mechanic deliberately excluded (issue #258).
//!
//! RUNE's support claim — "only the verified slice, never a full set" (`docs/brief.md`,
//! `docs/roadmap.md` M3) — rests on a *checkable artifact* rather than prose. This
//! module builds that artifact from two sources: the bundled catalog (every supported
//! card) and a small authored `data/exclusions.json` (mechanics considered and left
//! out, each with its blocker). The committed report lives at
//! `docs/generated/compatibility.md`; a test regenerates it and diffs, so it can never
//! go stale — the failure mode that killed the hand-maintained coverage ledger (#252).
//!
//! Generation is **pure and deterministic**: same catalog + same exclusions in, the
//! byte-identical report out. Supported cards are listed in interned order (sorted
//! `functional_id`, ADR 0018 §3); exclusions are sorted by name; nothing carries a
//! timestamp. The engine still does zero runtime I/O — the exclusions are embedded at
//! compile time with [`include_str!`], exactly as the catalog is.
//!
//! **Legal posture.** The report and the exclusions file carry names and blockers
//! only — no Oracle text, flavor text, or branding. On the exclusions that is
//! structural: [`Exclusion`] is `deny_unknown_fields`, so a stray `oracle_text` field
//! fails to parse rather than being ignored (`docs/brief.md` Legal Considerations).

use std::collections::HashSet;
use std::fmt;
use std::fmt::Write as _;

use serde::Deserialize;

use crate::card::{CardData, CardDatabase};
use crate::id::CardId;

/// The bundled exclusions, embedded at compile time (ADR 0006 — no runtime I/O).
const EXCLUSIONS_JSON: &str = include_str!("../data/exclusions.json");

/// One authored exclusion: a mechanic (or card) considered and deliberately left out
/// of scope, and the named blocker keeping it there.
///
/// Names and blockers only. The schema's legal posture extends here: `deny_unknown_fields`
/// makes "no Oracle text" structural, so a field beyond these two fails the load rather
/// than being silently ignored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    /// The excluded mechanic or card, by name.
    pub name: String,
    /// The concrete reason it is out of scope (e.g. "no loyalty system").
    pub blocker: String,
}

/// Everything that can go wrong generating the report (ADR 0018 §5 discipline: a bad
/// input is an error naming the offender, never a half-built artifact).
#[derive(Debug)]
pub enum CompatError {
    /// `data/exclusions.json` is not a valid array of [`Exclusion`]s — including an
    /// unknown field (a presentation asset) rejected by `deny_unknown_fields`.
    Json(serde_json::Error),
    /// A name appears as both a supported card and an exclusion — the report would
    /// claim the same thing is in and out of scope.
    SupportedAndExcluded {
        /// The name claimed on both sides.
        name: String,
    },
    /// Two exclusions share a name, so one would shadow the other.
    DuplicateExclusion {
        /// The name claimed twice.
        name: String,
    },
}

impl fmt::Display for CompatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(err) => write!(f, "data/exclusions.json does not match the schema: {err}"),
            Self::SupportedAndExcluded { name } => write!(
                f,
                "`{name}` is listed as both a supported card and an excluded mechanic"
            ),
            Self::DuplicateExclusion { name } => {
                write!(f, "two exclusions claim the name `{name}`")
            }
        }
    }
}

impl std::error::Error for CompatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

/// Parse the bundled exclusions.
fn bundled_exclusions() -> Result<Vec<Exclusion>, CompatError> {
    serde_json::from_str(EXCLUSIONS_JSON).map_err(CompatError::Json)
}

/// Generate the compatibility report from `db` and the bundled exclusions.
///
/// # Errors
/// Returns a [`CompatError`] if the exclusions do not parse or are inconsistent with
/// the catalog (a name that is both supported and excluded, or a duplicated exclusion).
pub fn compatibility_report(db: &CardDatabase) -> Result<String, CompatError> {
    report_from(db, bundled_exclusions()?)
}

/// The generator core, taking exclusions explicitly so tests can drive the consistency
/// guards with a synthetic catalog and synthetic exclusions.
fn report_from(db: &CardDatabase, mut exclusions: Vec<Exclusion>) -> Result<String, CompatError> {
    // Supported: every catalog card, in interned order — `build.rs` assigns `CardId`s
    // 0..n by sorted `functional_id` (ADR 0018 §3), so this iteration is already the
    // stable, deterministic ordering the report needs, with every card present by
    // construction (nothing can be silently missing).
    // `filter_map` rather than an unwrap: every interned handle resolves by
    // construction, but the engine forbids panicking APIs (`docs/coding-standards.md`),
    // and a missing handle would simply (and harmlessly) be omitted.
    let supported: Vec<&CardData> = (0..db.len() as u64)
        .map(CardId)
        .filter_map(|id| db.card(id))
        .collect();
    let supported_names: HashSet<&str> = supported.iter().map(|c| c.name.as_str()).collect();

    // Consistency guards: a name cannot be both supported and excluded, and no
    // exclusion may repeat.
    let mut seen = HashSet::new();
    for ex in &exclusions {
        if supported_names.contains(ex.name.as_str()) {
            return Err(CompatError::SupportedAndExcluded {
                name: ex.name.clone(),
            });
        }
        if !seen.insert(ex.name.as_str()) {
            return Err(CompatError::DuplicateExclusion {
                name: ex.name.clone(),
            });
        }
    }
    exclusions.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = String::new();
    // The header names the regeneration command and the sources, so a reader who hits
    // the freshness failure knows exactly what to run.
    let _ = writeln!(out, "# Card compatibility report");
    let _ = writeln!(out);
    let _ = writeln!(out, "<!--");
    let _ = writeln!(out, "GENERATED FILE — do not edit by hand.");
    let _ = writeln!(
        out,
        "Regenerate with:  cargo test -p rune-engine regenerate_compatibility_report -- --ignored"
    );
    let _ = writeln!(
        out,
        "Sources:          crates/rune-engine/data/catalog/  +  crates/rune-engine/data/exclusions.json"
    );
    let _ = writeln!(out, "-->");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "RUNE claims support only for the verified slice of cards listed here — never a \
         full set. Every supported card is a hand-authored functional definition \
         (ADR 0018); the excluded list names mechanics considered and deliberately left \
         out of scope, each with the blocker keeping it there. No Oracle text, flavor \
         text, or branding appears in this report or its sources."
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## Supported cards ({})", supported.len());
    let _ = writeln!(out);
    let _ = writeln!(out, "| functional_id | name | implementation |");
    let _ = writeln!(out, "| --- | --- | --- |");
    for card in &supported {
        // Every bundled card today is a data-driven functional definition; a card
        // using the ADR 0007 escape hatch reads as `scripted`.
        let implementation = if card.scripted {
            "scripted"
        } else {
            "functional"
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            card.functional_id, card.name, implementation
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Excluded mechanics ({})", exclusions.len());
    let _ = writeln!(out);
    let _ = writeln!(out, "| mechanic | blocker |");
    let _ = writeln!(out, "| --- | --- |");
    for ex in &exclusions {
        let _ = writeln!(out, "| {} | {} |", ex.name, ex.blocker);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

    use super::*;

    /// The committed report's path, relative to the repository root — named in the
    /// freshness-failure message and written by the regeneration test.
    const REPORT_PATH: &str = "docs/generated/compatibility.md";

    /// The committed report, read from the repository.
    fn committed_report() -> std::io::Result<String> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../",
            "docs/generated/compatibility.md"
        );
        std::fs::read_to_string(path)
    }

    #[test]
    fn the_report_lists_every_supported_card_and_every_exclusion() {
        let db = CardDatabase::bundled().unwrap();
        let report = compatibility_report(&db).unwrap();

        // Every catalog card appears by its functional_id...
        for id in (0..db.len() as u64).map(CardId) {
            let card = db.card(id).unwrap();
            assert!(
                report.contains(card.functional_id.as_str()),
                "the report omits {}",
                card.functional_id
            );
        }
        // ...and every authored exclusion appears with its blocker.
        for ex in bundled_exclusions().unwrap() {
            assert!(
                report.contains(&ex.name),
                "the report omits exclusion {}",
                ex.name
            );
            assert!(report.contains(&ex.blocker));
        }
        // The support-claim posture is stated, not merely implied.
        assert!(report.contains("never a full set"));
    }

    #[test]
    fn generation_is_deterministic() {
        let db = CardDatabase::bundled().unwrap();
        assert_eq!(
            compatibility_report(&db).unwrap(),
            compatibility_report(&db).unwrap()
        );
    }

    #[test]
    fn a_name_that_is_both_supported_and_excluded_fails_generation() {
        // The consistency guard: a mechanic listed as excluded must not also name a
        // supported card. Driven with a synthetic catalog + a colliding exclusion.
        let json = r#"[{"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
                        "types":["creature"],"mana_cost":"{G}","colors":["green"],
                        "power":1,"toughness":1}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let exclusions = vec![Exclusion {
            name: "Test Boar".to_string(),
            blocker: "should never both be supported and excluded".to_string(),
        }];
        assert!(matches!(
            report_from(&db, exclusions).unwrap_err(),
            CompatError::SupportedAndExcluded { name } if name == "Test Boar"
        ));
    }

    #[test]
    fn a_duplicated_exclusion_fails_generation() {
        let db = CardDatabase::from_json(
            r#"[{"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
                 "types":["creature"],"mana_cost":"{G}","colors":["green"],
                 "power":1,"toughness":1}]"#,
        )
        .unwrap();
        let twice = |name: &str| Exclusion {
            name: name.to_string(),
            blocker: "a blocker".to_string(),
        };
        assert!(matches!(
            report_from(&db, vec![twice("X"), twice("X")]).unwrap_err(),
            CompatError::DuplicateExclusion { name } if name == "X"
        ));
    }

    #[test]
    fn the_bundled_exclusions_parse_and_carry_only_a_name_and_blocker() {
        // The legal posture is structural: an exclusion with an oracle_text field fails
        // to parse (deny_unknown_fields), so no rules prose can enter this file.
        assert!(!bundled_exclusions().unwrap().is_empty());
        let with_prose = r#"[{"name":"X","blocker":"y","oracle_text":"Draw a card."}]"#;
        assert!(serde_json::from_str::<Vec<Exclusion>>(with_prose).is_err());
    }

    #[test]
    fn the_committed_report_is_fresh() {
        // The freshness gate (runs inside `make check`): the committed report must be
        // byte-identical to a fresh generation. A stale or hand-edited file fails here,
        // so the report can never drift from the catalog — the failure that ended the
        // hand-maintained ledger (#252).
        let db = CardDatabase::bundled().unwrap();
        let generated = compatibility_report(&db).unwrap();
        let committed = committed_report().unwrap_or_else(|e| {
            panic!(
                "cannot read the committed {REPORT_PATH}: {e}. Generate it with: \
                    cargo test -p rune-engine regenerate_compatibility_report -- --ignored"
            )
        });
        assert_eq!(
            committed, generated,
            "{REPORT_PATH} is stale — regenerate it with: \
             cargo test -p rune-engine regenerate_compatibility_report -- --ignored"
        );
    }

    #[test]
    #[ignore = "writes the committed report; run explicitly to regenerate it"]
    fn regenerate_compatibility_report() {
        // The one writer. Run this to update the committed report after changing the
        // catalog or the exclusions: `cargo test -p rune-engine \
        // regenerate_compatibility_report -- --ignored`.
        let db = CardDatabase::bundled().unwrap();
        let report = compatibility_report(&db).unwrap();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../",
            "docs/generated/compatibility.md"
        );
        std::fs::write(path, report).expect("write the committed report");
    }
}
