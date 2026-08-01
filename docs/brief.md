# SAGE project brief

SAGE — **S**erver **A**uthoritative **G**ame **E**ngine — is an open-source implementation of
Magic: The Gathering built on one deterministic Rust rules engine. The name describes a
property, not an ambition: SAGE hosts Magic, not card games in general, and no abstraction
layer exists to make it generic.

## What we are building, in order

**The long-term goal is XMage in the browser** — comparable rules and card coverage, on a pure
state-based server-authoritative engine, reachable without an install. **Then** make it
beautiful. That order is strict. Coverage, multiplayer, every screen size, and Arena-grade
presentation pursued at once is four unfinished things; one of them finished is a product.

**The first milestone is the vertical slice of that goal: two people click a link and play a
real game of Magic in a browser.** No install, no JVM, no version mismatch, no "did you get it
set up yet." That friction — not beauty, not mobile, not card count — is the thing XMage
cannot fix and we can.

## How we work

**Every change ends in a state the maintainer can sit down and play. Playing is the merge
criterion.** Feature presence is not a proxy for it: a checklist can be complete while the game
is unplayable, and only one of those two facts matters. Designs are cheap and disposable. ADRs
are written *after* a decision survives contact with working code, not before. Together these
make it structurally impossible to build a second UI before the first one is good enough to
play on.

Corollary: no roadmap document. Milestone prose is how "complete" drifts loose from "good."

## Architecture

### Rules engine

`crates/sage-engine` is a pure, single-game state machine, and the project's most valuable
asset. It owns zones, turns, priority, the stack, combat, legal-action generation, card
effects, triggers, replacement effects, computed characteristics, state-based actions,
deterministic randomness, and the embedded card catalog.

It performs **no runtime I/O** — no sockets, rooms, clocks, threads, or ambient randomness.
Runtime dependencies are `serde`/`serde_json`, for parsing compile-time-embedded card data
only ([ADR 0002](decisions/0002-serde-in-engine.md)); anything further needs an ADR. `build.rs`
reads card files at compile time and embeds the result.

Triggers are collected by diffing before/after states — never listeners. Characteristics are
computed fresh through the CR 613 layer system, never cached
([ADR 0005](decisions/0005-computed-characteristics-and-layers.md)). Each battlefield entry
mints a fresh `PermanentId`, which is how zone-change identity works.

### Server

`crates/sage-server` wraps the engine with network and session concerns: WebSocket connections,
opaque session tokens and reconnect, rooms and seats, deck validation, per-viewer view
projection with hidden-information redaction, and **all automation policy**.

The engine/server seam is load-bearing and correct
([ADR 0010](decisions/0010-priority-automation.md)): the engine answers pure rules questions
("does this seat have a meaningful action?") and holds no policy; the server owns the settle
loop, per-seat stop preferences, and reconnect durability. Keep it that way. Baking a UX
judgment into the rules layer is how the engine becomes unsustainable.

### Protocol

Before a game the server sends complete `LobbyView` values and receives `LobbyCommand`s; during
a game it sends personalized `GameView` values and receives a `ChooseAction` naming an
`action_id` the server already issued. `valid_commands`/`valid_actions` are the only sources of
interactivity.

`sage-protocol`, the TypeScript mirror, and [`protocol.md`](protocol.md) are one contract and
change together.

### Clients

The web client is the current milestone: DOM and CSS, with inline SVG and canvas allowed for
presentational overlays anchored to ids the server stated. What it must look like and how it must
hold at any zoom, resolution, and aspect ratio is [`client-design.md`](client-design.md), which is
binding. Its two jobs are to make a legal game playable and to
**make a settle legible**: when the server auto-passes you through several steps, you must be
able to tell what happened. That second job is the actual product hypothesis and the thing
XMage does badly.

`crates/sage-cli` is the terminal client. It proves the protocol is independent of the web UI,
and it is the playtest surface whenever the web client is unavailable.

## Card model

One functional definition per card under `crates/sage-engine/data/catalog/<functional_id>.json`,
identified by a stable `FunctionalId`; `build.rs` interns compact `CardId` handles that are
never authored or persisted ([ADR 0008](decisions/0008-functional-card-definitions.md)).
Printings under `data/sets/` carry bibliography only — reprinting changes no behavior.

Card behavior is **data**: structured characteristics, abilities, effects, and keywords, with a
declared `scripted` code escape hatch for the exceptional
([ADR 0003](decisions/0003-card-effect-ir-hybrid.md)). Player-facing rules text is generated by
the server from that same data, so display cannot diverge from behavior.

**The IR's expressive vocabulary is the binding constraint on coverage, not authoring
throughput.** It currently cannot express static abilities, non-self triggers, activation costs
beyond `{T}`, or target restrictions — so a lord, an anthem, or a cost reducer is not writable
at all. Growing that vocabulary is the primary engine workstream, and progress on it is
measured, not asserted (see [`compatibility-report.md`](compatibility-report.md)).

## Legal constraints

SAGE distributes no card images, no official frames, symbols, watermarks, or Wizards of the
Coast branding, no exact Oracle text or flavor text, no implication of affiliation, and no
monetization path. Cards render procedurally from structured data; rules text is
server-generated. The functional schema rejects presentation fields structurally.

A card's *functional data* — name and mechanical characteristics — may match a real card; the
bundled catalog draws from Core Set 2019
([ADR 0009](decisions/0009-real-functional-card-data.md)).

These constraints govern **what the project distributes**. A player may separately opt in, on
their own device, to their browser fetching card images from a third-party source (currently
Scryfall) — either just the illustration inside SAGE's own frame, or the entire card image.
Those images are cached device-local only and are never uploaded, proxied, served, bundled,
committed, or redistributed by the project
([ADR 0012](decisions/0012-user-side-card-art.md)). Bundled art, if any, is original and
project-owned.

Weakening any of the above requires explicit legal review and an ADR.

## Exclusions

- Collection ownership, trading, and marketplace features
- Client-side rules evaluation
- Official card presentation or branding in the project's own distribution
- Monetization
- Game-agnostic abstraction — SAGE hosts Magic
- Ante, subgames, and novelty mechanics until explicitly added by decision
