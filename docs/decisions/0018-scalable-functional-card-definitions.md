# ADR 0018: Scalable functional card definitions and generated rules text

- Status: accepted
- Date: 2026-07-12
- Issue: #191

## Context

The first card catalog used one growing array, hand-assigned integer ids, and authored display
prose beside executable behavior. That structure created merge conflicts, made identities
unstable across authors, and allowed displayed text to drift from the rules the engine ran.

The catalog also needs to enforce the project’s prohibition on exact Oracle text and official
presentation assets by schema rather than review convention.

## Decision

### Functional definitions

Each printing-independent card is one versioned JSON object containing only structured facts
and behavior:

- `schema_version` and stable `functional_id`;
- name, types, subtypes, mana cost, colors, and creature power/toughness;
- keywords, abilities, spell effects, and Aura data; and
- an explicit `scripted` flag for the code escape hatch from ADR 0007.

`CardData` uses `deny_unknown_fields`. Exact Oracle text, flavor text, images, official frames
or symbols, artist data, and other presentation assets are not representable in the schema.
Breaking schema changes increment the version and migrate the whole catalog.

### Identity

`FunctionalId` is the authored identity: a lowercase `snake_case` slug that matches the file
name and remains stable across builds. Printings, decklists, fixtures, and scripted behavior use
it.

`CardId` remains a compact engine handle. The build script sorts functional ids and interns
`CardId(0..n)` for that build. It is never hand-authored or persisted because adding a card may
renumber later handles.

Printing identity remains set code plus collector number. Per-game `CardInstanceId` and
`PermanentId` values remain separate from both catalog layers.

### File layout and embedding

- `data/catalog/<functional_id>.json` contains one functional definition.
- `data/sets/<SET>.json` contains printing records that refer to functional ids.

`crates/rune-engine/build.rs` discovers both directories, validates them, interns handles, and
generates an `include_str!` manifest under `OUT_DIR`. Build-time filesystem access is allowed;
the compiled engine still performs no runtime I/O.

Adding a functional card creates one independent file. A reprint edits only its set file and
changes no behavior.

### Validation

Shared validators in `src/catalog.rs` run during catalog assembly and loading. They reject:

- unsupported schema versions;
- malformed, duplicate, or file-mismatched functional ids;
- invalid creature power/toughness;
- Aura data on non-Auras;
- unresolved printing references and duplicate collector numbers; and
- disagreement between the `scripted` declaration and `src/scripted.rs`.

Serde and the Rust type system reject unknown fields and malformed IR nodes. Formatter matches
are exhaustive, so an unsupported behavior variant fails compilation rather than silently
losing display text.

### Generated rules text

Functional definitions contain no rules-prose field. `rune-server` generates deterministic,
English display text from the structured IR. The engine never parses or depends on that text.

Scripted behavior cannot be inspected by the formatter, so a scripted card supplies concise,
non-Oracle explanatory text beside its Rust implementation. Loader validation keeps that text
and the scripted registration paired.

### Protocol projection

`CardView.rules_text` carries generated text. `CardView.functional_id` carries stable catalog
identity separately from the per-game `CardView.id`. Internal ability and effect IR never
crosses the protocol boundary.

A client can render every card from `GameView` without a separate catalog. Any future optional
presentation enrichment must remain client-local, keyed by stable identity, and cannot become
required game state or weaken the project’s asset restrictions without a new decision.

## Consequences

Card authors work in independent files without assigning shared integers. Executable behavior
and player-facing rules text cannot silently diverge, and prohibited presentation data fails
schema parsing. The build script and many small files add tooling complexity, while stable
external persistence must use `FunctionalId` rather than engine handles.
