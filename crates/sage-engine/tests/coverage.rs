//! Freshness gate for the catalog test-coverage audit (issue #774).
//!
//! The same forcing function `tests/compat.rs` applies to the compatibility report: the
//! committed `docs/generated/test-coverage.md` must be what the live catalog and the
//! live sources render. A new card, a new test, or a hand-edit of the committed file all
//! fail here, and the fix is always `make compat` + commit.
//!
//! **What is gated is drift, never content.** A card appearing in the report's unnamed
//! list does not fail anything — that list is a question for a reviewer, and turning it
//! into a merge blocker would buy a test named after every card rather than a test that
//! drives one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use sage_engine::coverage::{render_report, SCANNED_ROOTS};

/// The committed report, relative to the crate root — must match the generator's path.
const REPORT_RELATIVE: &str = "../../docs/generated/test-coverage.md";

/// The repository root, relative to the crate root.
const REPO_ROOT_RELATIVE: &str = "../..";

#[test]
fn committed_coverage_report_is_fresh() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut sources = Vec::new();
    for root in SCANNED_ROOTS {
        collect(
            &crate_root.join(REPO_ROOT_RELATIVE).join(root),
            &mut sources,
        );
    }
    let db = sage_engine::CardDatabase::bundled().expect("the bundled catalog");
    let expected = render_report(&db, &sources);

    let path = crate_root.join(REPORT_RELATIVE);
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read {} ({err}). Generate it with `make compat`.",
            path.display()
        )
    });

    assert_eq!(
        committed, expected,
        "\n\ndocs/generated/test-coverage.md is stale — regenerate it with `make compat` \
         and commit the result. Adding a test that names a card changes this file, which \
         is the point.\n"
    );
}

/// Every `.rs` file under `dir`, as text. The generator's twin — see its note on why the
/// walk is not shared.
fn collect(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push(text);
            }
        }
    }
}
