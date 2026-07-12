# ADR 0018: Scalable functional card definitions and fallback rules text

- Status: accepted
- Date: 2026-07-12
- Issue: #191

## Context

ADR 0013 split card data into an **oracle card** (printing-independent rules) and
a **printing** (bibliographic record referencing an oracle card), and that split
is implemented and shipped: `crates/rune-engine/data/oracle.json` holds 32
fixture `CardData` entries keyed by an integer `CardId`/`OracleId`, and
`crates/rune-engine/data/sets/{FIX,FIX2}.json` hold `Printing` records that
reference those ids — proven by `adding_a_reprint_changes_no_logic` in
`crates/rune-engine/src/card.rs`. ADR 0013's own words already flagged this as
an M1-scoped model whose file layout was tuned for "a ~100–150-card invented
starter set" (M3), not the catalog scale the project ultimately needs (the
brief's `docs/brief.md` "Shared: Card Data" section, and roadmap M6's "expanded
card pool"). Three things in the shipped M3 model don't scale past that:

- **One growing array, one hand-maintained manifest.** `oracle.json` is a single
  JSON array every new card is appended to, and `SET_MANIFEST` in `card.rs` is a
  hand-written `const` list of `include_str!`ed set files. Both are fine at 32
  entries and two sets; at thousands of cards across dozens of sets, every card
  addition is a diff to the same shared array (concurrent PRs collide on the
  same lines) and every new set requires a hand-edited Rust `const` nobody
  remembers to update.
- **Sequential integer identity, hand-assigned.** `CardId(1)` through
  `CardId(32)` are assigned by whoever writes the next JSON entry. There is no
  stable meaning to the number, no collision detection beyond "does this
  compile," and no way for two agents authoring different cards concurrently to
  avoid stepping on each other's next integer.
- **`oracle_text` conflates authored prose with generated rules text.** `CardData`
  carries an `oracle_text: String` field that is hand-authored, non-infringing
  prose (e.g. `"When Verdant Scout enters the battlefield, draw a card."`) kept
  in sync **by hand** with the accompanying `Ability`/`Effect` IR that actually
  drives behavior, and that string is projected verbatim onto
  `CardView.oracle_text` (`crates/rune-server/src/view.rs`) for display. Nothing
  enforces the two stay consistent, and the field name and `docs/brief.md`'s
  Legal Considerations ("Card oracle text is a grey zone — tolerated by WotC for
  free fan projects") both frame this as bundling real Oracle text — which is
  not what the fixtures actually contain today, and not a risk this project
  wants to carry once real card names/effects are implemented.

Issue #191 sets the target this ADR resolves against: cards implemented as
**structured, independently written functional definitions** that scale to a
large catalog; the engine **executes** those definitions and never parses or
depends on display prose; the server **generates deterministic fallback rules
text** from the functional definition and projects it in `GameView`; every
client remains a dumb renderer; exact Oracle text/image sync is explicitly
**deferred** to a future, client-only, optional-enrichment feature that the
server and engine never fetch, store, or require.

The hard rules constrain the design exactly as they did for ADR 0013
(`AGENTS.md`, `crates/rune-engine/AGENTS.md`, ADR 0002, ADR 0006, ADR 0007):

- **Zero I/O in the engine at runtime.** Card data is embedded at compile time;
  serde is sanctioned only for parsing embedded snapshots (ADR 0006).
- **Rules logic is data on the card, plus a `CardId`-keyed code escape hatch**
  (ADR 0007) — this ADR does not reopen the IR itself, only how definitions
  carrying that IR are authored, identified, laid out, and presented.
- **No card images, no official frames, no WotC branding, no monetization**
  (`docs/brief.md` Legal Considerations) — this ADR tightens the text side of
  that rule to match: no exact Oracle text either, not even as a "grey zone."
- **Zero game logic in the client**; the client never interprets the engine's
  ability/effect IR (`AGENTS.md`).
- **The entire client UI must be reconstructable from one `GameView`.** Any new
  projected field must not require a second round trip.

This ADR is a **decision only** — like ADR 0017, "this ADR's implementation is
out of scope here." It ships no engine/server/protocol/client code; it defines
the model and ends with the PR-sized follow-up issues that implement it.

## Decision

### 1. The oracle-vs-printing split stands; this ADR replaces its scale mechanics

ADR 0013 §1 (oracle card vs printing, rules live only on the oracle card, a
reprint is one printing record and zero logic changes) is **kept unchanged**.
What this ADR replaces is ADR 0013 §2 (file layout) and the identity/text
framing in §1/§6 — see §11 "Migration and governance" for the explicit
supersession.

### 2. Functional schema and versioning

A **functional definition** is the renamed, tightened successor to today's
oracle-card JSON entry: the printing-independent rules object for one card.
Each functional-definition file is a single JSON object (not an array — see
§4) with a required top-level `schema_version` (an unsigned integer, starting
at `1`). The loader rejects a file whose `schema_version` it does not
recognize as a build error, not a silent skip — a version bump is how a future
breaking change to the shape (a renamed field, a restructured `abilities`
encoding) is rolled out catalog-wide with a compiler/build-time forcing
function rather than a runtime surprise.

**Committed fields** (the functional/factual surface — everything the engine
or the fallback formatter reads):

- Identity: `functional_id` (see §3), `schema_version`.
- Factual characteristics: `name`, `mana_cost`, `supertypes`, `types`,
  `subtypes`, `power`/`toughness`, `colors` (derived today from `mana_cost`'s
  pips, but promoted to an explicit field so a colorless-cost-but-colored card
  and color identity are representable without parsing the cost string back —
  the same "structured, never parsed back" discipline `CardData::type_line`
  already uses).
- Behavior: `keywords`, `abilities` (the `Ability`/`Cost`/`TriggerCondition` IR,
  ADR 0007), `spell_effects` (`Effect`/`TargetSpec`), `aura` (`AuraGrant`) —
  all exactly the shapes `crates/rune-engine/src/ability.rs` and `card.rs`
  already define; this ADR does not change the IR, only what surrounds it.
- Escape hatch: an explicit `scripted: bool` flag (default `false`). `true`
  means this card's behavior is (also) defined in `crates/rune-engine/src/scripted.rs`
  under this `functional_id`'s interned `CardId` — see §5 validation.
- Provenance, not prose: `source_revision` (see §10).

**Excluded fields**, structurally rejected via `#[serde(deny_unknown_fields)]`
(the same mechanism `PrintingEntry` already uses to reject `image_uris`/`artist`
in ADR 0013 §6): exact Oracle text, flavor text, image URLs or asset paths,
official symbols/frames/watermarks, artist credit, and any other upstream
presentation asset. **`oracle_text` is removed from the committed field set
entirely** — no field on a functional definition holds authored or exact rules
prose. What a player reads is generated (§7), never stored.

### 3. Stable identity: `FunctionalId` replaces hand-assigned integers as the source of truth

A functional definition's authored, stable identity is a **`FunctionalId`**: a
lowercase `snake_case` string slug (e.g. `"thornback_boar"`), assigned once by
whoever authors the card, matching the definition's file name
(`data/catalog/thornback_boar.json`, see §4), and never reused or renumbered.
`FunctionalId` — not an integer — is what a printing file, a decklist, and (in
a future real-card catalog) an external mapping reference.

The engine's `CardId(u64)` **stays exactly the Rust type it is today** — every
existing `db.card(id)`, `GameState` field, and entity-id scheme keyed on it is
unaffected — but its *provenance* changes: `CardId` becomes an **interned,
build-time-assigned handle**, never hand-written in a data file. The build-time
catalog assembly (§4) collects every `FunctionalId` present in `data/catalog/`,
sorts them by byte value, and assigns `CardId(0)`, `CardId(1)`, … in that
deterministic order. Two authors adding cards in the same PR wave can never
collide on an id, because nobody writes one by hand.

This is a **within-a-build** interning, not a persisted mapping: a running
server process's `CardId`s are stable for the life of that process (matching
`ADR 0002`'s server-authoritative, in-memory `GameState`, which is not
persisted or resumed across a binary rebuild), but a catalog rebuild that adds
or removes a card can shift which integer a given `FunctionalId` interns to.
Nothing today depends on `CardId` being stable across builds — decklists,
printings, and this ADR's protocol projection (§8) all key on `FunctionalId`,
not the interned integer, specifically so that remains true. If a future
feature needs cross-build `CardId` stability (e.g. persisted game state), that
is a new ADR, not an implicit assumption of this one.

An oracle record may additionally carry an external `scryfall_oracle_id` (a
UUID string, optional) exactly as ADR 0013 §6 already allowed — unchanged, and
still just a data field the engine never keys logic on.

**Printing records** (`data/sets/<SET>.json`, ADR 0013 §1, unchanged shape
otherwise) reference a card by `functional_id: String` instead of the current
`oracle_id: u64`. The loader resolves that string to the build's interned
`CardId` the same way it resolves a `FunctionalId` file to its entry; an
unresolvable reference is a build-time validation error (§5), not a runtime
`None`.

### 4. File layout: one functional definition per file, sharded by `FunctionalId`

Replacing the single `oracle.json` array:

- **`crates/rune-engine/data/catalog/<functional_id>.json`** — one file per
  distinct card, holding exactly one functional-definition object (§2). This is
  preferred over the alternatives surveyed:
  - *Monolithic JSON* (today's `oracle.json`): every addition diffs the same
    file; review size and merge-conflict risk both grow with catalog size.
  - *Set-sized files*: better than monolithic, but two authors adding different
    cards to the same expansion still collide on the same file, and a reprint
    (by design, ADR 0013) touches no oracle data at all — so sizing oracle
    files by set conflates two axes that don't share a growth rate.
  - *Rust definitions*: contradicts "normal cards are data, not one Rust source
    file per card" (issue #191's first agreed principle) and ADR 0007's
    data/hatch split.
  - *Generated bundles from an external provider*: rejected as out of scope —
    #191 explicitly excludes "selecting a data provider for end users," and any
    future importer would still need to *emit* this same schema, so it is not a
    competing layout, just a future producer of this one.
  - **One file per `FunctionalId`** gives the smallest possible review/merge
    surface: one new card is one new file, touching zero existing lines. This
    is the "review/concurrency boundary" #191 asks this ADR to establish.
- **`crates/rune-engine/data/sets/<SET>.json`** — unchanged in role (one file
  per set of printing records, ADR 0013 §2), field-updated per §3.
- **Catalog manifest, generated, not hand-maintained.** The hand-written
  `SET_MANIFEST` `const` array in `card.rs` does not scale to hundreds of
  catalog files and dozens of sets. A **build script**
  (`crates/rune-engine/build.rs`) walks `data/catalog/*.json` and
  `data/sets/*.json` at compile time, and:
  1. Parses and validates every file (§5).
  2. Sorts `FunctionalId`s deterministically (byte order) and assigns interned
     `CardId`s (§3).
  3. Emits one generated Rust source under `OUT_DIR` — a `const` array of
     `(CardId, &'static str)` catalog entries and `(&'static str set_code,
     &'static str)` set entries, each `&'static str` pointing at bytes the
     generated file embeds via `include_str!` with paths relative to the crate
     root (so the *content* is still `include_str!`-embedded exactly as today;
     only the *list of what to embed* is generated instead of hand-typed).
  4. `card.rs` pulls it in with a single
     `include!(concat!(env!("OUT_DIR"), "/catalog_manifest.rs"))`.
  - `build.rs` declares `cargo:rerun-if-changed=data` so Cargo's normal
    incremental-build tracking covers it — adding, removing, or editing a
    catalog/set file triggers exactly one manifest regeneration, nothing else.
  - **Build-script I/O is explicitly acceptable here and does not weaken "zero
    I/O in the engine."** That hard rule is about the compiled `rune-engine`
    binary's runtime behavior (`crates/rune-engine/AGENTS.md`: "no dependencies
    on tokio, networking, timers... without an injected seed" — a *runtime*
    services rule). `build.rs` executes once per `cargo build`, on the
    machine building the crate, never in the shipped/running engine; its only
    output is more `&'static str` constants baked into the binary via
    `include_str!`, the exact mechanism ADR 0006 already sanctioned for
    embedding data at compile time. This ADR extends that precedent from "a
    human writes the `include_str!` list" to "a build script writes the
    `include_str!` list," and changes nothing about what the running engine
    does: still zero filesystem access, zero network, zero clock, zero
    randomness, at runtime.
  - `build.rs` may use `std::fs` and `serde_json` (already a build/dev
    dependency path via `[build-dependencies]`, not `[dependencies]`) — this
    does not touch the "empty `[dependencies]` unless an ADR says otherwise"
    rule, which governs the crate's runtime dependency graph, not its build
    tooling.

### 5. Validation

All of the following are **build-time** checks (`build.rs` failing the build
with a descriptive `panic!`/compile error is acceptable there — it is tooling,
not the panicking-APIs-forbidden runtime engine) plus `#[cfg(test)]` coverage
exercising the same validators against fixture data, so a regression is caught
by `cargo test` too, not only by editing `data/`:

- `schema_version` is a recognized value (§2).
- Every `FunctionalId` is unique and matches its file name.
- Every printing's `functional_id` resolves to a catalog entry; no two
  printings in the same set share a `(set_code, collector_number)` (already
  true today per `PrintingKey`, extended to the new reference field).
- Type/P&T invariants: a card with `types` including `Creature` carries both
  `power` and `toughness`; a card without it carries neither — promoted from
  today's implicit trust (nothing currently checks this) to an explicit,
  tested rule.
- Effect/target compatibility: every `Effect`/`AuraGrant` requiring a target
  carries a `TargetSpec` the existing `Effect::target_spec`/`cast_target_specs`
  accessors already expect; an `aura` field is present only on a card whose
  `subtypes` include `"Aura"`.
- Scripted escape-hatch registration is **bidirectional**: a functional
  definition with `scripted: true` must have a matching arm in
  `scripted_abilities` (or a future `scripted_rules_text`, §7) keyed by its
  interned `CardId`, and — the direction nothing enforces today — a
  `scripted.rs` match arm with no corresponding `scripted: true` catalog entry
  is also a build error. The two authoring tiers cannot silently diverge in
  either direction.
- Fallback-formatter completeness (§7) is enforced by the Rust compiler via
  exhaustive `match`es over the IR enums, not by a separate validation pass —
  the strongest available guarantee, since a new `Effect`/`Ability` variant
  without a formatter arm fails `cargo build` everywhere, not just when a card
  using it happens to be validated.

### 6. Performance expectations

Stated as targets the follow-up implementation issue (§12) must measure, not as
already-verified numbers:

- **Checkout.** Thousands of small JSON files is a well-trodden git shape
  (comparable to a monorepo's per-component config files); no special tooling
  is required at the catalog sizes this project targets (roadmap M6's
  "expanded card pool," not a full ~30,000-card Magic catalog).
- **Incremental build.** `build.rs`'s own work is O(files) string
  concatenation plus JSON validation — sub-second for thousands of small files
  — and it only re-runs when `data/` changes (Cargo's dependency tracking via
  `cargo:rerun-if-changed`), so an unrelated engine-code change triggers no
  catalog regeneration at all.
- **Startup parsing.** Unchanged shape from today: one `serde_json::from_str`
  over the generated, concatenated embedded snapshot. Target: `CardDatabase::bundled()`
  parses a 10,000-card catalog in well under 200ms on CI hardware — the
  follow-up issue adds a benchmark/test asserting this once the catalog is
  large enough to matter.

### 7. Fallback presentation: server-generated, deterministic, structurally complete

The formatter lives in **`rune-server`**, not the engine — generating display
prose is presentation, and keeping it out of `rune-engine` is what makes "the
engine never parses or depends on display prose" true by construction rather
than by discipline. A new module (e.g. `crates/rune-server/src/rules_text.rs`)
exposes a pure function `rules_text(&CardData, scripted: Option<&'static str>) -> String`:

- **Deterministic and pure.** Same functional definition in, same string out —
  no randomness, no locale (English only; localization is out of scope and
  would be a future ADR).
- **Exhaustive, compiler-enforced coverage.** The formatter `match`es every
  `Ability`, `Effect`, `TargetSpec`, `TriggerCondition`, `Cost`, `Keyword`, and
  `AuraGrant` variant with no wildcard `_ =>` arm. This directly satisfies
  #191's requirement that an unformattable construct "must fail validation or
  surface an explicit diagnostic rather than silently omit rules" — a new IR
  variant with no formatter arm is a `cargo build` failure across the whole
  workspace, the strongest diagnostic available, and stronger than a runtime
  check that only fires when a card using the new variant is actually loaded.
- **Composes clauses, not sentences copied from anywhere.** e.g. an activated
  mana ability composes its `Cost` clause and `Effect` clause
  (`"{T}: Add {G}."`); a triggered ability composes its `TriggerCondition` and
  effects (`"When this enters the battlefield, draw a card."`); `keywords`
  join as a comma list (`"Flying, trample"`). Output must be **semantically
  complete enough to play** (a player can act on it) but need not, and must
  not, reproduce official Oracle wording.
- **The scripted escape hatch gets a parallel seam.** Since scripted behavior
  is opaque Rust, it cannot be derived automatically; a scripted card supplies
  a hand-authored fallback string via a new `scripted_rules_text(CardId) ->
  Option<&'static str>` function alongside `scripted_abilities`, and §5's
  bidirectional validation requires one exactly when `scripted: true` is set.

### 8. Protocol projection

This ADR **decides** the shape; implementing it in `rune-protocol`/`view.rs`/
`docs/protocol.md`/the web client is the follow-up work in §12 (out of scope
for this ADR's own diff, per issue #191's boundaries).

- `CardView.oracle_text` is **renamed to `CardView.rules_text`**, carrying the
  server-generated string from §7. The old name is inaccurate today (the field
  never held real Oracle text) and would become actively misleading once real
  card names/effects are implemented; the protocol already tolerates a
  reshaped field pre-1.0 (unknown fields are ignored, ADR-documented
  forward-compat) and this is exactly the kind of shape change that convention
  exists for.
- `CardView` gains a **stable presentation lookup identity**:
  `functional_id: string`, projecting §3's `FunctionalId` verbatim. This is the
  provenance-neutral join key a future client-local cache (§9) would use to
  look up exact text/art by stable identity, without the server or protocol
  needing to know that cache exists.
- **Internal IR never crosses the boundary.** `Ability`/`Effect`/`TargetSpec`
  and the rest of `rune-engine`'s ability vocabulary stay engine-internal;
  `rules_text` and the existing display fields (`type_line`, `mana_cost`,
  `power`/`toughness`, `keywords`) remain the only derived-for-display surface,
  preserving "the web client never interprets the engine's internal
  ability/effect IR to determine or describe semantics" (#191).
- **The one-`GameView` reconstruction invariant is preserved.** Both new
  fields ride the existing `CardView`, already included wherever a card is
  shown; no second round trip, no client-held state needed to resolve them.

### 9. Deferred: client-local enrichment

Recorded here, not designed here, per #191's explicit scope boundary:

- A future, **client-only** feature may let a client resolve `functional_id`
  (§8) against a locally cached, separately sourced mapping to exact Oracle
  text and/or official art for display enrichment.
- That cache is **optional presentation enrichment, never authoritative game
  state** — a client with no cache renders `rules_text` and no art, and plays
  correctly; a client with a stale or absent cache entry falls back to
  `rules_text` for that card.
- **The server and engine never fetch, store, or require that cache.** Nothing
  in this ADR's model changes if that feature is never built.
- **The current no-images/no-official-frames hard rule is not weakened.**
  Accepting this ADR does not pre-approve that future feature's legal posture;
  it would need its own explicit decision when proposed.

### 10. Source revision / non-expressive hash mechanism

A functional definition may carry `source_revision`, an optional object:

```json
{ "checked_against": "scryfall-bulk-2026-07-01", "oracle_hash": "sha256:…" }
```

- `checked_against` names the external source/snapshot a maintainer compared
  this functional definition's *behavior* against (free-form string; no
  fetching, parsing, or storing of that source is implied or required).
- `oracle_hash` is a hash **of** the real Oracle text as it read at that check
  — never the text itself. A maintainer who wants to detect "did this card's
  real wording change since we last verified our IR matches it" recomputes the
  hash from a fresh read (Scryfall, a rules update, whatever) and compares;
  a mismatch is a signal to re-review, nothing more. No prose is ever
  persisted in the repository at any point in this mechanism.
- Both fields are absent for invented, non-real-card fixtures (the current 32
  fixtures stay exactly as invented after migration, §11) — `source_revision`
  only applies once real card identities are implemented.

### 11. Migration and governance

- **`data/oracle.json` → `data/catalog/<functional_id>.json`.** The 32 existing
  fixture entries are split one-per-file, each gaining `schema_version: 1` and
  a `functional_id` derived from its current name (e.g. `"Thornback Boar"` →
  `thornback_boar`), and losing its `oracle_text` field (the accompanying
  fixture behavior — vanilla creature, mana ability, ETB trigger, keywords,
  auras, etc. — is unchanged; only the authored-prose field is dropped in
  favor of §7's generated text).
- **`SET_MANIFEST` (hand-written) → `build.rs`-generated manifest.** `FIX.json`
  and `FIX2.json` keep their role and shape apart from `oracle_id` →
  `functional_id` (§3).
- **`CardId` stays the same Rust type**, reinterpreted as build-interned rather
  than hand-assigned (§3) — no downstream code that pattern-matches or stores
  `CardId` needs to change.
- **ADR 0013 is partially superseded, not rewritten.** Its §1 core claim
  (oracle vs. printing, rules live once) stands. This ADR supersedes ADR 0013
  §2 (file layout: monolithic `oracle.json` + hand-written `SET_MANIFEST`) and
  the identity/text framing in §1/§6 (sequential-integer-as-source-of-truth,
  and `oracle_text` as a bundled Scryfall-shaped field). ADR 0013's own text is
  left as the historical record of the M3 decision; its status line is updated
  to point here (see the accompanying edit to that file in this PR).
- **`docs/brief.md` is a preserved vision document** (its own header: "Where it
  conflicts with shipped reality, the source of truth wins... Sections that
  have since diverged carry an inline **Shipped:** note"). This ADR adds
  `Shipped:` notes to two sections rather than rewriting them: the Legal
  Considerations bullet asserting Oracle text is "a grey zone — tolerated," and
  the Shared: Card Data section's `oracle_text` field / "Bundled JSON snapshot"
  framing — both now point at this ADR's decision that RUNE bundles **no**
  exact Oracle text, only structured functional data and generated fallback
  text, with exact-text sync deferred to the optional client-only feature in
  §9. (See the accompanying edit to that file in this PR.)

## Consequences

- **Easier.** Concurrent card authoring no longer collides: one new card is one
  new file, diffing zero existing lines, and no author hand-assigns an
  identity that could collide with a concurrent PR. A card's presented rules
  text can never silently drift from its actual behavior, because it is
  derived from that behavior instead of hand-kept in sync with it, and a
  future IR addition that the formatter can't yet render fails the build
  everywhere rather than shipping a card with missing rules text. Legal
  exposure is reduced: no field anywhere in the functional-definition schema
  can hold exact Oracle text (structurally rejected, not just discouraged),
  closing the gap between what `docs/brief.md`'s original vision described and
  what issue #191's agreed principles now require. A future client-local
  cache has a clean, stable, provenance-neutral join key (`functional_id`)
  without the server or protocol needing to know that cache exists.
- **Harder / given up.** The catalog becomes thousands of small files instead
  of a few large ones, which trades "grep one file to see everything" for
  "grep a directory" — mitigated by the build script's generated manifest and,
  as future tooling, a dev-time catalog report (not built here). `build.rs`
  adds a new moving part to the engine crate's build (mitigated: it is
  compile-time-only tooling, extending the ADR 0006 precedent rather than
  breaking it, and it never touches the shipped binary's runtime behavior).
  `FunctionalId` slugs need a naming convention maintainers actually follow;
  collisions are caught at build time, not prevented at author time.
- **Deferred.** All of this ADR's own implementation (§12); real Scryfall/Oracle
  data ingestion and provider selection; the client-local enrichment cache
  (§9) and any weakening of the no-images rule that would require; fallback
  text localization; `CardId` stability across process restarts (revisit only
  if persisted game state is ever proposed).

## Follow-up issues

PR-sized, in dependency order — each closes against this ADR:

| # | Feature | Area | Depends on |
|---|---|---|---|
| A | `FunctionalId`, `schema_version`, and the tightened functional-definition schema (deny-unknown-fields, `oracle_text` removed) on the existing single-file loader | engine | — |
| B | `build.rs`: walk `data/catalog/` + `data/sets/`, validate (§5), intern `CardId`s, generate the embed manifest | engine | A |
| C | Migrate the 32 fixtures into `data/catalog/<functional_id>.json`; migrate `FIX`/`FIX2` printings to `functional_id` references; delete `SET_MANIFEST` | engine | B |
| D | `rules_text` formatter in `rune-server` with exhaustive-match coverage of the IR, plus `scripted_rules_text` seam and its bidirectional validation | server | C |
| E | Protocol: `CardView.oracle_text` → `rules_text`, add `functional_id`; update `docs/protocol.md`, `rune-protocol`, `view.rs`, and the web client's field usage | protocol, server, client | D |
| F | `source_revision` field (schema-only; no ingestion) plus authoring docs for maintainers | engine, docs | A |
| G | Startup-parse benchmark/test at catalog scale (§6 target) | engine | C |

Follow-up issue F is independent of C/D/E and may land in parallel with them.
