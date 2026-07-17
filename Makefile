.PHONY: verify check engine-test engine-lint engine-fmt compat client-check client-lint client-install client-audit deny setup

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the required GitHub checks —
# `check` (Engine + Client) and `deny` (cargo-deny) — so there is a single command whose
# coverage matches CI. `make check` remains the fast inner loop.
verify: check deny ## Full pre-merge verification: Engine + Client + cargo-deny (mirrors every required GitHub check)

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

setup:
	scripts/bootstrap.sh
