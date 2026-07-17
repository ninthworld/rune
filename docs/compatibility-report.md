# Card compatibility report

RUNE claims support for **only the verified slice of cards in its catalog, never a full
set**. [`docs/generated/compatibility.md`](generated/compatibility.md) is the checkable
artifact behind that claim (issue #258): a deterministic report naming every supported
card and every mechanic deliberately left out of scope.

## What it contains

- **Supported** — every functional definition in `crates/rune-engine/data/catalog/`,
  with its `functional_id`, name, and whether it is a plain data definition or uses the
  `scripted` code escape hatch (ADR 0018 §2). Listed in interned order
  (`FunctionalId`s sorted by byte value), so the ordering is identical on every machine.
- **Excluded** — a curated list of cards and mechanics that were considered and are out
  of scope, each with the single blocker that keeps it out.

Both sections carry **names and blockers only** — never Oracle text, flavor text, or
official branding (the schema's legal posture, ADR 0018, extends to this report and to
the exclusions data).

## How it stays honest

The report is **generated, never hand-edited**. `crates/rune-engine/src/compat.rs`
renders it as a pure function of the catalog + the exclusion list; the running engine
does no I/O (the exclusion list is baked in with `include_str!`, the ADR 0006 pattern).

A `cargo test` freshness gate (`crates/rune-engine/tests/compat.rs`) regenerates the
report in memory and fails if the committed copy has drifted. Because it runs under
`cargo test --workspace`, it is part of `make check` and CI — the committed report can
never go stale the way the old hand-maintained coverage ledger did (#252). Generating
twice produces byte-identical output (no timestamps), and a name that appears as both
supported and excluded is a hard error, not a self-contradicting report.

## Regenerate it

After adding or removing a catalog card, or editing the exclusion list:

```sh
make compat        # == cargo run -q -p rune-engine --bin gen-compat
```

Then commit the regenerated `docs/generated/compatibility.md`. If you forget, `make
check` fails with a pointer back to `make compat`.

## Add an exclusion

Edit [`crates/rune-engine/data/exclusions.json`](../crates/rune-engine/data/exclusions.json)
— an array of `{ "name": ..., "blocker": ... }` objects:

```json
{
  "name": "Planeswalkers",
  "blocker": "no loyalty counter system or loyalty abilities"
}
```

Rules:

- **Names and blockers only.** The schema uses `deny_unknown_fields`, so adding any
  other key (e.g. rules text) is a parse error — this is what keeps Oracle text out.
- A `name` must not also be a supported catalog card, or generation fails.
- Keep the blocker a short, factual reason (a missing system or mechanic), not prose
  copied from any card.

Then run `make compat` and commit both files.
