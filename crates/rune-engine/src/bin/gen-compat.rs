//! Generator for the card-compatibility report (issue #258).
//!
//! Writes `docs/generated/compatibility.md` from the bundled catalog and the curated
//! exclusion list. Run it with `make compat` (or `cargo run -p rune-engine --bin
//! gen-compat`) after adding a card or editing `data/exclusions.json`, then commit the
//! regenerated file. A `#[test]` in `tests/compat.rs` fails if the committed copy ever
//! drifts, so this is the *only* thing that should ever write that file.
//!
//! The report text itself is produced by [`rune_engine::compat::bundled_report`], a
//! pure function; this binary only locates the output path and writes the bytes.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

/// The report's path relative to the crate root (`crates/rune-engine`). Kept in lockstep
/// with the freshness test in `tests/compat.rs`.
const REPORT_RELATIVE: &str = "../../docs/generated/compatibility.md";

fn main() -> Result<(), Box<dyn Error>> {
    let report = rune_engine::compat::bundled_report()?;

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REPORT_RELATIVE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, report)?;
    println!("wrote {}", path.display());
    Ok(())
}
