# Engine agent guide

`sage-engine` is the pure, deterministic rules state machine. Read the engine section of
[`docs/brief.md`](../../docs/brief.md) before changing it.

## Hard rules

- Pure functions over immutable `GameState`. `apply_action` clones; it never mutates.
- **No dependencies on tokio, networking, timers, threads, wall-clock time, or randomness
  without an injected seed.** Runtime dependencies are limited to serde for embedded card
  data; adding another dependency requires an ADR.
- No listeners/observers. Triggers are collected by diffing before/after states.
  Characteristics are computed fresh by pure functions (layer system), never cached.
- Every permanent gets a fresh `PermanentId` on battlefield entry — zone-change identity
  is the mechanism, do not add zone-change counters.
- Every rules behavior change ships with tests in the same PR. Rules bugs get a
  regression test named after the issue (`issue_123_...`).
- Cards are **data**, authored against [`docs/card-schema.md`](../../docs/card-schema.md):
  a functional definition per card under a stable `functional_id`, no presentation
  assets (the schema rejects them structurally), and code-defined behavior only via the
  declared `scripted` escape hatch.
- **A definition's printed characteristics are checked against the printed card, not
  against the tests written over it.** `build.rs` validates *shape*; the accuracy gate is
  `tests/printed_characteristics.rs`, reading a fixture transcribed from the set by
  `scripts/printed-characteristics.py`. That script never reads `data/catalog/`, and it
  must not: a fixture regenerated from the thing it checks is a gate that passes forever
  and proves nothing. Rules text stays out of it — the project ships none, and a test
  fixture is not an exception.
- **Never write a `CardId` down.** `build.rs` assembles `data/catalog/` + `data/sets/`
  at compile time and interns `CardId(0..n)` from the sorted `FunctionalId`s (ADR 0008
  §3), so authoring one card renumbers its neighbours. Name a card by its
  `functional_id` and resolve the handle (`CardDatabase::card_id`) — in decklists, in
  `scripted.rs`, and in tests (`crate::fixtures::fixture`).
- `build.rs` may read catalog files at compile time. The running engine performs no I/O.
  Shared validators live in `src/catalog/` so build-time and load-time checks agree.
- **Automation policy belongs to the server, not here.** The engine may expose pure rules
  *predicates* (`automation.rs`); the loop, the per-seat preferences, and the pacing
  decisions live in the room layer (ADR 0010). This seam is the reason the engine stays
  sustainable — do not move policy across it.

## The IR is the constraint

Catalog coverage is limited by what the ability IR can *express*, not by authoring
throughput. **Where the vocabulary currently stops is surveyed in
[`docs/card-schema.md`](../../docs/card-schema.md#how-far-the-vocabulary-reaches)** — costs,
triggers and conditions, amounts, what is chosen at announcement, layer 6 and its two ability
accessors, combat restrictions, tokens/faces/emblems/copies, targeting and statics and
control, the mid-resolution question queue, and the replaceable events. Read that before
concluding a card is unwritable, and extend it in the same PR that extends the IR.

The catalog was selected as cards this vocabulary can say, so the empty `scripted.rs` table
is not evidence of expressiveness. Growing the vocabulary is the primary engine workstream;
each new primitive is one enum variant plus every exhaustive match that consumes it, across
the resolver and the server's rules-text formatter — both are wildcard-free, so the compiler
names every site.

**`data/exclusions.json` is the maintained list, and it is the one that has to stay
right.** Every exclusion names a single blocker; `make compat` regenerates
`docs/generated/compatibility.md` from it, and `cargo test` fails if the committed report has
drifted. The report claims to name *every* mechanic considered and left out of scope, so a
gap that is neither built nor named makes it false: when a change builds something an entry
names, drop or narrow that entry in the same PR, and when a change settles for a limitation,
name it there rather than in a comment. Prose in an `AGENTS.md` or in the brief drifts
silently; that file does not, so when a paragraph anywhere and an entry there disagree,
believe the entry and fix the paragraph.

**Ask a permanent for its abilities through `abilities_of_permanent`, never off the printed
face.** It is the only path any collector uses, which is what makes a removed trigger unable
to still fire and a granted mana ability unable to go unoffered. Its deliberately smaller
sibling `stored_abilities_of_permanent` exists for exactly one caller — the gate asking
whether a *source* still has the static ability it is about to contribute — and is the one
place the layer-6 walk is cut so it cannot recurse. Do not widen that gate: what it costs is
named in `exclusions.json` rather than fixed.

**Every catalog entry earns a test at its own tier, and the tiers are not
interchangeable.** A card with abilities, spell effects, an attachment, a cost or a trait is
*behavioral*: it wants a test that drives the real `apply_action` and asserts a state
transition, including the composition — a card whose every effect is covered elsewhere can
still be wrong in the order it does them, or ask a question the effect before it made
unanswerable. A card that is only its printed characteristics is *vanilla*: assert those, and
stop. A new prompt or view shape additionally wants the protocol projection and, for a
distinct interactive shape, a browser rendering. `docs/generated/test-coverage.md` names the
definitions no engine source mentions; it is regenerated by `make compat` and is a **review
aid, never a bar to clear** — a test named after a card proves nothing that a test driving one
does.

## Commands

- `cargo test -p sage-engine`
- `cargo clippy -p sage-engine --all-targets -- -D warnings`
