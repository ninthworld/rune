.PHONY: verify check client-check client-install e2e e2e-browser e2e-smoke e2e-views e2e-scenario scenario engine-test engine-lint engine-fmt compat deny setup

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

# Regenerate the two deterministic reports: card compatibility (issue #258) from the
# catalog + data/exclusions.json, and the catalog test-coverage audit (issue #774) from
# the catalog + the engine's own sources. Commit both; `make check` fails on drift in
# either. The coverage report gates only its own freshness — never its contents.
compat:
	cargo run -q -p sage-engine --bin gen-compat
	cargo run -q -p sage-engine --bin gen-coverage

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

# Browser end-to-end (ADR 0011).
#
# `e2e-smoke` is the blocking gate — one thin path against the real server, kept
# small enough that its runtime is never an argument for deleting it.
# `e2e-views` is the broad, non-blocking tier: committed fixtures replayed over
# an intercepted socket, no server involved.

# The one browser every environment runs, and the whole of how the three stay
# identical: `playwright install` reads the revision out of the *installed*
# `playwright-core` and fetches exactly that, so the exactly-pinned
# `@playwright/test` in package.json decides the browser and nothing else does.
#
# It is idempotent and it is a no-op wherever the browser is already present —
# which includes the official CI container, whose browsers sit at
# `$PLAYWRIGHT_BROWSERS_PATH` at the revision that same pin names. So CI still
# downloads nothing, and a laptop or an agent sandbox gets the identical build
# once and caches it.
#
# Never run `playwright install` *unpinned* (`npx playwright@latest`, a caret
# range, a hand-picked revision): that is what fetches a browser the driver did
# not ask for, and it is the failure ADR 0011 names.
e2e-browser: client-install
	cd clients/web && npx playwright install chromium

e2e-smoke: e2e-browser
	cd clients/web && npm run build && npm run e2e:smoke

e2e-views: e2e-browser
	cd clients/web && npm run build && npm run e2e:views

# The scenario tier (issue #777): the contributor tool driven through the real
# client. Non-blocking like `e2e-views`, and separate from it because it has a real
# engine behind it and so needs a Rust toolchain. It builds the client because it
# serves the built bundle, exactly as the other two tiers do.
e2e-scenario: e2e-browser
	cd clients/web && npm run build && npm run e2e:scenario

e2e: e2e-views e2e-smoke ## Both blocking-and-broad browser tiers

# Open the shipping client on a scenario and print the URL (issue #777). Development
# only, disposable, and loopback only. `SCENARIO=` picks the file.
SCENARIO ?= scenarios/murder-the-dreadmaw.toml
scenario: ## Play an exact position in the real client: make scenario SCENARIO=path/to.toml
	cd clients/web && npm run build
	cargo run -q -p sage-scenario -- $(SCENARIO)

# Supply-chain gate (the `cargo-deny` CI job). Same subcommand + checks the
# deny.yml workflow runs, kept here so the command lives in exactly one place.
deny:
	cargo deny check advisories licenses bans sources

setup:
	scripts/bootstrap.sh
