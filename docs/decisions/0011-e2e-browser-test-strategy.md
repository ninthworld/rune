# ADR 0011: Browser-level test strategy

- Status: accepted; implementation deferred
- Date: 2026-07-11
- Issue: #102

## Context

Unit and component tests can verify normalized views, React controls, and the pure
`GameView -> TableScene` mapping, but they cannot prove that a production bundle connects to
a server and renders correctly in a real WebGL browser. Browser coverage is heavier than the
fast development gate and needs a stable boundary to avoid brittle tests.

## Decision

### Runner and application

Use Playwright with a pinned Chromium version. Test a production Vite build served through
`vite preview`, not the development server.

### Test tiers

Use two complementary backends:

1. A lightweight mock WebSocket server replays shared `GameView` fixtures and validates
   returned actions. This is the default tier for rendering, prompts, targeting, and input.
2. A small smoke tier launches the real `rune-server` and drives the actual lobby and game
   protocol. Keep these flows coarse and few.

Fixtures must be shared with the existing protocol and component tests rather than maintained
as a separate wire model.

### Canvas assertions

Primary canvas assertions inspect the pure `TableScene` value consumed by Pixi. A test-only,
read-only `window.__RUNE_TEST__` hook may expose that derived scene in test builds. It must not
control state, compute rules, or ship as a production behavior path.

Use screenshot baselines only for a small set of pixel-level properties that scene assertions
cannot cover, such as draw order, colors, or targeting rings. Structured assertions remain the
default because they are more precise and less sensitive to GPU and font differences.

### Layout and CI

Browser tests, configuration, mock transport, and any screenshot baselines live under
`clients/web/e2e/`, separate from Vitest tests.

When restored, the browser suite runs through a dedicated Make target and required CI job,
outside `make check`. The complete pre-merge command must then include that target, and
`AGENTS.md`, `docs/coding-standards.md`, the Makefile, workflow names, and repository ruleset
must be updated together so “done” remains unambiguous.

## Consequences

The strategy covers the real browser and transport seams while keeping most cases deterministic
and fixture-driven. It adds browser installation, process orchestration, trace artifacts, and a
potential flake surface. The test hook and screenshot set require strict scope and maintenance.

This ADR is the blueprint for a future full browser suite. The current repository has no
dedicated Playwright suite, Make target, or CI job.
