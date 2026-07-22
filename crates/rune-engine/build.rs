//! Compile-time catalog assembly (ADR 0018 §4).
//!
//! Walks `data/catalog/` and `data/sets/`, validates every file, interns a [`CardId`]
//! for each functional definition, and writes `$OUT_DIR/catalog_manifest.rs` — the
//! `include_str!` list that `src/card.rs` pulls in with a single `include!`. Adding a
//! card is therefore adding one file: no existing line, in data or in Rust, changes.
//!
//! **This does not weaken "zero I/O in the engine."** That rule governs the *running*
//! engine (`crates/rune-engine/AGENTS.md`), and this script is not part of it: it runs
//! once per `cargo build`, on the machine doing the building, and its only output is
//! more `&'static str` constants baked into the binary by `include_str!` — the exact
//! mechanism ADR 0006 already sanctioned for embedding card data. What ships still does
//! zero filesystem, network, clock, and randomness work at runtime. All this script
//! changes is *who writes the `include_str!` list*: a build script instead of a human
//! maintaining a `const` by hand.
//!
//! The validators live in `src/catalog.rs` and are pulled in below rather than
//! reimplemented here, so the rules enforced at build time are the same code the loader
//! and the tests run (ADR 0018 §5).

// `src/catalog.rs` is compiled into both this build script and the engine itself. Its
// items are `pub` for the engine's benefit; inside this bin crate they are reachable
// only from `main`, and the engine exercises the parts this script does not.
#[allow(unreachable_pub, dead_code)]
#[path = "src/catalog.rs"]
mod catalog;

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use catalog::{check_printings, validate_definition};

/// One validated functional definition, ready to be interned.
struct Definition {
    /// Its authored identity — also its file name (ADR 0018 §4).
    functional_id: String,
    /// The absolute path the generated manifest will `include_str!`.
    path: PathBuf,
}

/// One validated set file.
struct Set {
    /// The set code, taken from the file name (`FIX.json` → `FIX`).
    code: String,
    /// The absolute path the generated manifest will `include_str!`.
    path: PathBuf,
    /// The functional ids its printings reference, each with the collector number that
    /// referenced it — so a dangling reference can name the printing at fault.
    references: Vec<(String, String)>,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Cargo's own incremental tracking: regenerate the manifest when — and only when —
    // a catalog or set file changes. An unrelated engine edit re-runs nothing.
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=src/catalog.rs");

    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let definitions = read_catalog(&root.join("data/catalog"))?;
    let sets = read_sets(&root.join("data/sets"))?;
    check_references(&definitions, &sets)?;

    let out = PathBuf::from(std::env::var("OUT_DIR")?).join("catalog_manifest.rs");
    fs::write(&out, render_manifest(&definitions, &sets)?)?;
    Ok(())
}

/// Every `*.json` in `dir`, sorted by file name — the deterministic order everything
/// downstream depends on.
fn json_files(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|err| format!("cannot read {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    Ok(paths)
}

/// A path's file name without its extension, as a `String`.
fn stem(path: &Path) -> Result<String, Box<dyn Error>> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .ok_or_else(|| format!("{} has no usable file name", path.display()).into())
}

/// Read and validate every functional definition, returning them **interned order**:
/// sorted by `FunctionalId` byte value, which is what assigns `CardId(0..n)` (ADR 0018 §3).
///
/// Sorting here — rather than trusting the filesystem's iteration order — is what makes
/// the interning deterministic: the same catalog produces the same handles on every
/// machine and every rebuild.
fn read_catalog(dir: &Path) -> Result<Vec<Definition>, Box<dyn Error>> {
    let mut definitions = Vec::new();
    let mut seen: HashMap<String, PathBuf> = HashMap::new();

    for path in json_files(dir)? {
        let file_stem = stem(&path)?;
        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?;

        let functional_id = validate_definition(Some(&file_stem), &value)
            .map_err(|violation| format!("{}: {violation}", path.display()))?;

        // A file name *is* an identity, and a directory cannot hold the same name
        // twice — so this can only fire if the two ever drift apart. Cheap to assert,
        // and it keeps the rule true by construction rather than by coincidence.
        if let Some(first) = seen.insert(functional_id.clone(), path.clone()) {
            return Err(format!(
                "two definitions claim the functional id `{functional_id}`: {} and {}",
                first.display(),
                path.display()
            )
            .into());
        }
        definitions.push(Definition {
            functional_id,
            path,
        });
    }

    if definitions.is_empty() {
        return Err(format!("no functional definitions found in {}", dir.display()).into());
    }
    definitions.sort_by(|a, b| a.functional_id.cmp(&b.functional_id));
    Ok(definitions)
}

/// Read and validate every set file, returning them sorted by set code.
fn read_sets(dir: &Path) -> Result<Vec<Set>, Box<dyn Error>> {
    let mut sets = Vec::new();

    for path in json_files(dir)? {
        let code = stem(&path)?;
        let text = fs::read_to_string(&path)?;
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?;

        let mut references = Vec::new();
        for entry in &entries {
            let functional_id = entry
                .get("functional_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{}: a printing has no `functional_id`", path.display()))?;
            let collector_number = entry
                .get("collector_number")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    format!("{}: a printing has no `collector_number`", path.display())
                })?;
            references.push((functional_id.to_string(), collector_number.to_string()));
        }

        check_printings(&code, references.iter().map(|(_, number)| number.as_str()))
            .map_err(|violation| format!("{}: {violation}", path.display()))?;

        sets.push(Set {
            code,
            path,
            references,
        });
    }

    sets.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(sets)
}

/// Every printing must reference a definition the catalog actually holds (ADR 0018 §5).
///
/// Resolved here, at build time, so a dangling reference is a compile error rather than
/// a `None` the engine trips over mid-game.
fn check_references(definitions: &[Definition], sets: &[Set]) -> Result<(), Box<dyn Error>> {
    let known: HashMap<&str, ()> = definitions
        .iter()
        .map(|def| (def.functional_id.as_str(), ()))
        .collect();

    for set in sets {
        for (functional_id, collector_number) in &set.references {
            if !known.contains_key(functional_id.as_str()) {
                return Err(format!(
                    "{} #{collector_number} references `{functional_id}`, \
                     which is not in data/catalog/",
                    set.code
                )
                .into());
            }
        }
    }
    Ok(())
}

/// Render the generated manifest: the two `const` arrays `src/card.rs` includes.
///
/// The definitions arrive already sorted, so a definition's position in `CATALOG` *is*
/// its interned `CardId` — the handle is the index, assigned by this script and never
/// written by hand (ADR 0018 §3).
fn render_manifest(definitions: &[Definition], sets: &[Set]) -> Result<String, Box<dyn Error>> {
    let mut out = String::new();
    out.push_str(
        "// @generated by build.rs from data/catalog/ and data/sets/ — do not edit.\n\
         // Each entry's `id` is its index in this array: `FunctionalId`s sorted by byte\n\
         // value, interned to CardId(0..n) (ADR 0018 §3).\n\n",
    );

    writeln!(out, "pub(super) const CATALOG: &[CatalogEntry] = &[")?;
    for (index, definition) in definitions.iter().enumerate() {
        let path = definition
            .path
            .to_str()
            .ok_or("catalog path is not UTF-8")?;
        writeln!(
            out,
            "    CatalogEntry {{ id: CardId({index}), functional_id: {:?}, json: include_str!({path:?}) }},",
            definition.functional_id,
        )?;
    }
    writeln!(out, "];\n")?;

    writeln!(out, "pub(super) const SET_MANIFEST: &[SetSnapshot] = &[")?;
    for set in sets {
        let path = set.path.to_str().ok_or("set path is not UTF-8")?;
        writeln!(
            out,
            "    SetSnapshot {{ code: {:?}, json: include_str!({path:?}) }},",
            set.code,
        )?;
    }
    writeln!(out, "];")?;

    Ok(out)
}
