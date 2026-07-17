//! Freshness gate for the card-compatibility report (issue #258).
//!
//! The hand-maintained coverage ledger died because nothing forced it to stay current
//! (#252). This test is the forcing function for its generated replacement: it renders
//! the report from the live catalog + exclusion list and asserts the committed
//! `docs/generated/compatibility.md` matches byte-for-byte. A new card, an edited
//! exclusion, or a hand-edit of the committed file all fail here — the fix is always
//! `make compat` + commit. Because it runs under `cargo test --workspace`, the gate is
//! part of `make check` and CI with no extra wiring.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

/// The committed report, relative to the crate root — must match the generator's path.
const REPORT_RELATIVE: &str = "../../docs/generated/compatibility.md";

#[test]
fn committed_report_is_fresh() {
    let expected = rune_engine::compat::bundled_report().expect("render the report");

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REPORT_RELATIVE);
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read {} ({err}). Generate it with `make compat`.",
            path.display()
        )
    });

    assert_eq!(
        committed, expected,
        "\n\ndocs/generated/compatibility.md is stale — regenerate it with `make compat` \
         and commit the result.\n"
    );
}
