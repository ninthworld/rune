#!/usr/bin/env sh
# One-time prerequisite check for RUNE contributors (human or agent).
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

# --- Full gate: `make verify` adds the cargo-deny surface --------------------

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
