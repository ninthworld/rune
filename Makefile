.PHONY: check engine-test engine-lint engine-fmt client-check client-lint client-install client-audit e2e setup

check: engine-lint engine-test client-check client-audit ## Everything the Engine + Client CI jobs run (browser e2e is the separate `e2e` target / E2E job — see ADR 0011)

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

setup:
	scripts/bootstrap.sh
