.PHONY: check engine-test engine-lint engine-fmt client-check client-lint client-install setup

check: engine-lint engine-test client-check ## Everything CI runs

engine-lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

engine-fmt:
	cargo fmt --all

engine-test:
	cargo test --workspace

client-install:
	cd clients/web && npm install

client-lint: client-install
	cd clients/web && npm run lint

client-check: client-install
	cd clients/web && npm run lint && npm run typecheck && npm run build

setup:
	scripts/bootstrap.sh
