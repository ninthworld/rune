#!/usr/bin/env bash
# Push the current branch and open a PR as `rune-agent[bot]` so the repo owner
# can review and approve it. Commits keep their original author; only the push
# and the PR are attributed to the bot.
#
#   scripts/bot-pr.sh "feat(engine): title" "Closes #123\n\nBody..."
#   scripts/bot-pr.sh --head <branch> --draft --no-push "title" "body"
#
# The flag form exists for the issue runner (ADR 0016), which has already pushed the
# branch from its mirror and wants a draft PR for a branch it is not standing on.
set -euo pipefail

REPO="${RUNE_BOT_REPO:-ninthworld/rune}"
head=""
push=1
gh_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --head)    head="${2:?--head needs a branch}"; shift 2 ;;
    --draft)   gh_args+=(--draft);                 shift ;;
    --no-push) push=0;                             shift ;;
    *) break ;;
  esac
done

title="${1:?usage: bot-pr.sh [--head <branch>] [--draft] [--no-push] <title> [body]}"
body="${2:-}"

if [[ -z "$head" ]]; then
  head=$(git symbolic-ref --quiet --short HEAD) \
    || { echo "bot-pr: detached HEAD — check out a branch first" >&2; exit 1; }
fi
[[ "$head" != "main" ]] || { echo "bot-pr: refusing to open a PR from main" >&2; exit 1; }

if [[ "$push" -eq 1 ]]; then
  "$(dirname "$0")/bot-push.sh"
fi

GH_TOKEN=$("$(dirname "$0")/bot-token.sh") gh pr create \
  --repo "$REPO" --base main --head "$head" \
  --title "$title" --body "$body" "${gh_args[@]}"
