# Phase 1 2.5D fixture battlefield

Issue #483's integration harness is isolated from the shipped play flow at:

```text
/fixtures/2.5d
```

Vite development enables the route automatically. A production preview must
opt in with `VITE_RUNE_FIXTURE_HARNESS=true`; a normal production build routes
the path through the ordinary app and cannot enter the harness.

## Capture matrix

The selector covers the full layout-model set. Each scenario also has a stable
capture URL, so browser automation can take deterministic side-by-side images:

| Scenario | Capture query | Compare against |
| --- | --- | --- |
| Commander, 4 players | `?scenario=commander4&frame=0&capture=1` | `docs/ui-concepts/rune-2.5d-interface-baseline.jpg`, `layouts-v1/layout-commander4-v1.jpg` |
| Duel | `?scenario=duel&capture=1` | `layouts-v1/layout-duel-v1.jpg` |
| Six players | `?scenario=six&capture=1` | `layouts-v1/layout-six-v1.jpg` |
| Token wall | `?scenario=tokens&capture=1` | `layouts-v1/layout-tokens-v1.jpg` |
| Big hand | `?scenario=big-hand&capture=1` | `layouts-v1/layout-bighand-v1.jpg` |
| Combat web | `?scenario=combat-web&capture=1` | `layouts-v1/layout-combat-v1.jpg` |
| Deep stack | `?scenario=deep-stack&capture=1` | `layouts-v1/layout-stackweb-v1.jpg` |
| Phone portrait | `?scenario=phone&capture=1` | `layouts-v1/layout-phone-v1.jpg` |

`quality=high|standard|lite` selects the effect budget for a capture. The
Commander sequence additionally accepts `frame=0..6`.

## Automation and budgets

The route publishes `window.__RUNE_2_5D_FIXTURE__` after mounting. It exposes
scenario/frame selection, play/pause, reconnect-style rebuild measurement, and
the live budget report. The report measures idle/tween RAF rate and p95 frame
time, rebuild time, and document node count against
`docs/design/presentation-budgets.md`.

The same frame summarizer and limits run under Vitest with controlled
timestamps. `scenarios.test.ts` rebuilds every scenario through the real
`PlaneReconciler` + `CardFace` stack and enforces the reconnect and DOM
ceilings without requiring WebGL.
