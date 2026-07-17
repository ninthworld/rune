# ADR 0002: Server-authoritative rules; immutable engine state

- Status: accepted
- Date: 2026-07-10

## Context

Magic rules are stateful and hidden information matters. Splitting rules knowledge across
clients and the server would create inconsistent outcomes and expose authority to clients.

## Decision

All rules live in `rune-engine`. `apply_action(&GameState, &Action, &CardDatabase) ->
GameState` returns a new state. Clients receive personalized `GameView` values and may submit
only issued actions and server-enumerated choices. The engine performs no runtime I/O; the
server owns networking, rooms, policy, and time.

## Consequences

Replay, resync, simulation, and testing are deterministic. Clients stay replaceable and do
not become rules authorities. Every decision requires a server round trip, and exhaustive
legal-action generation remains core engine complexity.
