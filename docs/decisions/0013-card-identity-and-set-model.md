# ADR 0013: Card identity vs printing — a scalable set model

- Status: accepted
- Date: 2026-07-11
- Issue: #106

## Context

The engine ships one embedded file, `crates/rune-engine/data/cards.json`: a flat
array of six invented cards, each an `{ id, name, types, mana_cost, oracle_text,
power/toughness, abilities }` record parsed by serde into `CardData` and keyed by
`CardId` (`crates/rune-engine/src/card.rs`, ADR 0006, ADR 0007). That shape has
no notion of *where* a card was printed. It conflates two things Magic keeps
separate:

- a **card** — the rules object with a stable identity, one name, one mana cost,
  one type line, one set of abilities (the "oracle" printing-independent
  characteristics `CardData` already models); and
- a **printing** — a specific appearance of that card in a specific set, with a
  set code, a collector number, and a rarity. The same card is printed in many
  sets over its lifetime.

If printings and cards stay conflated, every reprint duplicates the card's rules
logic — its abilities IR (ADR 0007), its computed-characteristics seed values
(ADR 0010) — once per set it appears in. That is the exact failure the roadmap's
M3 ("A real card pool") forbids: *"adding a reprint to a second set file changes
no logic."* A card pool of any real size (M3 targets a ~100–150-card invented
starter set, and the model must scale to Scryfall-shaped data beyond that) cannot
carry N copies of a card's behavior for N printings.

Two sibling decisions depend on this one being settled first:

- **ADR 0012 (lobby protocol, PR #124)** carries a `game_setup` identifier in
  room config and a deck submission in the pre-game gate, and *explicitly defers*
  to this ADR two things: (1) **what setups exist and how a `GameSetup`/format is
  defined**, and (2) **the card-identity vocabulary a decklist references.** This
  ADR must *provide* both so 0012's lobby plumbing has a concrete target. This
  ADR does not restate 0012's lobby internals.
- The M1 exit criterion and issue **#109** ("Engine: GameSetup, deck loading,
  seeded shuffle, opening hands") need a `GameSetup` shape *now*, before the full
  card-data model exists.

The hard rules constrain the design (`AGENTS.md`, `crates/rune-engine/AGENTS.md`,
ADR 0002, ADR 0006, ADR 0007):

- **Zero I/O in the engine.** `rune-engine` has no tokio, no sockets, no
  filesystem, no timers. Card data is *embedded at compile time* (`include_str!`)
  and parsed in memory; serde is permitted for exactly that (ADR 0006). Any
  loading decision here must stay build-time embedding, never runtime I/O.
- **Rules logic is data on the card, plus a `CardId`-keyed code escape hatch**
  (ADR 0007). The identity model must *carry* that IR on the card exactly once,
  never per printing.
- **`GameState` is a pure `Clone`/`Eq` value type**; the `CardDatabase` is kept
  *out* of `GameState` and threaded as `&CardDatabase` (ADR 0007, ADR 0010). A
  card's stable identity is therefore an opaque key the engine reasons about, not
  a struct embedded in state.
- **No card images, no official frames, no WotC branding, no monetization**
  (`docs/brief.md` Legal Considerations). The brief simultaneously sanctions the
  *data* source: *"Source: Scryfall API for online play. Bundled JSON snapshot
  for offline play,"* and lists the Scryfall-shaped fields (`name`, `mana_cost`,
  `color_identity`, `type_line`, `oracle_text`, `power`/`toughness`, `loyalty`,
  `keywords`, `layout`). So the model may mirror Scryfall's *data* shape; it must
  never ingest art, frames, artist credit, or branding.

## Decision

RUNE models cards as **oracle identity and printing as two separate record
kinds**, with **all rules logic on the oracle card and none on the printing**.
The rules below are what the codebase follows once this model is implemented; the
M1-vs-M3 split (final subsection) says which parts land when.

### 1. Oracle card vs printing — two records, rules on the oracle only

- An **oracle card** is the printing-independent rules object. It owns a stable
  **`OracleId`** and carries exactly the characteristics the engine reasons
  about: name, mana cost, supertypes/types/subtypes, oracle text, power/toughness
  (and later loyalty, color identity, keywords), and its **ability IR (ADR
  0007)** — the declarative `abilities` plus, for cards the IR can't express, the
  `CardId`-keyed scripted escape hatch. This is today's `CardData`, promoted to
  the single home of behavior. **Rules logic lives here and only here.**
- A **printing** is a purely bibliographic record: a `set_code`, a
  `collector_number`, a `rarity`, and a reference to the `OracleId` it prints. It
  carries **no name, no cost, no types, no abilities, no rules of any kind** —
  everything mechanical is read through its `OracleId`. It also carries **no art,
  no frame, no artist, no branding** (Legal Considerations).
- **A reprint is a new printing record pointing at the same `OracleId`.** Adding
  a card to a second set is one printing entry and zero changes to rules logic —
  the M3 invariant, satisfied by construction.

`OracleId` is the engine's existing opaque card key. The current integer `CardId`
(`crates/rune-engine/src/id.rs`) *is* the `OracleId`: it already keys
`CardDatabase` and every rules read (`db.card(id)`, `abilities_of(db, id)`), and
nothing about that changes. For Scryfall compatibility (§6) an oracle record may
additionally carry Scryfall's external `oracle_id` (a UUID string) as a data
field the loader interns to a `CardId`; rules code never keys on the UUID.

### 2. File layout — `data/oracle.json` + `data/sets/<SET>.json`, embedded

The single `data/cards.json` is replaced by:

- **`crates/rune-engine/data/oracle.json`** — an array of oracle-card records
  (the current `CardData` fields keyed by `OracleId`). One entry per distinct
  card, regardless of how many sets print it.
- **`crates/rune-engine/data/sets/<SET>.json`** — one file per set, named by set
  code, holding that set's printing records: `{ oracle_id, collector_number,
  rarity }` entries. A set file references oracle cards; it never restates them.

Loading stays **compile-time embedding**, honoring the engine's zero-I/O rule:
`oracle.json` is embedded with `include_str!` exactly as `cards.json` is today,
and each set file is embedded the same way behind a small explicit manifest
(a `const` list of `include_str!`ed set snapshots) rather than any directory
walk — the engine must not read the filesystem at runtime, and enumerating files
at build time keeps the embed static and reviewable. `CardDatabase::from_json`
generalizes to build the oracle map from `oracle.json`; a parallel
`PrintingDatabase` (or an added map on `CardDatabase`) is built from the embedded
set files. serde over embedded snapshots (ADR 0006) is unchanged — there are just
more, better-separated snapshots. No new dependency is introduced.

### 3. Decks reference cards by oracle identity

A decklist entry is **`(OracleId, count)`** — printing-agnostic. Deck legality
and every in-game rule read the `OracleId`; a decklist never needs a set code or
collector number to be playable, and swapping which printing a player "owns"
changes nothing about the game. This is the **card-identity vocabulary ADR 0012
defers to this ADR**: the deck submission in 0012's pre-game gate carries oracle
ids (optionally with a display-only printing hint that legality and rules
ignore). One decklist is valid across every set that has ever printed its cards.

### 4. `GameSetup` and format config — engine data vs server config

Two layers, deliberately split so the pure engine holds no format policy and no
I/O:

- **`GameSetup` — engine data (a pure value type in `rune-engine`).** It holds
  exactly the rules-affecting parameters the engine needs to start and run a
  game: **player count, starting life total, starting hand size**, and the
  **mulligan rule** (London, per issue #111). It is a plain `Clone`/`Eq` value
  threaded into game construction (issue #109); it contains no networking, no
  named formats, and no deck-legality policy. This is the shape #109 builds.
- **Format config — server configuration (`rune-server`).** The server owns a
  registry mapping a **named `game_setup` identifier** (e.g. `"1v1"`,
  `"ffa-4"`, `"commander"` — the identifier ADR 0012 carries in room config) to
  a concrete engine `GameSetup` **plus deck-legality rules**: minimum/maximum
  deck size, per-card copy limit, singleton, and any banned/restricted lists.
  **Deck legality is validated by the server's pre-game gate (ADR 0012), not by
  the engine** — it is matchmaking/format policy, not a rule of an in-progress
  game, and keeping it server-side preserves the engine's purity and its freedom
  from format churn.

So the `game_setup` string in ADR 0012's room config is a **key into the server's
format registry**, which yields the engine `GameSetup` the room starts its game
with. This ADR provides both halves 0012 defers: the `GameSetup` value shape and
the identifier vocabulary that names the setups.

### 5. Migration of the six invented cards → oracle fixtures

The six cards in `data/cards.json` — **Thornback Boar, Riverbank Otter,
Emberfang Jackal, Stonehide Basilisk, Forest, Verdant Scout** — become six
**oracle-card records** in `data/oracle.json`, keeping their existing integer ids
as their `OracleId`s (so `CardId(1)` is still Thornback Boar and every existing
`card.rs` test keys unchanged). They are **retained as engine test fixtures, not
a shipped set**: they exist to exercise the pipeline (a vanilla creature, a basic
land with a mana ability, an ETB-draw trigger) exactly as they do today. No
printing records are required for the fixtures; if a set file is wanted for
coverage, a synthetic fixture set (e.g. `data/sets/FIX.json`) can print them, but
the fixtures' role is to prove oracle identity + abilities, independent of any
real set. The invented starter *set* is separate M3 content, not this migration.

### 6. Scryfall-shaped data, with the art/branding prohibition intact

The oracle record deliberately mirrors the **data** shape Scryfall exposes and
the brief already lists — `oracle_id`, `name`, `mana_cost`, `type_line`
(structured here, rendered by `CardData::type_line`), `oracle_text`,
`power`/`toughness`, `loyalty`, `color_identity`, `keywords`, `layout` — so real
Scryfall-style oracle data can populate the model at M3 without reshaping it.
This is an explicit project decision, grounded in `docs/brief.md` ("Source:
Scryfall API … Bundled JSON snapshot"). The printing record mirrors only
Scryfall's **bibliographic** fields (set code, collector number, rarity). What
the model **never** ingests: `image_uris`/any art, card frames, artist credit,
and WotC branding (`docs/brief.md` Legal Considerations). We model the data shape
Scryfall uses; we never pull its images or branding.

### 7. M1 vs M3 split — what lands now, what waits

This ADR is an **M1** decision (roadmap M1 feature "ADR 0013: card identity vs
printing"); it ships **no card-data code**. The split:

- **Now (M1):** *only the `GameSetup` basics are pulled forward.* Issue #109
  ("Engine: GameSetup, deck loading, seeded shuffle, opening hands") implements
  the §4 engine `GameSetup` value type (player count, starting life, starting
  hand size) and deck loading against the *existing* `CardDatabase`/`cards.json`.
  `cards.json` is **not** split in M1 — decks referencing `OracleId`s work
  against the current single-file database unchanged. The server-side format
  registry (§4) is scoped with ADR 0012's lobby/room work.
- **Later (M3):** the full oracle/printing/set-file model — §1 record split, §2
  `oracle.json` + `data/sets/<SET>.json` layout and its loading, the
  `PrintingDatabase`, and the invented starter set — lands under roadmap M3 ("A
  real card pool"), whose exit criterion this model is written to satisfy:
  *adding a reprint to a second set file changes no logic.* The six-card
  migration (§5) happens as part of that M3 file-layout work; until then the
  fixtures stay in `cards.json`.

This ADR decides the model; M1 pulls forward only `GameSetup`, and M3 implements
the rest.

## Consequences

- **Easier.** A card's rules logic exists exactly once, on its oracle record, no
  matter how many sets print it — the M3 "reprint changes no logic" invariant is
  structural, not a discipline to remember. Decklists are printing-agnostic, so a
  deck outlives the sets it was built from and 0012's deck submission has a
  stable vocabulary. The engine gains a concrete `GameSetup` value type it can
  start games from (#109) while all format policy and deck legality stay in the
  server, keeping the engine pure and format-agnostic. The model maps cleanly
  onto real Scryfall oracle data at M3 with no reshaping, and the existing serde
  + `include_str!` embedding (ADR 0006) extends to many files with no new
  dependency and no runtime I/O.
- **Harder / given up.** Card data grows from one file to an oracle file plus a
  set file per set, and the loader gains an explicit set manifest and a printing
  map — more moving parts than today's single array, and the set manifest must be
  updated by hand when a set is added (the price of static, reviewable,
  zero-I/O embedding rather than a directory walk). Deck-legality validation is
  split from the engine into the server, so the two must agree on what an
  `OracleId` deck means — mitigated by decks referencing only oracle identity.
  Introducing an external Scryfall `oracle_id` UUID alongside the engine's
  integer `CardId` means the loader owns the intern mapping.
- **Deferred.** Everything in §1/§2/§5 is M3 implementation, not M1. Also
  deferred: the color-identity/keywords/layout fields on the oracle record (added
  when the effect IR and rendering need them, per ADR 0010's characteristic
  growth), double-faced/split `layout` handling, the invented starter set's
  contents, banned/restricted list mechanics, and any format beyond the 1v1
  `GameSetup` #109 needs first. The server-side format registry's concrete
  encoding is settled with ADR 0012's lobby/room implementation, which consumes
  the `game_setup` identifier defined here.

## Follow-up issues

- #109 — engine: `GameSetup`, deck loading, seeded shuffle, opening hands (M1;
  the `GameSetup` basics pulled forward by this ADR).
- M3 card-pool work (roadmap M3) — implement the oracle/printing record split,
  the `data/oracle.json` + `data/sets/<SET>.json` layout and loading, migrate the
  six invented cards to oracle fixtures, and ship the invented starter set.
