.PHONY: verify check engine-test engine-lint engine-fmt client-check client-lint client-install client-audit e2e e2e-browser deny setup

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the required GitHub checks —
# `check` (Engine + Client), `e2e` (E2E), and `deny` (cargo-deny) — so there is a single
# command whose coverage matches CI. `make check` remains the fast inner loop.
verify: check e2e deny ## Full pre-merge verification: Engine + Client + E2E + cargo-deny (mirrors every required GitHub check)

check: engine-lint engine-test client-check client-audit ## Fast inner-loop gate: everything the Engine + Client CI jobs run (browser e2e and cargo-deny are separate — see `verify`)

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

# Browser end-to-end suite (ADR 0011). Deliberately OUTSIDE `make check`: it needs
# a real browser and a built-and-served client, so it runs as its own target and
# its own CI job to keep the fast unit gate browser-free. Drives the preinstalled
# Chromium and never downloads a browser. Builds `rune-server` first because the
# real-server smoke tier (issue #144) launches the actual binary.
e2e: client-install
	cargo build -p rune-server
	cd clients/web && npm run e2e:typecheck && npm run e2e

# Provision the pinned Chromium (+OS deps) for the browser suite. Kept separate from
# `e2e` so local runs against a preinstalled Chromium never trigger a download; CI and
# `bootstrap.sh` use it to install the browser. `make e2e-browser e2e` shares one
# `npm ci` because both resolve the same `client-install` prerequisite.
e2e-browser: client-install
	cd clients/web && npx playwright install --with-deps chromium

# Supply-chain gate (the `cargo-deny` CI job). Same subcommand + checks the
# deny.yml workflow runs, kept here so the command lives in exactly one place.
deny:
	cargo deny check advisories licenses bans sources

setup:
	scripts/bootstrap.sh
