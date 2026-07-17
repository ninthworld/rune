# ADR 0008: Tokio WebSocket server transport

- Status: accepted
- Date: 2026-07-11
- Issue: #30

## Context

`rune-server` needs concurrent connections, room tasks, graceful shutdown, structured logging,
and JSON WebSocket transport. These runtime concerns must remain outside the pure engine.

## Decision

The server uses:

- `tokio` for the multithreaded async runtime, networking, signals, channels, and timers;
- `tokio-tungstenite` for WebSocket framing;
- `futures-util` for stream and sink helpers; and
- `tracing` with `tracing-subscriber` for structured logs.

The server accepts plain WebSockets. TLS termination is not implemented by this dependency
choice and requires a separate deployment or architectural decision.

These dependencies live in `rune-server`, never `rune-engine`, and must remain compatible with
the repository license and dependency policy.

## Consequences

Connections and rooms can run as lightweight tasks on one runtime, and server timers remain
isolated from deterministic engine state. The server gains a larger dependency and operational
surface, while the engine boundary remains unchanged.
