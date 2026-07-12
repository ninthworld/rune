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
- `docs/rules-coverage.md` updated when engine rule behavior was added or changed
  (the CR-citation convention in `docs/coding-standards.md`).
- No unrelated diffs.

## `main` branch ruleset (GitHub settings — not enforceable from a file)

The protection contract for `main` is defined as an importable ruleset at
[`.github/rulesets/main.json`](../../.github/rulesets/main.json) and applied once via
**Settings → Rules → Rulesets → Import** (steps in
[`.github/rulesets/README.md`](../../.github/rulesets/README.md)). Keeping the JSON in
the repo makes the applied settings reviewable and reproducible even though GitHub can
only enforce them from repository settings, not from the file. The active ruleset
enforces:

- **Pull requests required.** No direct pushes to `main`; every change arrives via PR.
- **≥ 1 recorded approval**, and **stale approvals are dismissed** when new commits are
  pushed, so the approval always reflects the merged code.
- **Code-owner review** for the protected paths in `.github/CODEOWNERS`
  (`/crates/rune-engine/`, `/docs/protocol.md`, `/docs/decisions/`, and `*`).
- **Review-conversation resolution** required before merge.
- **Required status checks** `Engine`, `Client`, `E2E`, and `cargo-deny`, with
  **strict** "Require branches to be up to date before merging" enabled — a PR that is
  behind `main` cannot merge until it is updated onto current `main` and the checks
  re-run against that base. (The complete local verification contract that mirrors
  these checks is #184's `make verify` gate.)
- **Linear history** and **squash-only** merging (`allowed_merge_methods: ["squash"]`);
  the squashed PR title becomes the commit — keep it Conventional Commits.
- **No force pushes** and **no deletion** of `main`.

### Who may approve, and bypass

- **Only a human other than the PR author may approve.** Agents never approve or merge —
  not their own PRs and not another agent's. For RUNE that approver is the maintainer,
  **@ninthworld** (the sole code owner). An agent's job ends at "green CI + PR ready for
  review"; the recorded approval and the merge click are the maintainer's.
- Normal agent/author credentials **cannot bypass** the ruleset. The only bypass actor
  is the repository **Admin** role, reserved for explicit emergencies; every bypass is
  recorded in the repository audit log, so it is auditable after the fact.

### Handling stale branches

Because strict status checks are on, an agent PR that has fallen behind `main` is
blocked from merging until it is brought current. The agent that owns the branch
updates it — since the branch is exclusively its own, it may rebase onto current `main`
and push with `--force-with-lease` (see the force-push rule in `AGENTS.md`), which
re-triggers the required checks against the new base. A merge commit from `main` is not
an option here (linear history is required). Executable worktree/rebase automation is
out of scope for this contract and lives in the future agent-runner task.

## Other repository settings (apply once on GitHub)

- Actions → Workflow permissions: read-only default token.
- Merge queue is unavailable while RUNE is a user-owned public repository; the strict
  "up to date before merging" rule above is the stale-branch guard in its place. Adopt a
  merge queue if the repo moves under an organization and PR volume warrants it.
- Known caveat: PRs created by `github-actions[bot]` / the default `GITHUB_TOKEN`
  do **not** trigger CI (recursion protection). Agent PRs must be opened with the
  agent's own credentials (GitHub App or PAT), or a human approves workflow runs
  on bot PRs.
- Seed the labels above before the first agent run.
