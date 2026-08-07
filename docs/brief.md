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
effects, triggers, computed characteristics, state-based actions, deterministic randomness,
and the embedded card catalog.

**The replacement-effect layer covers two events: a permanent entering the battlefield, and
damage** ([ADR 0019](decisions/0019-replacement-effects.md)). That entry is a *value* every road
onto the battlefield builds and hands to one function, which collects the replacements that apply
to it — the entering object's own (CR 614.1c: "enters tapped", "enters with counters") and any
an ability created for the turn — has the affected permanent's controller order them when more
than one does (CR 616.1), and applies each at most once (CR 614.5). Damage is the same shape at
the one seam damage is dealt: a prevention shield rewrites the event before it lands (CR 615), so
prevented damage is never marked, never lethal, and never life loss. No other event can be
replaced: regeneration, a draw, life gained, and a permanent *leaving* the battlefield are all
out of scope, and the compatibility report says so as an exclusion.

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
presentational overlays anchored to ids the server stated. Its two jobs are to make a legal game
playable and to **make a settle legible**: when the server auto-passes you through several
steps, you must be able to tell what happened. That second job is the actual product hypothesis
and the thing XMage does badly, and it is not yet designed — today a settle reaches the player
as a run of log lines.

#### Which document wins

Everything in `docs/` is current — there is no superseded material to sift — but "current" is not
the same as "the last word", and for the client's *appearance* the order is stated once, here:

1. **`clients/prototype`** is the highest authority on how a screen should look and behave. It is
   a throwaway sandbox: nothing ships from it, and `client-design.md` does not govern it.
2. **[`client-design.md`](client-design.md)** records what the prototype settled, and is binding
   on `clients/web`. Where the two disagree the prototype wins, and §8 lists every rule the
   prototype retired. Its §10 holds the open questions the prototype did not answer.
3. **`clients/web`** is built to that document. It is the thing that ships.

This ordering governs appearance and layout only. It never overrides a **hard rule** — zero game
logic in the client, view reconstructability, the protocol contract — and it never overrides an
accepted **ADR**, which records a decision that already survived contact with working code.
Nothing visual is worth bending one of those.

`crates/sage-cli` is the terminal client. It proves the protocol is independent of the web UI,
and it is the playtest surface whenever the web client is unavailable.

`crates/sage-scenario` is a **development-only** contributor tool, not a client and not a
product surface. It builds an exact game position from a checked-in file, serves it on a
loopback socket, and opens the shipping web client on it — so a mechanic or an interaction can
be played by hand without playing five turns to reach it. Everything past the first state is a
real game: the engine offers the actions, the server projects the views and drives the AI seat,
and the client is the built bundle pointed at a socket. It adds no protocol command, and nothing
ships from it. Format and vocabulary: [`scenarios.md`](scenarios.md).

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
throughput.** It has grown past the vocabulary the first cards needed: static abilities, triggers
that observe another permanent, activation costs of mana and loyalty as well as `{T}`, targets
restricted by type and by what a permanent currently is, tokens, planeswalkers, emblems, and
effect amounts that scale with a count of permanents are all writable now. What remains
unwritable is narrower and more specific — a cost reducer, a cost paid by sacrificing, a modal
spell, an X cost, an anthem pointed at anything but creatures its controller controls.

Growing that vocabulary is the primary engine workstream, and progress on it is measured, not
asserted. **This document names no exclusion list of its own**: the authoritative one is
generated from the catalog and
[`data/exclusions.json`](../crates/sage-engine/data/exclusions.json), and every exclusion carries
the single blocker that keeps it out. See
[`generated/compatibility.md`](generated/compatibility.md) for the report and
[`compatibility-report.md`](compatibility-report.md) for how it is produced and kept honest.

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
