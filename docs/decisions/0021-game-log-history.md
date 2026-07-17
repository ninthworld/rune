# ADR 0021: Structured game-log history

- Status: accepted
- Date: 2026-07-17
- Issue: #259

## Context

Game views are full snapshots. A client-side accumulated activity history would make
a reconnect or fresh mount depend on messages it did not receive.

## Decision

The pure engine appends sequence-numbered, structured public facts to `GameState`.
It retains the latest 200 entries. The server projects that bounded window into every
`GameView`, applying the same hidden-information policy as other view data. Event
payloads contain typed entity references and data, never pre-rendered prose; clients
compose their own readable text.

Two properties keep the projected history coherent and stable:

- **Emit at the seam, in causal order.** Each event is recorded where its fact occurs
  in the transition pipeline, not diffed from a before/after snapshot at the end. A
  step change is recorded on entry to the step, *before* the step's turn-based actions,
  so `step_changed` precedes the `cards_drawn`/`damage_dealt`/`permanent_died` it
  causes; `game_over` is recorded last, after its causes. This is what lets the
  agent-vs-agent transcript read as a coherent sequence.
- **Carry identity, don't re-resolve.** An event that references a permanent stores the
  immutable card identity alongside the (never-reused) `PermanentId`. The server names
  the object from that recorded identity, so a retained entry — a declared attacker, a
  dead creature — keeps its name after the permanent leaves play, instead of degrading
  to "unknown" once it is no longer on the battlefield.

Damage and life are distinct events: damage to a player or permanent (including
nonlethal) is `damage_dealt`; `life_changed` carries only non-damage life movement, so
a hit is never double-reported. "Died" is creature-only (CR 700.4): the single
creature-death seam logs `permanent_died`, while an Aura or other permanent moving to a
graveyard is an unlogged zone change.

## Consequences

Fresh clients render exactly the history carried by their first view, with no
load-bearing local accumulation. The bounded window limits snapshot size but does not
provide a complete match transcript. Adding an event requires engine emission at the
correct seam, receiver-safe projection, and protocol documentation. Because names are
snapshotted into events rather than re-resolved, a card whose oracle data changes mid-
match would still show its recorded name in old entries — an acceptable trade for
stable history.
