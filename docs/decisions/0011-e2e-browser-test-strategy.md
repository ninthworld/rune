# ADR 0011: End-to-end browser test strategy for the web client

- Status: accepted (suite paused — see note)
- Date: 2026-07-11
- Issue: #102

> **Note (paused):** the browser E2E suite, its `E2E` CI job, and the `make e2e` targets
> were removed to keep the inner loop fast while the in-game UI is still in flux. This ADR
> is retained as the blueprint for reinstating them later; the strategy below is unchanged.

## Context

The web client is the least-tested layer of RUNE, and structurally the hardest to
test. Vitest + React Testing Library run in CI (the `Client` job / `make check`),
but they exercise only the pure, headless pieces: the `GameView` store, the
`buildTableScene` mapping (`clients/web/src/table/scene.ts`), and DOM components.
The **Pixi canvas — the surface that actually draws the battlefield, hand, and
stack cards (ADR 0003) — is entirely untested.** It needs a WebGL context, so
under jsdom it no-ops: `SceneReconciler` (`clients/web/src/table/sceneReconciler.ts`)
builds and mutates a Pixi display tree that no headless test can render or inspect.
Nothing verifies that a real build, served in a real browser, connects to a socket
and paints the correct cards.

The roadmap makes this urgent, not academic. M1's outcome ("take a seat") has as a
hard exit criterion "a Playwright e2e test drives real Chromium from connection
screen → … → first GameView rendered on the battlefield, and runs in CI," and the
M1 feature table lists **this ADR (#102) as the unblocker for the Playwright
harness (#104)** and the connection screen (#103). The roadmap's own baseline notes
the gap: the store's `connect()` is never called by production code, so the app
shows "Waiting for game state…" forever, and "there is no browser/e2e harness and
the Pixi canvas no-ops headless, so canvas rendering is untested." M1 is a
client/server/protocol-heavy milestone whose whole point is the seams between those
three; the bugs it will produce are integration bugs a unit test cannot see. This
is the moment to settle *how* we test the real client in a real browser, before the
harness and the connection/lobby UI land on top of an undecided approach.

The forces that constrain the decision are the project's hard rules and existing
architecture:

- **The DOM/canvas split makes the canvas a pure visual surface (ADR 0003).** The
  Pixi layer "stays a pure performance surface"; it holds no logic and reads no
  state the scene does not hand it. Everything a user *reads or clicks that is not
  a card* is React DOM floating above the canvas. This is what makes canvas testing
  tractable: the DOM half is assertable with ordinary selectors, and the canvas
  half is a deterministic function of scene data we already compute in pure code.
- **The scene is already pure and headless by design.** `buildTableScene` is a pure
  `GameView → TableScene` function, explicitly written to be "unit-testable without
  a WebGL context," and `SceneReconciler` guarantees that after applying any scene
  "the tree is identical to what a freshly constructed reconciler would produce from
  that scene alone." The rendered result is therefore fully determined by the
  `TableScene` value — a plain data structure (bands, hand, per-card `rect`, `data`,
  `tier`, `targetable`) with no WebGL in it. Asserting on that value asserts on what
  the canvas draws, without reading pixels.
- **The whole UI must rebuild from one `GameView` + pending prompt** (`AGENTS.md`,
  `clients/web/AGENTS.md`). Client state is never load-bearing across messages, so a
  browser test can drive the client purely by feeding it `GameView` frames over a
  socket and asserting the resulting DOM + scene — no privileged test seam into
  client logic is needed beyond reading the derived scene.
- **`make check` is the CI contract.** `docs/coding-standards.md` states it plainly:
  "`make check` … is exactly what CI runs — if it isn't green, it isn't done." A
  browser suite that needs a downloaded Chromium, a Vite server, and (for smoke
  tests) the `rune-server` binary is heavier and flakier than the fast unit gate.
  Where it sits relative to `make check` is itself a decision with a governance
  consequence, not a detail.

## Decision

RUNE adopts a single browser-level end-to-end test strategy for the web client.
The rules below are what the codebase and CI will follow; the harness itself is out
of scope for this ADR (it lands in #104) — this decision is what #104 builds to.

### Runner: Playwright driving the preinstalled Chromium

E2E tests are written with **Playwright**, driving the **Chromium that is already
installed in the toolchain**. Playwright is the current standard for browser
automation, gives real WebGL (so the Pixi canvas genuinely renders), has
first-class screenshot/visual-comparison support, auto-waiting that suppresses a
large class of timing flakes, and a trace viewer for debugging CI failures. We pin
Chromium and do not test a browser matrix: the client targets evergreen browsers
and the value here is integration coverage, not cross-browser compatibility. Using
the preinstalled Chromium avoids a per-run browser download in CI.

### What runs under test: Vite preview + two socket backends

Tests run against the **real production client**: `vite build` followed by
`vite preview` (not the dev server), so the artifact under test is the same bundle
CI ships. The client is driven entirely through its WebSocket — consistent with the
"rebuild from one `GameView`" invariant — against **two** backends chosen per test
tier:

1. **A mock WebSocket server replaying fixture `GameView`s — the default, for fast
   tests.** A tiny in-process WS server accepts the client's connection and pushes
   canned `GameView` frames (and, for input tests, validates the `ChooseAction` the
   client echoes back). This tier is deterministic and fast: no engine, no game, no
   room lifecycle — just "given this exact `GameView`, the browser paints this." It
   is where the bulk of rendering, targeting-mode, and DOM/canvas assertions live.
2. **The real `rune-server` binary — for a small number of smoke tests.** A handful
   of tests launch the actual `rune-server` and connect the real client to it, to
   prove the true end-to-end path (build → browser → socket → server) works against
   the real protocol implementation, not just a mock that could drift from it. These
   are the M1 exit-criterion tests (connection screen → … → first GameView on the
   battlefield). They are slower and are kept few and coarse.

The mock tier catches rendering regressions cheaply; the smoke tier catches
mock-vs-reality drift. Neither tier puts game logic in the test: fixtures and the
real server are the only sources of `GameView`s.

### Canvas assertion strategy: expose the pure `TableScene`, plus optional baselines

Because the canvas is a pure visual surface (ADR 0003) whose output is fully
determined by the `TableScene` data structure the client already computes, the
**primary** canvas assertion is on that data, not on pixels:

- **Expose the pure scene on a test-only `window` hook.** In test/preview builds
  only, the client publishes the current `TableScene` (the value produced by
  `buildTableScene` and consumed by `SceneReconciler`) on a namespaced `window` hook
  (e.g. `window.__RUNE_TEST__.scene`). A Playwright test reads it via `page.evaluate`
  and asserts on structured facts — "Grizzly Bears is in the local band at this
  `rect`, tapped, with two +1/+1 counters," "in targeting mode exactly these entity
  ids are `targetable` and the rest are dimmed." This gives precise, stable,
  human-readable canvas coverage with no image diffing. The hook exposes **derived
  render data only** and is strictly read-only and test-build-gated: it is not a
  control channel and adds no logic to production code paths, so it does not violate
  "zero game logic in the client" (the scene is already computed; the hook only reads
  it) nor the "rebuild from one `GameView`" invariant.
- **Optional screenshot baselines for the pixels themselves.** For a *small*,
  deliberately chosen set of representative frames, we additionally keep Playwright
  screenshot baselines (`toHaveScreenshot`) to catch regressions the scene data
  cannot describe — actual draw output, z-order, colors from `tokens.ts`, targeting
  rings. These are **opt-in and secondary**: pixel baselines are inherently brittle
  across renderer/font/GPU differences, so they are used sparingly, pinned to the
  same Chromium, and never the sole assertion for a behavior that scene-data
  assertions can express. The scene hook is the workhorse; baselines are a backstop.

### Directory layout and fixture strategy

- E2E tests, their Playwright config, the mock WS server, and screenshot baselines
  live under **`clients/web/e2e/`**, separate from the co-located Vitest unit tests.
  This keeps the two suites — and their very different runtimes — cleanly divided.
- Fixtures are **reused, not reinvented.** The mock WS server replays the existing
  `GameView` fixtures in **`clients/web/src/game-view.fixture.ts`**
  (`SAMPLE_GAME_VIEW_JSON`, `TARGETING_GAME_VIEW_JSON`, …), which already mirror the
  Rust round-trip fixture in `crates/rune-protocol`. One fixture set backs unit
  tests, mock-WS e2e tests, and (by construction) matches what the real server
  emits, so the three tiers cannot silently disagree about the wire shape. New e2e
  scenarios add fixtures to that module rather than defining parallel ones.

### CI placement: a separate `E2E` job / `make e2e`, outside `make check`

The browser suite runs as its **own `make e2e` target and its own CI job (`E2E`)**,
**not** inside `make check` and not inside the existing `Engine`/`Client` jobs. It
needs a browser, a built-and-served client, and (for smoke tests) the `rune-server`
binary; folding that into the fast `make check` gate would make the everyday
inner-loop check slow and browser-dependent, and would couple the pure unit gate to
a heavier, flakier runtime.

**This is a deliberate change to the "`make check` = CI" equation** that
`docs/coding-standards.md` states, and it is called out for human sign-off in the
Consequences below. After this ADR, "CI" is `make check` (`Engine` + `Client`)
**plus** a separate `E2E` job; `make check` alone is no longer the complete CI
surface. The `E2E` job is a required check for the M1 exit criterion but is
configured and governed separately from the unit gate (e.g. it may run on a
different trigger cadence if flake or runtime warrants, without weakening
`make check`).

## Consequences

- **Easier.** The canvas becomes testable for the first time: the DOM/canvas split
  (ADR 0003) plus the already-pure scene pipeline means we assert on real rendered
  structure without decoding pixels. M1's Playwright exit criterion (#104) has a
  decided foundation to build on instead of re-litigating runner, backend, and
  canvas-assertion approach mid-implementation. Reusing `game-view.fixture.ts`
  across unit, mock-WS, and smoke tiers keeps one source of truth for the wire shape,
  so the tiers can't drift. The mock tier gives fast, deterministic rendering
  coverage; the `rune-server` smoke tier proves the real path and guards against
  mock-vs-reality drift.
- **Harder / given up.** A new, heavier test runtime enters the repo: a browser, a
  Vite preview server, the server binary, and Playwright's toolchain and flake
  surface. The client gains a **test-only `window` hook**, which must stay strictly
  read-only, namespaced, and gated to test/preview builds so it never becomes a
  logic path or ships in production. Optional screenshot baselines carry maintenance
  cost (re-baselining on legitimate visual changes, pinned Chromium) and are
  therefore kept few. Installing Playwright/Chromium and writing the tests are
  explicitly **out of scope** here (#104).
- **Governance — needs human sign-off.** Pulling e2e out of `make check` **breaks the
  "`make check` = CI" invariant** that `docs/coding-standards.md` and `AGENTS.md`
  lean on ("`make check` — everything CI runs"). After this ADR the full CI surface
  is `make check` **plus** the separate `E2E` job, and a green `make check` no longer
  means "everything CI runs" passed. Because that invariant is load-bearing for how
  agents reason about "done," this placement decision is **flagged for explicit human
  sign-off** as part of accepting this ADR, and the wording in
  `docs/coding-standards.md` / `AGENTS.md` describing `make check` as the complete CI
  surface should be reconciled when the `E2E` job and `make e2e` target actually land
  (#104), not in this docs-only change.
- **Deferred.** The Playwright harness, config, mock WS server implementation, the
  `make e2e` target, the `E2E` CI job, and the connection-screen wiring that lets a
  browser reach a first `GameView` are all follow-up work (#103, #104 and later M1
  items). Multi-client and full-lobby e2e flows (roadmap M1/M2) build on this same
  strategy once the single-client harness exists.
