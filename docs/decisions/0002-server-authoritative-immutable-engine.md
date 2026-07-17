# ADR 0002: Server-authoritative rules; immutable engine state

- Status: accepted
- Date: 2026-07-10

## Context

Magic rules are enormous and stateful, and hidden information matters. Splitting rules
knowledge across clients and the server would multiply bugs, create inconsistent outcomes,
and expose authority to clients. See `docs/brief.md` for the full rationale.

## Decision

All rules live in `rune-engine`. `apply_action(&GameState, &Action, &CardDatabase) ->
GameState` returns a new state. Clients receive personalized `GameView` values and may submit
only an issued `action_id` plus server-enumerated choices. The engine performs no runtime I/O;
the server owns networking, rooms, policy, and time.

## Consequences

Undo, replay, resync, spectating, simulation, AI tree search, and deterministic testing become
structural benefits. Clients stay replaceable and do not become rules authorities. Every
decision requires a server round trip, and exhaustive legal-action generation remains the
project's core engine complexity.
