# ADR 0024: User-side card art with pluggable sources

- Status: accepted
- Date: 2026-07-18

## Context

The card frame was designed around a reserved **art window** from the start
(`docs/design/ui-design-notes.md` §Card render): the region holds a procedural
monogram today and an image eventually, with the name band, cost pips, type
line, and P/T pill keeping their positions either way. UI and frame design work
has stalled on that "eventually" — there was no way to put any image in the
window, so every visual decision was being made against a face that will not be
the shipped face.

The legal policy (`docs/brief.md`, Legal constraints) is deliberately
conservative: no card images, official frames, symbols, or WotC branding — and
any weakening requires an explicit decision. The constraint that matters legally
is **what the project distributes**: the repository, the built client, and the
server must not contain or serve official imagery. That constraint says nothing
about what a player's own browser fetches from a third party at the player's own
request — the model long established by community tools, where the application
ships imageless and users may point it at an image source themselves.

Two other forces:

- The protocol already carries `CardView.functional_id`, reserved (ADR 0013/0018
  §9) for exactly this: a stable presentation identity a *client-local* cache can
  enrich without any wire change.
- The embedded catalog ships functional stand-in cards with invented names
  ("Emberfang Jackal"), so an external image source cannot be queried by card
  name alone today. Migrating the catalog to real cards is a large engine-and-
  test change (hundreds of references) that deserves its own batch.

## Decision

Card illustrations are a **client-local presentation concern with pluggable,
player-selected sources**, keyed by `functional_id`. Three sources exist, chosen
in a settings surface and stored as a device preference:

1. **Procedural** (default): the vector frame with the monogram placeholder —
   exactly the pre-art client. Nothing downloads; offline play is unaffected.
2. **Bundled**: original, project-owned illustrations (the RUNE-generated set)
   shipped with the client under `clients/web/public/card-art/` and gated by a
   manifest. Only art the project may redistribute is ever added here.
3. **Scryfall** (opt-in): the player's browser fetches real card illustrations
   directly from Scryfall after an explicit consent step, rate-limited per
   Scryfall's guidelines, and caches them in IndexedDB **on the player's device
   only**. Only the bare illustration (`art_crop`) is used — it renders inside
   RUNE's own procedural frame; official frames, symbols, and full card scans
   stay excluded. Additional sources can be added behind the same interface
   later.

Rules the codebase follows:

- The repository, built client, and server never contain or serve official
  imagery. Downloaded art never leaves the player's device and is never
  re-uploaded, proxied, or shared (not even to other clients in the same game).
- Art is cache, never state: the UI must remain fully reconstructable from one
  `GameView` with the art store empty (cards render procedurally), preserving
  the client's reconstruction invariant. Clearing the cache is always safe.
- The renderer treats art as a looked-up texture keyed by the card's
  `functional_id`; it never fetches, and no game data flows to any art source
  beyond the card names being resolved.
- While the catalog ships functional stand-ins, a client-side mapping
  (`clients/web/src/card/art/artMap.json`) pairs each `functional_id` with a
  real card whose illustration fits its color and flavor; resolution falls back
  to the card's own name, so the map shrinks away once the catalog migrates to
  real cards (tracked on the roadmap as its own batch).

## Consequences

- UI and frame design are unblocked: the art window renders real pixels at the
  field and hand tiers (dense tiers keep their information budget), and the
  inspector shows the illustration, without waiting on the catalog migration or
  commissioned art.
- The project's distribution posture is unchanged — what ships is exactly as
  imageless as before, and the brief's Legal constraints section now records the
  user-side carve-out explicitly.
- The client gains its first IndexedDB dependency and a background-loading
  pipeline (rate-limited queue, texture registry, change subscription). All of
  it is injectable and covered by offline unit tests; no test touches the
  network.
- Scryfall's availability is outside our control. Failures degrade to the
  procedural face silently, per card — an unavailable illustration can never
  block play.
- The art mapping is a curated flavor pairing, not a claim of functional
  equivalence; it is provisional by design and deleted card-by-card as the
  real-card catalog migration lands.
