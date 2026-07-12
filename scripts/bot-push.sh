#!/usr/bin/env bash
# Push a branch to `origin` as `rune-agent[bot]`. Extra args are passed through to
# `git push` (e.g. --force-with-lease when bringing a stale branch current onto main
# — see docs/agents/workflow.md).
#
#   scripts/bot-push.sh                          # the current branch, from the cwd
#   scripts/bot-push.sh --force-with-lease
#   scripts/bot-push.sh --repo <dir> --branch <name> [git push args...]
#
# The `--repo`/`--branch` form exists for the issue runner (ADR 0016): it pushes from
# the runner-owned *mirror*, never from the provider's working copy, so no credentialed
# git command ever runs in a repository a provider could have rewritten. A bare mirror
# has no current branch, hence the explicit `--branch`.
set -euo pipefail

repo=""
branch=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)   repo="${2:?--repo needs a path}";     shift 2 ;;
    --branch) branch="${2:?--branch needs a name}"; shift 2 ;;
    *) break ;;
  esac
done

git_args=()
push_args=()
[[ -n "$repo" ]] && git_args+=(-C "$repo")

if [[ -z "$branch" ]]; then
  branch=$(git "${git_args[@]}" symbolic-ref --quiet --short HEAD) \
    || { echo "bot-push: detached HEAD — check out a branch first" >&2; exit 1; }
  # Only meaningful for a working checkout: a bare mirror has no branch to track, and
  # the runner passes an explicit `--force-with-lease=<ref>:<sha>` rather than relying
  # on a remote-tracking ref for its lease.
  push_args+=(--set-upstream)
fi
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
git "${git_args[@]}" \
    -c credential.helper= \
    -c credential.helper='!f() { echo username=x-access-token; echo "password=$BOT_TOKEN"; }; f' \
  push "${push_args[@]}" origin "$branch" "$@"
