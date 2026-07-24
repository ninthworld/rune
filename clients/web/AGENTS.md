# Web client agent guide

The web client is a renderer and input adapter. Read
[`docs/design/ui-design-notes.md`](../../docs/design/ui-design-notes.md) before changing
the table UI.

## Hard rules

- `valid_actions[]` drives ALL interactivity. Nothing outside it is clickable,
  focusable, or hoverable-as-actionable. The client never computes legality.
- Actions have subjects: entity-subject actions render on the entity; the action bar
  holds only global actions plus a contextual echo of the selection. Never enumerate
  per-card actions as bar buttons (docs/decisions/0004).
- Rendering split (ADR 0030): there is **one** rendering stack. Cards and the
  scene plane render in React DOM (`table/live/` stages via `plane/` + the DOM
  `card/dom/CardFace`, reconciled by `table/planeReconciler.ts`); no Pixi remains
  in the card/scene render path. Pixi survives only *outside* it — the passive
  effects overlay (`table/effects/`, `EffectsSurface`) and the ADR 0024 art
  texture cache (`card/art/artStore.ts` loads card art into a Pixi `Texture` the
  DOM renderer looks up). React DOM
  also owns controls, prompts, player information, browsers, and inspect surfaces.
  The legacy Pixi scene stack was fully retired: the ADR 0003 match table at the
  Phase 2 exit (#494), and the read-only spectate mode (`SpectatorTable`) at
  Phase 4 (#504) — it now rides the same `LivePlane` stack, staged receiver-less.
  `card/cardFactory.ts` is a **pure data model only** (`CardDisplayData`,
  `cardVisualSignature` — the live fold key — `parseManaCost`); it draws nothing.
- All card colors/sizes come from `src/tokens.ts`; the DOM card renderer reads the
  same constants — never inline card colors.
- The whole in-game UI must rebuild from one `GameView` and its pending prompt.
- Effective values (P/T, counters) are displayed exactly as the server computes them.
- No `localStorage` of game state; server is the source of truth. Device *preferences*
  (e.g. the art source) may persist; they must never be load-bearing for a view.
- Saved decks (ADR 0027) are an explicit carve-out from the rule above: the deck
  builder may persist player-authored decks device-local in IndexedDB
  (`src/deck/savedDeckStore.ts`), keyed by a player-chosen name, with a portable
  export/import JSON document. They are pre-game *input*, never a rendered view and
  never load-bearing across messages — the builder and every view must still rebuild
  fully with the deck store empty. Saving never implies legality: a saved deck is
  validated only at submission time by the room format through the unchanged
  `submit_deck` gate. Storage stays on the device (no server storage, no protocol
  change) and never leaves it until submitted; when storage is unavailable the
  builder degrades to the bundled starters. Overwriting or deleting a saved deck
  requires explicit intent (a confirm affordance).
- Card art (ADR 0024) is a client-local cache keyed by `functional_id` in
  `src/card/art/`: player-selected source, device-local storage, renderers only
  *look up* loaded textures. The UI must render fully with the art store empty, and
  nothing under `public/card-art/` may be anything but project-owned originals.
- Pregame (issue #506) lives in `src/pregame/`: the front door, lobby, and room
  are content compositions on **one** `PregameStage`, which mounts the scene
  environment once (`App.tsx`) so a place change never re-mounts the backdrop.
  All pregame color/shadow/duration flows from `sceneTokens.ts` through
  `pregame/pregameScene.ts` as `--pregame-*` properties — the `deckScene.ts`
  mold; no literal hex or duration belongs in `pregame.module.css`. Seat accents
  come from `SCENE_SEAT_ACCENTS[seat]`, the same index the match uses, so a
  seat's color survives the ready gate. `LobbyScreen.tsx` / `ConnectionScreen.tsx`
  are thin mount points over that composition. `screens.module.css` is now the
  deck-surface stylesheet only. See `docs/design/front-door-and-lobby.md`.
- Presentation settings (issue #505) — quality level, effect density, motion —
  are device preferences in `src/table/settings/presentationSettings.ts` (the same
  device-local, no-protocol idiom as art/decks), surfaced by
  `src/table/PresentationSettings.tsx` from the front door, the pregame session
  menu (lobby and room), and the in-match game menu.
  They only scale effects/environment/motion; the scene (plane, staging, cards,
  tap/travel motion) is never degraded at any level. See
  `docs/design/presentation-budgets.md` §Quality levels.
- Sound and haptics (issue #507) are hooks on the **same** effect taxonomy as the
  visual grammar, in `src/table/audio/`, subscribing to the presentation intents
  the scene already derives. They are optional, independently muted, and **never
  load-bearing**: no registered asset ⇒ complete silence and zero errors, nothing
  on the reconciler path is awaited, and no playback failure may reach the scene
  or input. Reduced motion never silences audio (independent channels); batch
  events collapse to one sound per batch window. Preferences live beside the #505
  ones in `src/table/settings/audioSettings.ts`. No audio asset ships — see
  ADR 0031, and put the first one under the `lazy/` prefix so `npm run budget`
  keeps passing.
- Touch first: 44px minimum targets; no action reachable only by drag or hover.

## Commands

- `npm install` (in this directory)
- `npm run lint` — ESLint (flat config) + Prettier `--check`; CI runs this
- `npm run lint:fix` — auto-fix ESLint + write Prettier formatting
- `npm run typecheck` — strict TS
- `npm run build` — typecheck + production build (CI runs this)
- `npm run budget` — load-budget gate on the built `dist/` (CI runs this after
  `build`); ceilings live in `scripts/loadBudget.js`, rationale in
  `docs/design/presentation-budgets.md` §Load and asset budgets
- `npm run dev` — Vite dev server

Use Prettier for formatting; see [`docs/coding-standards.md`](../../docs/coding-standards.md).

## Testing

- Co-located Vitest specs (`src/**/*.test.ts{,x}`) run in jsdom and cover everything
  headless: stores, wire shapes, pure scene/plane derivation, and DOM components.
- **`e2e/` is the browser suite** (ADR 0011) — its own npm package so the Playwright
  toolchain never enters the fast gate's install. Run it with `make e2e` (from the repository
  root); it is **not** part of `make check`, and rides `make verify` plus the `E2E` CI job.
  Two specs, and only two:
  - `smoke.spec.ts` — the canary (issue #279). Two contexts through front door → lobby →
    match, asserting what jsdom structurally cannot: a live, attached WebGL canvas and a
    populated scene plane.
  - `four-player.spec.ts` — the multiplayer vertical slice (issue #499). **One** scenario,
    four contexts, run twice (the second with `prefers-reduced-motion: reduce`): the full
    loop of a four-seat pod — mulligan, explicit main-phase priority, a land, a deliberate
    mana activation, a targeted spell on the stack and resolved, an attack declared against
    one of several legal defending players, blockers, damage/death, a disconnect/reconnect
    with live combat on the board, and the turn boundary. Both passes must reach the same
    decisions, the same final state, and the same pixels.
- Scenario helpers live in `e2e/support/`: `client.ts` (front door, lobby, room, the basics),
  `table.ts` (in-match gestures — mana, casting, attackers + defender, blockers), `drive.ts`
  (the bounded driver that makes the *travel* moves nothing asserts on), `render.ts` (canvas
  and plane probes), `shots.ts` (the screenshot leg), `hook.ts` (the read-only hook mirror).
  Extend them rather than writing a parallel set — the six-player coverage builds on these.
- Two rules bind the e2e suite as hard as the client itself: **no test-only production
  control path** (every move is a click on a rendered control — `window.__RUNE_TEST__` is
  read-only and is used only to decide *which* control to click), and **no client-side
  legality** (what is offered comes from the server's `valid_actions`). No sleeps: every
  wait is a wait on a condition.
- The canary runs against the **dev server**, not `vite preview`: the regression it guards
  is React StrictMode's development-only effect double-invoke.

## Dependencies

- Commit `package-lock.json`; CI installs with `npm ci`.
- `npm audit --audit-level=high` fails on high or critical advisories.
- Prefer a deterministic `package.json` override for an accepted transitive advisory and
  explain it in the PR. Do not raise the audit threshold to silence a finding.

## References

- [`docs/design/ui-requirements.md`](../../docs/design/ui-requirements.md) — product
  capabilities the UI must eventually represent.
- `prototypes/ui-battlefield-v3.html` — historical visual reference only; never import it.
