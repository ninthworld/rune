.PHONY: verify check client-check client-install e2e e2e-smoke e2e-views engine-test engine-lint engine-fmt compat deny setup

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the required GitHub checks, so
# there is a single command whose coverage matches CI. `make check` remains the
# fast inner loop.
#
# `e2e-smoke` is here because it is a required check. The broader `e2e-views`
# tier is not: ADR 0011 keeps breadth non-blocking so a merge never waits on
# browser flake, and `make check` stays green-able without a browser at all.
verify: check client-check e2e-smoke deny ## Full pre-merge verification: Engine + Client + E2E smoke + cargo-deny

# Engine only, and deliberately so: this is the loop an engine change runs on a
# hundred times, and it must not require node to be installed.
check: engine-lint engine-test ## Fast inner-loop gate: everything the Engine CI job runs (see `verify` for the rest)

engine-lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

engine-fmt:
	cargo fmt --all

# Regenerate the deterministic card-compatibility report (issue #258) from the
# catalog + data/exclusions.json. Commit the result; `make check` fails if it drifts.
compat:
	cargo run -q -p sage-engine --bin gen-compat

engine-test:
	cargo test --workspace

# Everything the Client CI job runs. `npm ci` needs a lockfile and installs
# exactly what it pins, so a local run and CI resolve identical trees.
client-install:
	cd clients/web && npm ci

# The protocol mirror's parity test reads the fixtures under
# crates/sage-protocol/fixtures/, the same files the Rust tests pin — so this
# target fails when the wire types changed and the mirror did not.
client-check: client-install
	cd clients/web && npm run format:check && npm run lint && npm run typecheck && npm test && npm run build

# Browser end-to-end (ADR 0011). Never runs `playwright install`: the browser is
# whatever the image already provides, resolved in playwright.config.ts.
#
# `e2e-smoke` is the blocking gate — one thin path against the real server, kept
# small enough that its runtime is never an argument for deleting it.
# `e2e-views` is the broad, non-blocking tier: committed fixtures replayed over
# an intercepted socket, no server involved.
e2e-smoke: client-install
	cd clients/web && npm run build && npm run e2e:smoke

e2e-views: client-install
	cd clients/web && npm run build && npm run e2e:views

e2e: e2e-views e2e-smoke ## Both browser tiers

# Supply-chain gate (the `cargo-deny` CI job). Same subcommand + checks the
# deny.yml workflow runs, kept here so the command lives in exactly one place.
deny:
	cargo deny check advisories licenses bans sources

setup:
	scripts/bootstrap.sh
