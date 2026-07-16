# AGENTS.md — clients/web

The RUNE web client. A dumb renderer by design — read docs/brief.md (Component 2)
and docs/design/ui-design-notes.md before changing anything.

## Hard rules
- `valid_actions[]` drives ALL interactivity. Nothing outside it is clickable,
  focusable, or hoverable-as-actionable. The client never computes legality.
- Actions have subjects: entity-subject actions render on the entity; the action bar
  holds only global actions plus a contextual echo of the selection. Never enumerate
  per-card actions as bar buttons (docs/decisions/0004).
- DOM/canvas split (docs/decisions/0003): battlefield, hand, and stack cards live in
  the Pixi canvas; prompts, action bar, player tiles, log, browsers, and inspect are
  React DOM. Text a user reads or clicks is DOM.
- All card colors/sizes come from `src/tokens.ts`. Both renderers (Pixi + HTML)
  read the same constants; never inline card colors.
- The whole UI must rebuild from a single GameView + pending prompt (reconnect test).
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

Formatting is owned by Prettier (`.prettierrc.json`); don't hand-format. See
`docs/coding-standards.md` for the project-wide standard.

## Dependency audit policy
- CI and `make check` run `npm audit --audit-level=high` (the `client-audit`
  Make target / the "Audit" CI step). A new **high or critical** advisory fails
  the build; moderate/low are reported but non-blocking.
- Why `high`: the client's toolchain (vite, vitest, esbuild) is dev/build-time
  only — none ship in the runtime bundle. Gating at `high` blocks the serious,
  actionable advisories without CI churn from the frequent moderate advisories
  in build tooling. The tree is currently clean at every level (0 findings).
- The lockfile stays committed; CI installs with `npm ci`. When a dependency
  fix changes `package.json`, run `npm install` and commit the updated
  `package-lock.json` so `npm ci` stays reproducible.
- Escape hatch for an accepted/false-positive high+ finding: pin the transitive
  package via a `"overrides"` entry in `package.json` (preferred, deterministic),
  and note the advisory + justification in the PR. Do not silence with
  `--audit-level` bumps.

## References
- prototypes/ui-battlefield-v3.html — working reference for card factory, bands,
  zone rail, browser, inspect. Reference only; never import from prototypes/.
- docs/design/ui-requirements.md — full capability list; check the stress analysis
  section before locking any rendering decision.
