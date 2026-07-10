#!/usr/bin/env sh
# One-time toolchain check for RUNE contributors (human or agent).
set -e

fail=0

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

[ "$fail" -eq 0 ] && echo "toolchains ready — run 'make check'" || exit 1
