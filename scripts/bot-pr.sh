#!/usr/bin/env bash
# Push the current branch and open a PR as `rune-agent[bot]` so the repo owner
# can review and approve it. Commits keep their original author; only the push
# and the PR are attributed to the bot.
#
# Usage: scripts/bot-pr.sh "feat(engine): title" "Closes #123\n\nBody..."
set -euo pipefail

REPO="${RUNE_BOT_REPO:-ninthworld/rune}"
title="${1:?usage: bot-pr.sh <title> [body]}"
body="${2:-}"

branch=$(git symbolic-ref --quiet --short HEAD) \
  || { echo "bot-pr: detached HEAD — check out a branch first" >&2; exit 1; }
[[ "$branch" != "main" ]] || { echo "bot-pr: refusing to open a PR from main" >&2; exit 1; }

"$(dirname "$0")/bot-push.sh"

GH_TOKEN=$("$(dirname "$0")/bot-token.sh") gh pr create \
  --repo "$REPO" --base main --head "$branch" \
  --title "$title" --body "$body"
