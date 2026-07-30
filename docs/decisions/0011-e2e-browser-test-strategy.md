# ADR 0011: End-to-end browser test strategy for the web client

- Status: accepted
- Date: 2026-07-11
- Issue: #102

## Context

The client is the least-tested layer of SAGE and structurally the hardest to test.
Component-level tests under a headless DOM exercise only the pure pieces — the `GameView`
store, view-model mapping, individual components. They cannot see the seams this project is
actually made of: a real build, served over HTTP, opening a real WebSocket, receiving real
`GameView` frames, and surviving a reconnect. Those seams are where the integration bugs are,
because that is where three independently-correct layers meet.

A browser suite is the only thing that covers them, and the cost of keeping one is the whole
problem: a browser gate that is slow, flaky, or awkward to run gets removed, and then the
coverage is gone exactly when the client is changing fastest. So the strategy has to be
designed for *survivability*, not just coverage.

Three specific costs have to be engineered against:

1. **CI duration.** A browser job every PR waits on is a tax on every change, including
   changes that cannot possibly break the client.
2. **Iteration time.** If the only way to see a result is a full install-build-serve-run
   cycle, the edit loop is dominated by waiting, and the suite stops being run locally at all.
3. **Playwright/Chromium version mismatch.** Playwright wants to download a browser matched to
   its version; the environment already has one. Left alone, this fails in CI or silently
   tests a different browser than the image provides.

The project's own rules also constrain the design. The client holds **zero game logic** and
the entire UI is reconstructable from **one `GameView` plus a pending prompt**, so client
state is never load-bearing across messages. That means a browser test can drive the client
entirely by feeding it view frames over a socket and asserting on what it renders — no
privileged test seam into client internals is required.

## Decision

SAGE keeps one browser-level end-to-end suite, designed to be cheap enough to stay.

### Runner: Playwright driving the preinstalled Chromium

Tests are written with **Playwright**, driving the **Chromium already installed in the
image**. Playwright is the standard for browser automation, has auto-waiting that suppresses
a large class of timing flakes, and ships a trace viewer for debugging CI failures. One
pinned browser, no matrix: the value here is integration coverage, not cross-browser
compatibility.

**Never run `playwright install`** — not in CI, not in an agent session. Consume the
preinstalled browser via `PLAYWRIGHT_BROWSERS_PATH` with `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`,
and pin an explicit `executablePath` when the pinned `@playwright/test` disagrees with the
image. This is the mitigation for cost 3 and is not optional.

### What runs under test: the production build, two socket backends

Tests run against the **real production build** — a built bundle served statically, not a dev
server — so the artifact under test is the one that ships. The client is driven entirely
through its WebSocket, consistent with the "rebuild from one `GameView`" invariant, against
two backends chosen per tier:

1. **A mock WebSocket server replaying fixture views — the default.** A tiny in-process server
   accepts the client's connection and pushes canned `GameView` frames, and for input tests
   validates the `ChooseAction` the client echoes back. Deterministic and fast: no engine, no
   game, no room lifecycle — just "given this exact view, the browser renders this." Most
   rendering and interaction assertions live here.
2. **The real `sage-server` binary — for a few smoke tests.** A small number of tests launch
   the actual server and connect the real client to it, proving the true path (build →
   browser → socket → server) works against the real protocol implementation rather than a
   mock that can drift from it.

The mock tier catches rendering regressions cheaply; the smoke tier catches mock-vs-reality
drift. Neither tier puts game logic in the test: fixtures and the real server are the only
sources of `GameView`s.

### Fixtures are shared, not reinvented

The mock server replays the same `GameView` fixtures the client's unit tests use, and those
mirror the round-trip fixture in `crates/sage-protocol`. One fixture set backs unit tests,
mock e2e, and — by construction — what the real server emits, so the tiers cannot silently
disagree about the wire shape. New scenarios add to that set rather than defining a parallel
one.

### CI placement: one blocking smoke path, deeper flows non-blocking

The browser suite runs as its **own `make e2e` target and its own CI job**, not inside
`make check`.

- **One thin smoke path is the required gate**: load the built client, reach a first rendered
  `GameView`, take an action. It is the blocking check, and it is kept small enough that its
  runtime is not an argument for deleting it.
- **Deeper flows run in a non-blocking job** — full lobby, reconnect, multi-client — so
  breadth never gates a merge on browser flake.
- **The engine gate stays independent.** `make check` must remain green-able without a
  browser, so engine-only work never waits on one (cost 2).
- The suite must be runnable against an already-built bundle without a full reinstall, for the
  same reason.

Consequently "CI" is `make check` plus a separate browser job; a green `make check` alone is
not the complete pre-merge surface. `docs/coding-standards.md`, `AGENTS.md`, and the required
status checks say so explicitly when the job lands.

## Consequences

- **Easier.** The paths that actually break — socket, protocol, view reconstruction,
  reconnect — get covered for the first time, against the real bundle. Sharing one fixture set
  across unit, mock, and smoke tiers keeps a single source of truth for the wire shape. The
  mock tier gives fast deterministic coverage; the smoke tier proves the real path.
- **Harder / given up.** A heavier test runtime enters the repo: a browser, a static server,
  the server binary, and Playwright's toolchain and flake surface. Two-tier CI placement is
  more configuration than one job. Pixel-level assertions are deliberately not part of this
  decision — screenshot baselines are brittle across renderer, font, and GPU differences, and
  if they are ever added they are a sparing backstop, never the sole assertion for a behavior
  that a structural assertion can express.
- **Bounded by design.** The blocking gate is one path on purpose. Breadth lives in the
  non-blocking job, and the trade is deliberate: a suite that always runs and covers one thing
  well is worth more than a thorough one that gets deleted.
