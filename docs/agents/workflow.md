# Agent workflow and GitHub configuration

## The loop

1. **Issues are the queue.** Agents pick up issues labeled `agent-task` +
   `status:ready`, or create new ones (agent-task template) when they identify
   work — triage, decomposition, and prioritization happen in issues, not PRs.
2. **One issue → one branch → one PR.** Branch `agent/<issue>-<slug>`.
3. **PR is the review gate.** Fill the template, link `Closes #N`, label `agent`.
   CI (`Engine`, `Client`) must be green. A human reviews and merges — always.
4. **CI failure protocol:** the PR author (agent) investigates its own red CI and
   pushes fixes. If the failure is unrelated (flake, infra), say so in a comment
   with evidence rather than retrying blindly.
5. **Conflicts between agents are resolved by humans**, not by other agents.

To run this loop locally with a model served by Ollama instead of a cloud
provider, see [`local-ai-setup.md`](local-ai-setup.md).

## Labels

`agent-task`, `agent` (on PRs), `bug`, `decision`, `dependencies`,
`area:{engine,protocol,server,cli,client,docs,ci}`,
`status:{ready,blocked,needs-decision}`, `good-first-task`.

## Definition of done

- Acceptance criteria of the linked issue met.
- `make check` green in CI.
- Tests cover the change; rules fixes include a regression test named after the issue.
- Docs/ADRs updated where behavior or architecture changed.
- No unrelated diffs.

## Repository settings (apply once on GitHub — cannot be committed)

- Branch protection on `main`: require PRs; required checks `Engine` and `Client`;
  require 1 human approval; dismiss stale approvals; no force pushes; authors
  cannot approve their own PRs. Enable merge queue when PR volume warrants it.
- Squash-merge only; PR title becomes the commit (enforce Conventional Commits).
- Actions → Workflow permissions: read-only default token.
- Known caveat: PRs created by `github-actions[bot]` / the default `GITHUB_TOKEN`
  do **not** trigger CI (recursion protection). Agent PRs must be opened with the
  agent's own credentials (GitHub App or PAT), or a human approves workflow runs
  on bot PRs.
- Seed the labels above before the first agent run.
