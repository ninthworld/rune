#!/usr/bin/env sh
# One-time prerequisite check for SAGE contributors (human or agent).
#
# Covers both local gates:
#   - `make check`  (fast inner loop): Rust toolchain + Node 20+
#   - `make verify` (full pre-merge):  the above + cargo-deny
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

# --- Full gate: `make verify` adds the browser and the cargo-deny surface ----

# The browser the e2e gate drives. It is *provisioned*, not required up front:
# `make e2e-*` runs `playwright install chromium`, which fetches the revision the
# pinned driver names and no other. So this reports rather than fails — a first
# `make verify` downloads it once, and every later one finds it.
if [ -n "$PLAYWRIGHT_BROWSERS_PATH" ]; then
  browsers="$PLAYWRIGHT_BROWSERS_PATH"
else
  browsers="$HOME/.cache/ms-playwright"
fi
if [ -d "$browsers" ] && ls "$browsers" 2>/dev/null | grep -q '^chromium'; then
  echo "ok: playwright chromium present in $browsers"
else
  echo "note: no playwright chromium in $browsers yet — 'make e2e-browser' (or any 'make e2e-*') fetches the pinned revision once; do not run 'playwright install' unpinned"
fi



if command -v cargo-deny > /dev/null 2>&1; then
  echo "ok: cargo-deny $(cargo-deny --version)"
else
  echo "missing: cargo-deny — install with 'cargo install --locked cargo-deny' (needed by 'make deny'/'make verify' and the cargo-deny CI job)"
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  echo "prerequisites ready — 'make check' is the fast gate; run 'make verify' before opening a PR"
else
  echo "one or more prerequisites are missing (see above): 'make check' needs cargo + node; 'make verify' also needs cargo-deny"
  exit 1
fi
