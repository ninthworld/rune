#!/usr/bin/env bash
# Mint a short-lived (1h) installation token for the rune-agent GitHub App and
# print it to stdout. PRs and pushes made with this token are authored by
# `rune-agent[bot]`, not by a human — which is what lets the repo owner review
# and approve them under the `main-protection` ruleset (CODEOWNERS is a human,
# and GitHub forbids approving your own PR).
#
# Setup: see docs/agents/workflow.md ("Bot-authored PRs").
#   RUNE_BOT_APP_ID  — the App ID from the app's settings page
#   RUNE_BOT_KEY     — path to the app's private key (default below)
set -euo pipefail

KEY_PATH="${RUNE_BOT_KEY:-$HOME/.config/rune/rune-agent.pem}"
APP_ID="${RUNE_BOT_APP_ID:-$(cat "$HOME/.config/rune/app-id" 2>/dev/null || true)}"
REPO="${RUNE_BOT_REPO:-ninthworld/rune}"

die() { echo "bot-token: $*" >&2; exit 1; }

[[ -n "$APP_ID" ]] || die "no App ID. Set RUNE_BOT_APP_ID or write it to ~/.config/rune/app-id"
[[ -r "$KEY_PATH" ]] || die "private key not readable at $KEY_PATH (set RUNE_BOT_KEY)"

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# GitHub rejects a JWT whose iat is in the future, so backdate it to absorb clock skew.
now=$(date +%s)
header=$(printf '{"alg":"RS256","typ":"JWT"}' | b64url)
payload=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' "$((now - 60))" "$((now + 540))" "$APP_ID" | b64url)
signature=$(printf '%s.%s' "$header" "$payload" \
  | openssl dgst -sha256 -sign "$KEY_PATH" -binary \
  | b64url)
jwt="$header.$payload.$signature"

installation_id=$(gh api "repos/$REPO/installation" -H "Authorization: Bearer $jwt" --jq '.id' 2>/dev/null) \
  || die "app $APP_ID is not installed on $REPO (Install App in the app's settings)"

gh api --method POST "app/installations/$installation_id/access_tokens" \
  -H "Authorization: Bearer $jwt" --jq '.token'
