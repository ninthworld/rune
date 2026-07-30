.PHONY: verify check engine-test engine-lint engine-fmt compat deny setup

# The complete local pre-merge gate: everything required before a PR merges into
# `main`. Composes the existing targets 1:1 with the required GitHub checks, so
# there is a single command whose coverage matches CI. `make check` remains the
# fast inner loop.
#
# Client and e2e targets are absent until the web client exists. They land with
# it — including the browser e2e suite, which is a required gate (ADR 0011).
verify: check deny ## Full pre-merge verification: Engine + cargo-deny (mirrors every required GitHub check)

check: engine-lint engine-test ## Fast inner-loop gate: everything the Engine CI job runs (cargo-deny is separate — see `verify`)

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

# Supply-chain gate (the `cargo-deny` CI job). Same subcommand + checks the
# deny.yml workflow runs, kept here so the command lives in exactly one place.
deny:
	cargo deny check advisories licenses bans sources

setup:
	scripts/bootstrap.sh
