.PHONY: verify check engine-test engine-lint engine-fmt compat client-check client-lint client-install client-audit deny e2e e2e-install setup

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the required GitHub checks —
# `check` (Engine + Client), `deny` (cargo-deny), and `e2e` (the browser suite)
# — so there is a single command whose coverage matches CI.
# `make check` remains the fast inner loop and stays browser-free.
verify: check deny e2e ## Full pre-merge verification: Engine + Client + cargo-deny + browser suite (mirrors every required GitHub check)

check: engine-lint engine-test client-check client-audit ## Fast inner-loop gate: everything the Engine + Client CI jobs run (cargo-deny is separate — see `verify`)

engine-lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

engine-fmt:
	cargo fmt --all

# Regenerate the deterministic card-compatibility report (issue #258) from the
# catalog + data/exclusions.json. Commit the result; `make check` fails if it drifts.
compat:
	cargo run -q -p rune-engine --bin gen-compat

engine-test:
	cargo test --workspace

client-install:
	cd clients/web && npm ci

client-lint: client-install
	cd clients/web && npm run lint

# `npm run budget` measures the production `dist/` the preceding `npm run build`
# just produced — never dev-server output — against the load ceilings in
# docs/design/presentation-budgets.md, and fails the gate on a regression (#510).
client-check: client-install
	cd clients/web && npm run lint && npm run typecheck && npm run test && npm run build && npm run budget

# Fail the build on new high+ (high/critical) npm advisories in the client tree.
# Threshold and escape hatch (package.json "overrides") documented in clients/web/AGENTS.md.
client-audit: client-install
	cd clients/web && npm audit --audit-level=high

# Supply-chain gate (the `cargo-deny` CI job). Same subcommand + checks the
# deny.yml workflow runs, kept here so the command lives in exactly one place.
deny:
	cargo deny check advisories licenses bans sources

# The client install is a prerequisite (Playwright starts its dev server), so
# `make verify` runs `npm ci` in `clients/web` exactly once across both gates.
e2e-install: client-install
	cd clients/web/e2e && npm ci

# The browser suite (ADR 0011): the smoke canary (#279, two contexts) and the
# four-player vertical slice (#499, four contexts, run twice — once with reduced
# motion), both against the real `rune-server` binary. Deliberately OUT of
# `make check` — it needs a browser, a Vite server, and a compiled server — and
# in `make verify`, matching the separate `E2E` CI job. The e2e toolchain lives
# in its own package (`clients/web/e2e`) so it never lands in the fast gate's
# install. Playwright starts the dev server itself; this target only has to
# provide the server binary it launches.
#
# Chromium is not downloaded here: install it once with
# `cd clients/web/e2e && npx playwright install --with-deps chromium`
# (or point `PLAYWRIGHT_BROWSERS_PATH` at an existing install).
e2e: e2e-install
	cargo build -p rune-server
	cd clients/web/e2e && npm run typecheck && npm test

setup:
	scripts/bootstrap.sh
