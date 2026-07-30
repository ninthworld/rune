# ADR 0007: Structured game-log history in `GameState`

- Status: accepted
- Date: 2026-07-30

## Context

Views are full snapshots, and the client rebuilds its entire UI from the latest one. A history
accumulated client-side would break that: a reconnect or a fresh mount would depend on
messages the client never received, and two clients that joined at different times would show
different histories of the same game.

The history therefore has to come from the server, in every view, like any other public fact.

## Decision

The pure engine appends sequence-numbered, structured public facts to `GameState`, retaining
the latest 200 entries. The server projects that bounded window into every view, applying the
same hidden-information policy as other view data. Event payloads carry typed entity
references and data, never pre-rendered prose; clients compose their own readable text.

Two properties keep the projected history coherent:

- **Emit at the seam, in causal order.** Each event is recorded where its fact occurs in the
  transition pipeline, not diffed from a before/after snapshot at the end. A step change is
  recorded on entry to the step, *before* the step's turn-based actions, so `step_changed`
  precedes the `cards_drawn` / `damage_dealt` / `permanent_died` it causes, and `game_over` is
  recorded last, after its causes. This is what makes a transcript read as a sequence rather
  than a pile.
- **Carry identity, don't re-resolve.** An event referencing a permanent stores the immutable
  card identity alongside the never-reused `PermanentId`. The server names the object from
  that recorded identity, so a retained entry — a declared attacker, a dead creature — keeps
  its name after the permanent leaves play instead of degrading to "unknown".

Damage and life are distinct events: damage to a player or permanent, including nonlethal, is
`damage_dealt`; `life_changed` carries only non-damage life movement, so a hit is never
double-reported. "Died" is creature-only (CR 700.4) — the single creature-death seam logs
`permanent_died`, while an Aura or other permanent moving to a graveyard is an unlogged zone
change.

## Consequences

- Fresh clients render exactly the history carried by their first view, with no load-bearing
  local accumulation.
- The bounded window limits snapshot size but does not provide a complete match transcript.
- Adding an event requires engine emission at the correct seam, receiver-safe projection, and
  protocol documentation — three coordinated changes, deliberately.
- Because names are snapshotted into events rather than re-resolved, a card whose data changed
  mid-match would still show its recorded name in old entries. That is the intended trade for
  stable history.
