# Seed backlog

Convert the **still-open** items below into GitHub issues (agent-task template),
in order. Ordering follows the development sequence in docs/brief.md. Each is
sized for one PR unless noted.

> Status legend: ✅ done (issue merged) · ⏳ needs an issue.
> Last reconciled against GitHub issues + `main`: 2026-07-11.

## Foundation — ✅ complete
1. ✅ **protocol: add serde** — serde/serde_json in rune-protocol; GameView +
   ChooseAction round-trip tests; docs/protocol.md field table. Landed directly
   in code (no dedicated issue): see `crates/rune-protocol` + round-trip tests.
2. ✅ **client: commit package-lock.json and switch CI to `npm ci`** — issue #17
   (lockfile committed in PR #14).
3. ✅ **client: add ESLint (typescript-eslint, react-hooks)** — subsumed by the
   coding-standards issue #9 (client enforcement PR).
4. ✅ **ci: add cargo-deny (licenses/advisories)** — issue #16.

## Engine — ✅ complete
5. ✅ **engine: GameState skeleton** — issue #15.
6. ✅ **engine: phase FSM** — issue #21.
7. ✅ **engine: action generator** (`valid_actions`) — issue #23.
8. ✅ **engine: action pipeline** (validate → clone → apply → SBA → triggers) —
   issue #24.
9. ✅ **engine: card database loader** — issue #25.

## Server + CLI — ⏳ needs issues
10. ✅ **server: tokio + WebSocket skeleton** — layer 1 accepts connections; ADR for
    dependency additions — issue #30 (ADR-0008; tokio + tokio-tungstenite accept
    loop with graceful shutdown).
    - ✅ **lobby: wire connections into rooms** — layer-1 room registry
      (`Arc<RwLock<...>>` of active rooms) that seats each accepted+handshaken
      connection via an auto-pairing "next open seat" policy and routes it into the
      room task, replacing the #30 echo so a running binary can host a game end to
      end — issue #42.
11. ✅ **server: room task** — one async task per room owns one engine instance,
    routes `action_id`s through `valid_actions`/`apply_action`, and broadcasts
    personalized, hidden-zone-redacted GameViews; seats held open across
    disconnects — issue #31.
12. ✅ **cli: interactive client** — numbered valid_actions, stdin choice loop
    against a local server (dev sequence step 3) — issue #32 (`rune-cli` connects
    over WebSocket, renders each personalized `GameView`, and echoes back a chosen
    `action_id`; end-to-end test drives the real room task over an in-memory
    transport).
13. ⏳ **cli: LLM agent mode** — GameView JSON in, action_id out, timeout fallback.

## Client (after protocol serde) — ⏳ needs issues
14. ✅ **client: WebSocket store** — Zustand store holding latest GameView; reconnect
    resends full state (test the reconstruct-from-one-GameView invariant) — issue #34.
15. ✅ **client: port the Pixi card factory from prototypes/ui-battlefield-v3.html**
    into src/, reading src/tokens.ts (reference only — reimplement, don't import) —
    issue #35 (`src/card/cardFactory.ts` + token additions; smoke tests).
16. ✅ **client: battlefield bands + hand + action bar** rendering GameView from the
    store; subject-owned action routing per ADR 0004 — issue #36 (`src/table/*`:
    pure GameView→scene mapping, Pixi bands/hand, React action bar/tiles/prompt +
    entity overlay; routing + reconstruct-from-one-GameView tests).
