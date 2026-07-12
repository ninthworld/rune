.PHONY: verify check engine-test engine-lint engine-fmt client-check client-lint client-install client-audit runner-test ci-policy-test ai-review-test ci-lint e2e e2e-browser deny setup

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the four required GitHub checks —
# `check` (Engine + Client), `e2e` (E2E), and `deny` + `ci-lint` (cargo-deny) — so there
# is a single command whose coverage matches CI. `make check` remains the fast inner loop.
verify: check e2e deny ci-lint ## Full pre-merge verification: Engine + Client + E2E + cargo-deny (mirrors every required GitHub check)

check: engine-lint engine-test client-check client-audit runner-test ci-policy-test ai-review-test ## Fast inner-loop gate: everything the Engine + Client CI jobs run (browser e2e and cargo-deny are separate — see `verify`)

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

# Issue-runner tests (ADR 0016). Dependency-free Node 20, so this needs no install step
# and runs in the `Client` CI job, which is the one that already has a Node toolchain.
# The shell expands this glob, deliberately: `node --test '<glob>'` needs Node 21+ (CI
# pins 20), and passing the directory makes Node resolve it as a package via its
# package.json rather than searching it for tests.
runner-test:
	node --test tools/agent-task/*.test.js

# The workflow-policy rules (#199). Unit tests, so they ride in the fast gate and the
# `Client` job next to the runner's — the gate they *enforce* is `ci-lint`, which needs
# actionlint and therefore lives in `verify` instead.
ci-policy-test:
	node --test tools/ci-policy/*.test.js

# The AI reviewer (ADR 0015, #243). Dependency-free Node, so it rides the fast gate and the
# `Client` job like the runner's tests. No test here calls a paid model or mutates a repository:
# GitHub is faked at the `fetch` boundary and the provider is faked at the adapter boundary.
ai-review-test:
	node --test tools/ai-review/*.test.js

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

# Workflow gate (#199), the second half of the `cargo-deny` CI job — CI is supply chain too.
# Two layers, in order: actionlint answers "is this a valid workflow?" (schema, expressions,
# shellcheck), and tools/ci-policy answers "is it allowed here?" (immutable SHA pins,
# declared least-privilege tokens, no `pull_request_target`, no untrusted event data
# interpolated into a shell). Lives in `verify` and not `check` so the fast inner-loop gate
# keeps needing nothing but cargo and node.
ci-lint:
	@command -v actionlint > /dev/null 2>&1 || { \
	  echo "missing: actionlint — install with 'go install github.com/rhysd/actionlint/cmd/actionlint@latest'"; \
	  echo "         or 'brew install actionlint'. CI installs it via taiki-e/install-action."; \
	  exit 1; \
	}
	actionlint
	node tools/ci-policy/check.js

setup:
	scripts/bootstrap.sh
