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
- DOM/canvas split (ADR 0003): Pixi renders cards and table visuals; React DOM renders
  controls, prompts, player information, browsers, and inspect surfaces.
- All card colors/sizes come from `src/tokens.ts`. Both renderers (Pixi + HTML)
  read the same constants; never inline card colors.
- The whole in-game UI must rebuild from one `GameView` and its pending prompt.
- Effective values (P/T, counters) are displayed exactly as the server computes them.
- No `localStorage` of game state; server is the source of truth.
- Touch first: 44px minimum targets; no action reachable only by drag or hover.

## Commands

- `npm install` (in this directory)
- `npm run lint` — ESLint (flat config) + Prettier `--check`; CI runs this
- `npm run lint:fix` — auto-fix ESLint + write Prettier formatting
- `npm run typecheck` — strict TS
- `npm run build` — typecheck + production build (CI runs this)
- `npm run dev` — Vite dev server

Use Prettier for formatting; see [`docs/coding-standards.md`](../../docs/coding-standards.md).

## Dependencies

- Commit `package-lock.json`; CI installs with `npm ci`.
- `npm audit --audit-level=high` fails on high or critical advisories.
- Prefer a deterministic `package.json` override for an accepted transitive advisory and
  explain it in the PR. Do not raise the audit threshold to silence a finding.

## References

- [`docs/design/ui-requirements.md`](../../docs/design/ui-requirements.md) — product
  capabilities the UI must eventually represent.
- `prototypes/ui-battlefield-v3.html` — historical visual reference only; never import it.
