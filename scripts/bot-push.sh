#!/usr/bin/env bash
# Push the current branch to `origin` as `rune-agent[bot]`. Extra args are passed
# through to `git push` (e.g. --force-with-lease when bringing a stale branch
# current onto main — see docs/agents/workflow.md).
#
#   scripts/bot-push.sh
#   scripts/bot-push.sh --force-with-lease
set -euo pipefail

branch=$(git symbolic-ref --quiet --short HEAD) \
  || { echo "bot-push: detached HEAD — check out a branch first" >&2; exit 1; }
[[ "$branch" != "main" ]] || { echo "bot-push: refusing to push main" >&2; exit 1; }

BOT_TOKEN=$("$(dirname "$0")/bot-token.sh")
export BOT_TOKEN

# Push to the named remote, never a bare URL: pushing to a URL sets
# `branch.<name>.remote` to that URL and creates no remote-tracking ref, which
# silently breaks `--force-with-lease` (it has no lease to compare against).
#
# The empty `credential.helper` first is load-bearing. The key is multi-valued, so
# `-c credential.helper=...` alone *appends* to the user's existing helpers rather
# than replacing them — the maintainer's keyring helper answers first and the push
# goes out as a human. Resetting the list makes the bot the only credential source.
# The token reaches git via the environment, so it never lands in argv, `ps`, or
# the reflog.
git -c credential.helper= \
    -c credential.helper='!f() { echo username=x-access-token; echo "password=$BOT_TOKEN"; }; f' \
  push --set-upstream origin "$branch" "$@"
