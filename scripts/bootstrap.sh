#!/usr/bin/env sh
# One-time prerequisite check for RUNE contributors (human or agent).
#
# Covers both local gates:
#   - `make check`  (fast inner loop): Rust toolchain + Node 20+
#   - `make verify` (full pre-merge):  the above + cargo-deny + Playwright Chromium
# Each missing prerequisite prints an actionable install command; the script exits
# non-zero if anything is absent.
set -e

fail=0

# --- Fast gate: `make check` -------------------------------------------------

if command -v cargo > /dev/null 2>&1; then
  echo "ok: cargo $(cargo --version)"
else
  echo "missing: Rust toolchain — install via https://rustup.rs (rust-toolchain.toml pins stable + rustfmt + clippy)"
  fail=1
fi

if command -v node > /dev/null 2>&1; then
  node -e 'const v=+process.versions.node.split(".")[0]; process.exit(v>=20?0:1)' \
    && echo "ok: node $(node --version)" \
    || { echo "missing: Node 20+ (found $(node --version))"; fail=1; }
else
  echo "missing: Node 20+ — https://nodejs.org"
  fail=1
fi

# --- Full gate: `make verify` adds the E2E and cargo-deny surfaces -----------

if command -v cargo-deny > /dev/null 2>&1; then
  echo "ok: cargo-deny $(cargo-deny --version)"
else
  echo "missing: cargo-deny — install with 'cargo install --locked cargo-deny' (needed by 'make deny'/'make verify' and the cargo-deny CI job)"
  fail=1
fi

# actionlint validates the committed workflow YAML; `make ci-lint` runs it, then applies
# RUNE's own policy rules (tools/ci-policy: immutable Action pins, least-privilege tokens).
# Needed by `make verify` and the `cargo-deny` CI job, which is where that gate runs (#199).
if command -v actionlint > /dev/null 2>&1; then
  echo "ok: actionlint $(actionlint --version | head -1)"
else
  echo "missing: actionlint — install with 'go install github.com/rhysd/actionlint/cmd/actionlint@latest' or 'brew install actionlint' (needed by 'make ci-lint'/'make verify' and the cargo-deny CI job)"
  fail=1
fi

# Playwright's pinned Chromium for the browser E2E suite. Browsers live under
# PLAYWRIGHT_BROWSERS_PATH when set, otherwise Playwright's default cache dir.
pw_path="${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}"
if ls "$pw_path"/chromium-* > /dev/null 2>&1; then
  echo "ok: Playwright Chromium present under $pw_path"
else
  echo "missing: Playwright Chromium — run 'make e2e-browser' to install it (needed by 'make e2e'/'make verify' and the E2E CI job)"
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  echo "prerequisites ready — 'make check' is the fast gate; run 'make verify' before requesting final review"
else
  echo "one or more prerequisites are missing (see above): 'make check' needs cargo + node; 'make verify' needs all five"
  exit 1
fi
