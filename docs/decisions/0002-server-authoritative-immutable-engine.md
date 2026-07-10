# ADR 0002: Server-authoritative rules; immutable engine state

- Status: accepted
- Date: 2026-07-10

## Context
MTG rules are enormous and stateful. Splitting rules knowledge across client and
server multiplies bugs and enables cheating. See docs/brief.md for full rationale.

## Decision
All rules live in rune-engine behind `apply_action(&GameState, Action) ->
GameState` with immutable state. Clients receive personalized GameViews and may
only submit an `action_id` from `valid_actions[]`. The engine has no I/O
dependencies; the server (layers 1-2) owns all networking and timing.

## Consequences
Undo, replay, resync, spectating, and AI tree search are structural freebies.
Clients are simple and unable to cheat. Cost: every interaction is a round trip,
and the engine must generate exhaustive valid_actions — that generator is the
project's core complexity.
