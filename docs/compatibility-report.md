# Card compatibility report

RUNE supports only a **verified slice** of Magic cards — never a full set. That claim
is backed by a checkable artifact, not prose: a generated report that names every
supported card and every mechanic deliberately left out of scope.

- **The report:** [`docs/generated/compatibility.md`](generated/compatibility.md) — a
  committed, generated file. Do not edit it by hand.
- **The generator:** `rune_engine::compatibility_report`
  (`crates/rune-engine/src/compat.rs`), pure and deterministic — same catalog +
  exclusions in, byte-identical report out (stable ordering, no timestamps).
- **Sources:** the bundled catalog (`crates/rune-engine/data/catalog/`) for the
  supported list, and `crates/rune-engine/data/exclusions.json` for the excluded list.

## Freshness gate

A test regenerates the report and diffs it against the committed file
(`compat::tests::the_committed_report_is_fresh`). It runs inside `make check`, so the
report can never drift from the catalog — the failure mode that ended the old
hand-maintained coverage ledger. If you change the catalog or the exclusions and forget
to regenerate, CI fails with the exact command to run.

## Regenerating

After adding a card or editing the exclusions:

```sh
cargo test -p rune-engine regenerate_compatibility_report -- --ignored
```

Then commit the updated `docs/generated/compatibility.md` alongside your change.

## Adding an exclusion

Edit `crates/rune-engine/data/exclusions.json` and add an object with exactly two
fields, then regenerate:

```json
{ "name": "Planeswalkers", "blocker": "no loyalty or planeswalker-type system" }
```

- `name` — the excluded mechanic or card.
- `blocker` — the concrete reason it is out of scope.

**Names and blockers only.** No Oracle text, flavor text, or branding belongs here; the
schema enforces this structurally (`deny_unknown_fields`), so any extra field fails the
build (`docs/brief.md` Legal Considerations). Generation also fails if a name is listed
as both supported and excluded, or if an exclusion is duplicated.
