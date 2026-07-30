# ADR 0019: Chrome styling layer, tokens, and RUNE identity

- Status: accepted
- Date: 2026-07-17
- Issue: #293

## Context

The whole DOM chrome (everything outside the Pixi card renderer — HUDs, tray, rail,
prompts, overlays, lobby, connection screen) was styled through ~1,360 lines of
per-element inline `CSSProperties` objects in `clients/web/src/table/styles.ts`.
Inline styles have hard limits the design now needs past:

- **No state selectors.** `:hover`, `:focus-visible`, `:active`, `:disabled` cannot
  be expressed inline, so keyboard focus was invisible and controls had no press or
  hover feedback.
- **No media / container queries and no `prefers-reduced-motion`.** The reduced-motion
  requirement (ui-requirements §Accessibility) and later responsive/animation work
  have nowhere to live.
- **Ad-hoc values.** Every chrome color was a hex literal repeated per shape; one
  background/border/radius produced the "uniform panel" look
  `docs/design/ui-design-notes.md` (§Visual hierarchy) replaces with surface tiers,
  and typography was a single stack with no scale or display face.

`docs/design/ui-design-notes.md` (§Visual hierarchy, §Palette, §Identity) specifies
a chrome token set (surface tiers, border, elevation, spacing/radius scales, a
typography scale with a display face) and restrained RUNE identity. We must deliver
that system and migrate existing chrome onto it **without redesigning** any
component (per-region redesigns are follow-up issues), so later diffs stay clean.

The card tokens in `src/tokens.ts` (color/size constants the Pixi factory and the
HTML card component share) are out of scope and must stay untouched.

### Options for the styling layer

Requirements the choice must satisfy: real state selectors; media/container queries;
static extraction or negligible runtime; works with the existing Vite build; no new
high-risk dependency (the client audit fails on high/critical advisories).

1. **Plain global CSS + custom properties.** Zero deps, zero runtime, full selector
   power. But global class names have no build-time scoping — collisions and dead
   rules become a manual discipline problem across a growing component tree.
2. **CSS Modules.** Built into Vite/Vitest with **no new dependency**. Class names are
   locally scoped and hashed at build time; styles are statically extracted to a
   `.css` file (zero runtime). Full selector power, media/container queries,
   `prefers-reduced-motion`. Authoring is plain CSS reading `var(--…)` tokens.
3. **vanilla-extract.** Type-safe styles authored in TypeScript with static
   extraction — powerful, but adds a Vite plugin + toolchain dependency (new audit
   surface) and a second way to author styles, for benefits (typed token access) a
   custom-property token file already delivers.

## Decision

Adopt **CSS Modules** as the chrome styling layer, driven by a **custom-property
token set** and a small global base layer.

- **Tokens** live in `src/chrome/tokens.css` as `:root` custom properties — the
  single source of truth for chrome surfaces, borders, elevation, the spacing and
  radius scales, the typography scale, accents, and one motion token. They sit
  **alongside** the card tokens in `src/tokens.ts` and never entangle with them; the
  few values that must agree with a card token (board color, selection/targeting
  accents, text) are duplicated as their own chrome tokens so each system changes
  independently. **No chrome hex literal exists outside this file.**
- **Component styles** live in co-located `*.module.css` (chrome for the whole app is
  `src/table/chrome.module.css`), read only tokens via `var(--…)`, and carry the
  real state selectors inline styles could not: every button/control has visible
  `:hover`, `:focus-visible`, `:active`, and `:disabled` treatments from this shared
  layer. Class names mirror the old style-object names so the migration is
  mechanical (`style={s.button}` → `className={s.button}`).
- **The base layer** `src/chrome/base.css` owns global concerns: document defaults and
  the **single `prefers-reduced-motion` mechanism** (a global `@media (reduce)` block
  neutralizing animation/transition durations) that all later animation work reuses.
- **Canvas-anchored geometry stays inline.** `styles.ts` is reduced to geometry-only
  helpers — functions that position a DOM element over the Pixi scene from a runtime
  rect (coordinates that only exist at render time, so they cannot be static
  classes). Their colors read tokens (`var(--…)`, or the shared `SURFACES` card token
  for the selection/target ring). The interactive canvas buttons additionally take a
  `.canvasControl` class for the shared focus/hover treatment.

### Display face and RUNE identity

Body text stays a legible system stack (`--rune-font-body`). Identity moments — the
RUNE wordmark, victory/defeat, and phase names (§Identity) — use a **display face**
token, `--rune-font-display`. We **do not bundle a font binary**: a well-licensed OFL
face is acceptable per the issue, but a bundled binary is a vendored asset to vet on
every audit plus a FOUT, whereas the lowest-risk "distinctive" option is a geometric
**system** stack (`'Avenir Next', 'Century Gothic', 'Futura', system-ui`). This token
is the single swap point — dropping in an OFL `@font-face` later changes one line and
no call sites. This keeps us clear of any card-image/frame/WotC-branding concern
(AGENTS.md hard rule): the identity is type and procedural geometry only.

## Consequences

- Chrome now expresses hover/focus/active/disabled, media/container queries, and
  reduced motion; keyboard focus is visible everywhere. Styles are statically
  extracted to one hashed `.css` file with no runtime cost, on the existing Vite
  build and with no new dependency.
- Chrome values are tokens; the "uniform panel" look can become real surface tiers by
  editing `tokens.css`, and a future OFL display face is a one-line swap.
- The migration was mechanical: same layout, same look within tolerance, DOM
  semantics and every `data-testid` preserved (all 255 client tests pass unchanged).
- Cost: styling chrome now means editing a `.module.css` plus (for new values) a
  token, rather than a TS object — two files instead of one. Combining a base class
  with modifiers uses a small `cx()` join helper instead of object spread.
- Card tokens (`src/tokens.ts`) and the Pixi renderer are untouched; the two systems
  stay decoupled.
