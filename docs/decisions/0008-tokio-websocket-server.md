# ADR 0008: tokio + tokio-tungstenite for the server transport

- Status: accepted
- Date: 2026-07-11
- Issue: #30

## Context
`rune-server` owns layers 1–2 (lobby + rooms) per `docs/brief.md`: WebSocket
connections, sessions, and turn timers. Until now it shipped as a synchronous
scaffold `main()` with an empty runtime dependency set — `Cargo.toml` noted that
"tokio, tokio-tungstenite, etc. arrive with the first server milestone." This is
that milestone: the server must run an async runtime and accept WebSocket client
connections (layer 1).

Per `AGENTS.md`, dependency additions in the server need an ADR recording the
crate choices and why. The realistic options for the async transport:

- **Runtime.** `docs/brief.md` ("Concurrency Model") already commits the project
  to Tokio: one lightweight task per connected client, one task per room, a
  work-stealing scheduler over an OS thread pool. `async-std` and `smol` exist
  but are less widely used and would contradict the brief; there is no reason to
  diverge.
- **WebSocket.** `tokio-tungstenite` is the de-facto async WebSocket crate: a
  thin Tokio adapter over `tungstenite`, integrating with `tokio::net` streams.
  Alternatives (`fastwebsockets`, a full framework like `axum`'s WS extractor)
  either add a large HTTP framework we do not yet need or are less mature. A raw
  `tungstenite` sync loop would fight the async runtime.
- **Async combinators.** `tokio-tungstenite` exposes the connection as a
  `futures` `Stream`/`Sink`, so `futures-util` is needed for `StreamExt`/`SinkExt`.
- **Logging.** The acceptance criteria require logging the connection lifecycle.
  `tracing` + `tracing-subscriber` are the standard structured-logging stack and
  compose with async spans; `log` + `env_logger` would work but `tracing` is the
  better long-term fit for per-connection/per-room spans.

These crates and their transitive dependencies are MIT / Apache-2.0 (ADR-0005,
`deny.toml`). TLS is explicitly **out of scope** for this milestone, so
`tokio-tungstenite` is pulled with `default-features = false` and only the
`handshake` feature — no `native-tls`/`rustls` surface enters the graph.

The engine's "zero I/O" hard rule is unaffected: all of these dependencies live
in `rune-server` only. `crates/rune-engine` keeps its I/O-free dependency set
(ADR-0006); nothing here leaks a runtime, socket, or timer into the engine.

## Decision
`rune-server` may depend on the async transport stack:

- **`tokio`** (features `rt-multi-thread`, `net`, `macros`, `signal`, `sync`,
  `io-util`, `time`) — the async runtime, TCP listener, Ctrl-C signal, and the
  `watch` channel used to broadcast graceful shutdown.
- **`tokio-tungstenite`** (`default-features = false`, feature `handshake`) — the
  server-side WebSocket handshake (`accept_async`) and framed message I/O. No TLS
  feature is enabled; TLS termination, if ever needed, gets its own ADR.
- **`futures-util`** — `StreamExt`/`SinkExt` over the WebSocket stream.
- **`tracing`** + **`tracing-subscriber`** (feature `env-filter`) — structured
  connection-lifecycle logging, filterable via `RUST_LOG`.

Scope and guardrails:

- These live in `rune-server` **only**. No async/runtime/socket/timer dependency
  may enter `crates/rune-engine` (hard rule, `AGENTS.md`); the engine stays a pure
  state machine invoked from the server.
- The server holds **no game logic**. This milestone proves the transport with an
  echo; rules stay in `rune-engine`.
- New crates must remain MIT-compatible per ADR-0005 and pass `deny.toml`.

## Consequences
- **Easier:** the server can accept real WebSocket clients and run the
  task-per-connection / task-per-room model the brief describes; later milestones
  (#11 room task, the GameView/ChooseAction message loop) build on this runtime
  instead of re-litigating the transport choice.
- **Harder / given up:** `rune-server`'s dependency graph and build time grow, and
  the crate now carries a runtime. That is inherent to being the I/O layer and is
  precisely why the boundary with the pure engine is a hard rule.
- Future contributors have a precedent: async/runtime/network dependencies belong
  in the server (and future clients), never in the engine.
