.PHONY: verify check engine-test engine-lint engine-fmt client-check client-lint client-install client-audit deny smoke setup

# Never let installing the client's dev deps auto-download a browser: `@playwright/test`
# is only needed by the `smoke` target, which supplies its own Chromium (pre-installed
# locally, or `playwright install`ed by the CI job). This keeps `make check`/`client-install`
# fast and browser-free. Explicit `playwright install` ignores this flag, so the smoke
# job can still fetch a browser.
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD := 1

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the required GitHub checks —
# `check` (Engine + Client), `deny` (cargo-deny), and the browser `smoke` canary
# (ADR 0011) — so there is a single command whose coverage matches CI. `make check`
# remains the fast inner loop (browser-free).
verify: check deny smoke ## Full pre-merge verification: Engine + Client + cargo-deny + browser smoke (mirrors every required GitHub check)

check: engine-lint engine-test client-check client-audit ## Fast inner-loop gate: everything the Engine + Client CI jobs run (cargo-deny is separate — see `verify`)

engine-lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

engine-fmt:
	cargo fmt --all

engine-test:
	cargo test --workspace

client-install:
	cd clients/web && npm ci

client-lint: client-install
	cd clients/web && npm run lint

client-check: client-install
	cd clients/web && npm run lint && npm run typecheck && npm run test && npm run build

# Fail the build on new high+ (high/critical) npm advisories in the client tree.
# Threshold and escape hatch (package.json "overrides") documented in clients/web/AGENTS.md.
client-audit: client-install
	cd clients/web && npm audit --audit-level=high

# Supply-chain gate (the `cargo-deny` CI job). Same subcommand + checks the
# deny.yml workflow runs, kept here so the command lives in exactly one place.
deny:
	cargo deny check advisories licenses bans sources

# Browser smoke canary (ADR 0011, issue #279): one Playwright spec drives two real
# browser contexts against a real, seeded `rune-server` on an ephemeral port, plays
# real turns through the rendered UI, and guards the StrictMode canvas-attach fix
# (#276). Deliberately part of `make verify` but NOT `make check` — it needs a built
# server binary, a dev server, and a browser, which the fast unit gate must stay free
# of. Prebuilds the server so the spec starts fast. Uses the pre-installed Chromium at
# /opt/pw-browsers when present (via `RUNE_PW_EXECUTABLE`); otherwise Playwright's own
# managed Chromium (which the CI job installs).
smoke: client-install
	cargo build -p rune-server
	cd clients/web && RUNE_PW_EXECUTABLE="$(wildcard /opt/pw-browsers/chromium)" \
		npx playwright test --config e2e/playwright.config.ts

setup:
	scripts/bootstrap.sh
