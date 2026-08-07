//! Generator for the catalog test-coverage audit (issue #774).
//!
//! Writes `docs/generated/test-coverage.md` from the bundled catalog and the text of the
//! workspace's Rust sources. Run it with `make compat` (which also regenerates the
//! compatibility report), then commit the result. A `#[test]` in `tests/coverage.rs`
//! fails if the committed copy drifts, so this is the only thing that should ever write
//! that file.
//!
//! The report itself is produced by [`sage_engine::coverage::render_report`], a pure
//! function; this binary does the reading and the writing, because the engine crate does
//! neither.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use sage_engine::coverage::{render_report, SCANNED_ROOTS};

/// The report's path relative to the crate root (`crates/sage-engine`). Kept in lockstep
/// with the freshness test in `tests/coverage.rs`.
const REPORT_RELATIVE: &str = "../../docs/generated/test-coverage.md";

/// The repository root, relative to the crate root.
const REPO_ROOT_RELATIVE: &str = "../..";

fn main() -> Result<(), Box<dyn Error>> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root.join(REPO_ROOT_RELATIVE);
    let sources = read_sources(&repo_root)?;
    let db = sage_engine::CardDatabase::bundled()?;
    let report = render_report(&db, &sources);

    let path = crate_root.join(REPORT_RELATIVE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, report)?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Every `.rs` file under [`SCANNED_ROOTS`], as text.
///
/// Duplicated in the freshness test on purpose: both sides read the filesystem for
/// themselves, because putting the walk in the library would be I/O in the engine. The
/// roots they walk are shared, so the two cannot disagree about *what* was scanned.
fn read_sources(repo_root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut sources = Vec::new();
    for root in SCANNED_ROOTS {
        collect(&repo_root.join(root), &mut sources)?;
    }
    Ok(sources)
}

fn collect(dir: &Path, out: &mut Vec<String>) -> Result<(), Box<dyn Error>> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(fs::read_to_string(&path)?);
        }
    }
    Ok(())
}
