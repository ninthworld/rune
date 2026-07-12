#!/usr/bin/env sh
# Install the pinned `actionlint` — the validity half of the workflow gate (`make ci-lint`).
#
#   scripts/install-actionlint.sh [target-dir]     # default: $HOME/.local/bin
#
# One implementation, three callers: the `cargo-deny` CI job, the provider image
# (tools/agent-task/Dockerfile), and a contributor following `scripts/bootstrap.sh`. The
# version and its checksum therefore live in exactly one place and cannot drift apart.
#
# The checksum is not decoration. This script fetches a release binary over the network and
# then runs it against the workflows that gate every merge; a hardening tool installed from
# an unverified download would be undoing the point of the tool. If the hash does not match,
# this fails and installs nothing.
#
# Bumping it: take the version and the matching `linux_amd64` line from
# https://github.com/rhysd/actionlint/releases (the `_checksums.txt` asset).
set -eu

VERSION="${ACTIONLINT_VERSION:-1.7.12}"
SHA256="${ACTIONLINT_SHA256:-8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8}"

target="${1:-$HOME/.local/bin}"
tarball="actionlint_${VERSION}_linux_amd64.tar.gz"
url="https://github.com/rhysd/actionlint/releases/download/v${VERSION}/${tarball}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "install-actionlint: fetching actionlint ${VERSION}"
curl -fsSL "$url" -o "$tmp/$tarball"

echo "${SHA256}  $tmp/$tarball" | sha256sum -c - > /dev/null \
  || { echo "install-actionlint: checksum mismatch for $tarball — refusing to install" >&2; exit 1; }

tar -xzf "$tmp/$tarball" -C "$tmp" actionlint
mkdir -p "$target"
install -m 0755 "$tmp/actionlint" "$target/actionlint"

echo "install-actionlint: installed $("$target/actionlint" --version | head -1) at $target/actionlint"
