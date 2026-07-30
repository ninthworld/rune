# ADR 0001: Server-authoritative rules; immutable engine state

- Status: accepted
- Date: 2026-07-30

## Context

Magic's rules are enormous and stateful, and hidden information is load-bearing: a player's
hand, library order, and pending decisions must never reach an opponent. Splitting rules
knowledge across clients and the server would multiply bugs, allow two clients to disagree
about the same board, and put authority over the game in a process the server does not
control.

## Decision

All rules live in `sage-engine`, as one pure function over immutable state:

```
apply_action(&GameState, &Action, &CardDatabase) -> GameState
```

It returns a new state rather than mutating one. `GameState` is a `Clone`/`Eq` value type
holding no caches, no derived values, and no handles to anything outside itself.

The engine performs **no runtime I/O** — no sockets, filesystem, clocks, threads, or ambient
randomness. The server owns networking, rooms, sessions, timers, and every policy decision.

Clients receive personalized `GameView` values and may submit only an `action_id` the server
already issued, plus choices the server enumerated. A client is never asked what is legal,
and never told anything its player may not know.

## Consequences

- **Easier.** Undo, replay, resync, spectating, simulation, AI tree search, and deterministic
  testing all fall out of the value semantics rather than being features to build. Clients
  stay replaceable: a terminal client and a browser client are the same protocol consumer.
  Hidden-information leaks are structurally preventable, because redaction happens in one
  projection layer rather than at every call site.
- **Harder / given up.** Every decision costs a server round trip, so there is no
  optimistic local prediction. Exhaustive legal-action generation is the engine's core
  complexity and its main performance concern: `apply_action` validates a submitted action by
  regenerating the legal set and checking membership, so that generator is on the hot path of
  every interaction.
- Whole classes of shortcut are permanently unavailable — a client cannot grey out an
  unaffordable card on its own, and any such affordance must be something the server states.
