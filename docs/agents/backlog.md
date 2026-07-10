# Seed backlog

Convert these into GitHub issues (agent-task template) once the repo is pushed.
Ordering follows the development sequence in docs/brief.md. Each is sized for
one PR unless noted.

## Foundation
1. **protocol: add serde** — serde/serde_json in rune-protocol; GameView +
   ChooseAction round-trip tests; update docs/protocol.md field table.
2. **client: commit package-lock.json and switch CI to `npm ci`** (see ci.yml TODO).
3. **client: add ESLint (typescript-eslint, react-hooks)** and wire into
   `npm run lint` + CI + Makefile.
4. **ci: add cargo-deny (licenses/advisories)** as a non-required check initially.

## Engine (in order)
5. **engine: GameState skeleton** — players, zones, turn/phase enums per brief.
6. **engine: phase FSM** — full step sequence, extra-turn/phase capable.
7. **engine: action generator** — `valid_actions(state)` for pass-priority only.
8. **engine: action pipeline** — validate → clone → apply → SBA loop → trigger
   collection scaffolding (pure diff-based, tests first).
9. **engine: card database loader** — CardId → immutable card data from bundled
   JSON snapshot (see brief "Shared: Card Data"; no Scryfall calls in engine).

## Server + CLI
10. **server: tokio + WebSocket skeleton** — layer 1 accepts connections; ADR for
    dependency additions.
11. **server: room task** — one task per room, owns one engine instance,
    broadcasts personalized GameViews.
12. **cli: interactive client** — numbered valid_actions, stdin choice loop
    against a local server (dev sequence step 3).
13. **cli: LLM agent mode** — GameView JSON in, action_id out, timeout fallback.

## Client (after protocol serde)
14. **client: WebSocket store** — Zustand store holding latest GameView; reconnect
    resends full state (test the reconstruct-from-one-GameView invariant).
15. **client: port the Pixi card factory from prototypes/ui-battlefield-v3.html**
    into src/, reading src/tokens.ts (reference only — reimplement, don't import).
16. **client: battlefield bands + hand + action bar** rendering GameView from the
    store; subject-owned action routing per ADR 0004.
