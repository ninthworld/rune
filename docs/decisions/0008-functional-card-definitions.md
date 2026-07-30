# ADR 0008: Functional card definitions and stable `FunctionalId`

- Status: accepted
- Date: 2026-07-30

## Context

Card data splits into a **functional definition** — the printing-independent rules for one
card — and a **printing**, a bibliographic record referencing one. Rules live exactly once, so
adding a reprint changes no logic.

The question this decides is how that model scales, and it has three parts that each break
independently at catalog size:

- **A single growing array plus a hand-maintained manifest.** One JSON array every new card is
  appended to, and a hand-written `const` list of embedded set files, are fine at a few dozen
  cards and two sets. At thousands of cards across dozens of sets, every addition diffs the
  same shared lines — concurrent work collides — and every new set needs a Rust `const` nobody
  remembers to update.
- **Hand-assigned sequential integers as identity.** If `CardId(1)`…`CardId(32)` are written
  by whoever adds the next entry, there is no stable meaning to the number, no collision
  detection beyond "does it compile," and no way for two authors to avoid stepping on each
  other's next integer.
- **A stored rules-prose field conflates authored text with generated text.** A hand-authored
  `oracle_text` kept in sync *by hand* with the ability IR that actually drives behavior has
  nothing enforcing the two agree, and it frames the project as bundling real Oracle text —
  which the legal posture forbids (`docs/brief.md`, Legal constraints).

The target: cards are structured, independently written functional definitions that scale; the
engine **executes** them and never parses display prose; the server **generates** rules text
from the same data; clients stay dumb renderers.

The architecture constrains the design (ADR 0001, ADR 0002, ADR 0003): zero I/O in the running
engine, card data embedded at compile time, rules logic as data plus a `CardId`-keyed code
escape hatch, zero game logic in the client, and the entire UI reconstructable from one view.

## Decision

### 1. Functional schema and versioning

A **functional definition** is the printing-independent rules object for one card: a single
JSON object with a required top-level `schema_version` (unsigned integer, starting at `1`).
The loader rejects an unrecognized `schema_version` as a build error, not a silent skip — a
version bump is how a breaking shape change is rolled out catalog-wide with a build-time
forcing function rather than a runtime surprise.

**Committed fields** — everything the engine or the text formatter reads:

- Identity: `functional_id` (§2), `schema_version`.
- Factual characteristics: `name`, `mana_cost`, `supertypes`, `types`, `subtypes`,
  `power`/`toughness`, `colors`. Colors are an explicit field rather than parsed back out of
  the mana cost, so a colorless-cost-but-colored card and color identity are both
  representable — the same structured-never-parsed-back discipline as `type_line`.
- Behavior: `keywords`, `abilities` (the `Ability`/`Cost`/`TriggerCondition` IR of ADR 0003),
  `spell_effects` (`Effect`/`TargetSpec`), `aura` (`AuraGrant`).
- Escape hatch: an explicit `scripted: bool`, default `false`. `true` means behavior is also
  defined in `scripted.rs` under this definition's interned `CardId` (§4).
- Provenance, not prose: `source_revision`.

**Excluded fields**, structurally rejected via `#[serde(deny_unknown_fields)]` — the same
mechanism printing records use to reject `image_uris` and `artist`: exact Oracle text, flavor
text, image URLs or asset paths, official symbols, frames, watermarks, artist credit, and any
other upstream presentation asset. **No field holds rules prose.** What a player reads is
generated (§6), never stored.

### 2. Stable identity: `FunctionalId`

A definition's authored identity is a **`FunctionalId`**: a lowercase `snake_case` slug
(`"llanowar_elves"`), assigned once by the author, matching the file name
(`data/catalog/llanowar_elves.json`), never reused or renumbered. A `FunctionalId` — not an
integer — is what a printing file, a decklist, and any external mapping reference.

`CardId(u64)` remains the Rust type every rules read goes through, but it is an **interned,
build-time-assigned handle**, never hand-written. The build collects every `FunctionalId` in
`data/catalog/`, sorts by byte value, and assigns `CardId(0)`, `CardId(1)`, … in that
deterministic order. Two authors adding cards concurrently can never collide on an id, because
nobody writes one.

This is a **within-a-build** interning, not a persisted mapping. A running process's `CardId`s
are stable for its lifetime, but a rebuild that adds or removes a card can shift which integer
a `FunctionalId` interns to. Nothing depends on cross-build stability — decklists, printings,
and the protocol projection all key on `FunctionalId` precisely so that stays true. A feature
needing cross-build `CardId` stability, such as persisted game state, is a new decision, not
an implicit assumption of this one.

A definition may carry an optional external `scryfall_oracle_id` (a UUID string) — a data
field the engine never keys logic on.

**Printing records** (`data/sets/<SET>.json`) reference a card by `functional_id: String`. The
loader resolves it to the build's interned `CardId`; an unresolvable reference is a build-time
validation error, not a runtime `None`.

**The four identities.** Only two of them are authored by a human:

| Layer | Type | Assigned by | Stable for |
|---|---|---|---|
| Functional | `FunctionalId` | the card's author, by hand | forever — never reused or renumbered |
| Interned handle | `CardId(u64)` | the build, in sorted order | one build |
| Printing | `PrintingKey { set_code, collector_number }` | the set file listing it, by hand | forever |
| Per-game instance | `CardInstanceId(u64)`, `PermanentId(u64)` | `GameState::mint_id`, at runtime | one game; a `PermanentId`, one battlefield stay |

- **Functional → handle** is one-to-one within a build, and is the only mapping the build
  invents. `CardId` is a *handle to* the authored identity, not the identity itself.
- **Functional → printing** is one-to-many: a printing names a `functional_id` and carries no
  rules, so every reprint resolves to the same definition. `PrintingKey` is not an engine id
  and never enters `GameState`.
- **Handle → instance** is one-to-many within a game. Two Forests in hand share one `CardId`
  and hold distinct `CardInstanceId`s. `PermanentId` is narrower still — reborn on each
  battlefield entry, which is how the engine gets zone-change identity without zone-change
  counters.
- **Instance → wire** is the protocol's `EntityId` string, minted per game from an instance,
  permanent, or seat (`card_5`, `perm_7`, `p0`). It is opaque and carries no catalog identity:
  `CardView.functional_id` is a separate field precisely so a presentation lookup and a
  game-object reference cannot be mistaken for each other.

**Game state holds no printing identity.** `GameState` stores `CardId`, so the engine cannot
tell which printing a copy came from — deliberate, since reprints are rules-identical and no
rule may depend on the answer. This bounds §8: a client-local cache can key on
`functional_id`, but not on a printing, because the server does not know the printing either.
Per-printing art would require deck construction to carry the printing in as presentation-only
data, which is a separate decision.

### 3. File layout: one definition per file

- **`crates/sage-engine/data/catalog/<functional_id>.json`** — one file per card, holding
  exactly one definition object. One new card is one new file touching zero existing lines,
  which is the smallest possible review and merge surface. The alternatives were surveyed and
  rejected: a monolithic array makes every addition diff the same file; set-sized files still
  collide when two authors add to the same expansion, and conflate two axes that don't share a
  growth rate, since a reprint touches no functional data at all; one Rust source file per
  card contradicts cards-are-data and the ADR 0003 split.
- **`crates/sage-engine/data/sets/<SET>.json`** — one file per set of printing records.
- **A generated manifest, never hand-maintained.** `crates/sage-engine/build.rs` walks
  `data/catalog/*.json` and `data/sets/*.json` at compile time; parses and validates every
  file (§4); sorts `FunctionalId`s and assigns interned `CardId`s (§2); and emits one
  generated Rust source under `OUT_DIR` — a `const` array of catalog and set entries whose
  `&'static str` contents are embedded with `include_str!`. `card.rs` pulls it in with a
  single `include!`. The build declares `cargo:rerun-if-changed=data`, so editing a catalog
  file triggers exactly one manifest regeneration and nothing else.

**Build-script I/O does not weaken "zero I/O in the engine."** That rule governs the compiled
engine's *runtime* behavior. `build.rs` runs once per build, on the building machine, never in
the shipped engine, and its only output is more `&'static str` constants baked in via
`include_str!` — the exact mechanism ADR 0002 sanctioned. This extends that precedent from "a
human writes the `include_str!` list" to "a build script writes it," and changes nothing about
what the running engine does: still zero filesystem, network, clock, or randomness at runtime.
`build.rs` may use `std::fs` and `serde_json` through `[build-dependencies]`, which is not the
crate's runtime dependency graph.

### 4. Validation

All of the following are **build-time** checks — failing the build with a descriptive error is
acceptable there, since it is tooling, not the panic-free runtime engine — plus test coverage
exercising the same validators against fixtures, so a regression is caught by `cargo test` and
not only by editing `data/`:

- `schema_version` is a recognized value.
- Every `FunctionalId` is unique and matches its file name.
- Every printing's `functional_id` resolves to a catalog entry, and no two printings in a set
  share a `(set_code, collector_number)`.
- Type and P/T invariants: a card whose `types` include `Creature` carries both `power` and
  `toughness`; a card without it carries neither.
- Effect and target compatibility: every `Effect`/`AuraGrant` requiring a target carries a
  `TargetSpec`, and an `aura` field appears only on a card whose `subtypes` include `Aura`.
- Escape-hatch registration is **bidirectional**: a definition with `scripted: true` must have
  a matching arm keyed by its interned `CardId`, *and* a `scripted.rs` arm with no
  corresponding `scripted: true` entry is equally a build error. The two authoring tiers
  cannot silently diverge in either direction.
- Formatter completeness (§6) is enforced by the compiler through exhaustive matches over the
  IR enums rather than a separate validation pass — a new variant without a formatter arm
  fails the build everywhere, not only when a card using it is loaded.

### 5. Performance shape

- **Checkout.** Thousands of small JSON files is a well-trodden git shape; no special tooling
  is needed at the catalog sizes this project targets — an expanded card pool, not the full
  ~30,000-card Magic corpus.
- **Incremental build.** The build script's own work is O(files) string concatenation plus
  JSON validation, and it re-runs only when `data/` changes, so an unrelated engine change
  triggers no catalog regeneration.
- **Startup parsing.** One `serde_json::from_str` over the generated, concatenated embedded
  snapshot.

### 6. Rules text is server-generated

The formatter lives in **`sage-server`** (`rules_text.rs`), not the engine. Generating display
prose is presentation, and keeping it out of the engine is what makes "the engine never parses
or depends on display prose" true by construction rather than by discipline.

- **Deterministic and pure.** The same definition in, the same string out — no randomness, no
  locale. English only; localization would be its own decision.
- **Exhaustive, compiler-enforced coverage.** The formatter matches every `Ability`, `Effect`,
  `TargetSpec`, `TriggerCondition`, `Cost`, `Keyword`, and `AuraGrant` variant with no
  wildcard arm. A new IR variant with no formatter arm is a build failure across the whole
  workspace — a stronger diagnostic than a runtime check that only fires when a card using it
  is loaded.
- **Composes clauses, never sentences copied from anywhere.** An activated mana ability
  composes its cost and effect clauses (`"{T}: Add {G}."`); a triggered ability composes its
  condition and effects (`"When this enters the battlefield, draw a card."`); keywords join as
  a comma list. Output must be semantically complete enough to play, and must not reproduce
  official Oracle wording.
- **The escape hatch gets a parallel seam.** Scripted behavior is opaque Rust and cannot be
  derived, so a scripted card supplies a hand-authored fallback string alongside its abilities,
  and §4's bidirectional validation requires one exactly when `scripted: true`.

### 7. Protocol projection

- `CardView.rules_text` carries the generated string from §6. The field is named for what it
  holds: text the server generated, not text copied from a card.
- `CardView.functional_id` projects §2's identity verbatim — the provenance-neutral join key a
  client-local cache uses to look up presentation by stable identity, without the server or
  protocol needing to know such a cache exists.
- **The internal IR never crosses the boundary.** `Ability`, `Effect`, `TargetSpec`, and the
  rest stay engine-internal. `rules_text` plus the existing display fields (`type_line`,
  `mana_cost`, `power`/`toughness`, `keywords`) are the only derived-for-display surface, so a
  client never interprets the engine's ability vocabulary to describe semantics.
- **One-view reconstruction is preserved.** Both fields ride the existing `CardView`, already
  included wherever a card is shown — no second round trip, no client-held state.

### 8. Client-local enrichment is bounded, not designed here

A client may resolve `functional_id` against a locally cached, separately sourced mapping for
display enrichment. Three constraints bind any such feature:

- It is **optional presentation enrichment, never authoritative game state.** A client with no
  cache renders `rules_text` and plays correctly; a stale or missing entry falls back to
  `rules_text` for that card.
- **The server and engine never fetch, store, or require it.** Nothing in this model changes
  if the feature does not exist.
- It does not pre-approve any weakening of the distribution rules. What may actually be
  fetched, by whom, and under what consent needs its own explicit decision.

## Consequences

- **Easier.** Adding a card is adding one file, so concurrent authoring stops colliding.
  Identity is authored once and never renumbered, so decklists and printings are stable
  against catalog churn. Display text cannot drift from behavior, because it is generated from
  the same data the engine executes.
- **Harder / given up.** The catalog becomes thousands of small files rather than one readable
  array, so surveying it means tooling rather than reading. The build script becomes
  load-bearing infrastructure: a bug there fails every build. Interned ids are stable only
  within a build, which permanently rules out writing a `CardId` anywhere durable.
- **The IR, not this layout, is the binding constraint on coverage.** This decision makes
  authoring cheap; what a card can *say* is bounded by ADR 0003's vocabulary, and that is
  where catalog growth is actually gated.
