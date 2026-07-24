# Prototypes

Standalone HTML prototypes retained as historical visual references. Current requirements
live in [`docs/design/`](../docs/design/), and production behavior lives in `clients/web`.

- `ui-battlefield-v3.html` — Pixi card factory, battlefield bands, collapsed
  stacks, subject-owned actions, zone rail, graveyard browser, peek/pinned
  inspect. Open directly in a browser.
- `ui-2-5d-spike-v1.html` — the 2.5D presentation architecture spike (issue
  #467, ADR 0030): perspective battlefield plane, DOM cards with lift/tap
  tactility, curved hand fan, FLIP travel animations, passive canvas effects
  overlay, stress/reconnect/reduced-motion demos, and the `window.__spike`
  measurement API. Findings in
  [`docs/design/spike-2-5d-findings.md`](../docs/design/spike-2-5d-findings.md).
  Open directly in a browser; `?flat=1` disables perspective as a control.

Do not import code or assets from this directory into production.
