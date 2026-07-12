# Agent workflow and GitHub configuration

## The loop

1. **Issues are the queue.** Agents pick up issues labeled `agent-task` +
   `status:ready`, or create new ones (agent-task template) when they identify
   work — triage, decomposition, and prioritization happen in issues, not PRs.
2. **One issue → one branch → one PR.** Branch `agent/<issue>-<slug>`.
3. **PR is the review gate.** Fill the template, link `Closes #N`, label `agent`.
   All four required checks — `Engine`, `Client`, `E2E`, `cargo-deny` — must be green
   (reproduce them locally with `make verify`). A human reviews and merges — always.
4. **CI failure protocol:** the PR author (agent) investigates its own red CI and
   pushes fixes. If the failure is unrelated (flake, infra), say so in a comment
   with evidence rather than retrying blindly.
5. **Conflicts between agents are resolved by humans**, not by other agents.

To run this loop locally with a model served by Ollama instead of a cloud
provider, see [`local-ai-setup.md`](local-ai-setup.md).

## Labels

`agent-task`, `agent` (on PRs), `bug`, `decision`, `dependencies`, `ci-change` (on PRs
touching CI-governance paths — see below),
`area:{engine,protocol,server,cli,client,docs,ci}`,
`status:{ready,in-progress,review,blocked,needs-decision}`, `good-first-task`.

`status:in-progress` and `status:review` are the issue-runner lifecycle states from
[ADR 0016](../decisions/0016-provider-neutral-issue-runner.md): a run claims a
`status:ready` issue by atomically creating its `agent/<issue>-<slug>` branch, and moves
it to `status:review` only once the draft PR exists.

## Definition of done

- Acceptance criteria of the linked issue met.
- `make check` green throughout implementation; `make verify` green before final review
  (the full `Engine` + `Client` + `E2E` + `cargo-deny` surface) wherever the browser
  suite can run, and all four checks green in CI.
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

### Bot-authored PRs

"Only a human other than the PR author may approve" only holds if agent PRs are not
authored by the maintainer. A PR is authored by whoever's token calls the API, so a
local agent session using the maintainer's `gh` login produces a PR **@ninthworld
cannot approve** — GitHub forbids approving your own PR, leaving the Admin bypass as
the only way to merge. That is the failure this section exists to prevent.

Agent PRs are therefore opened as **`rune-agent[bot]`**, a GitHub App the maintainer
owns (App ID 4277040, installed on this repo only). Commits keep their real author;
only the push and the PR carry the bot identity.

    scripts/bot-token.sh                   # mints a 1h installation token
    scripts/bot-push.sh [--force-with-lease]   # pushes the current branch as the bot
    scripts/bot-pr.sh "<title>" "<body>"   # pushes the branch, opens the PR as the bot

Use `bot-push.sh` for the rebase-onto-`main` flow above, so the branch stays bot-owned
and the lease ref exists. Never `git push` an agent branch directly: pushing to a bare
URL rather than the named remote creates no remote-tracking ref, and `--force-with-lease`
then silently has nothing to compare against.

One-time setup for a new machine — create the app's private key at
`~/.config/rune/rune-agent.pem` (mode `600`) and its App ID at `~/.config/rune/app-id`;
override with `RUNE_BOT_KEY` / `RUNE_BOT_APP_ID`. The key is a credential: never commit
it (`*.pem` is gitignored) and never paste it into a PR or issue.

The app's granted permissions are `contents`, `issues`, `pull_requests`, and
`workflows` (write), plus `metadata`, `actions`, and `checks` (read).

`workflows: write` means the bot **can edit the CI that gates its own PR** — including
hollowing out a required job while keeping its name, which reports green. GitHub does
not prevent this; the human code-owner review does (CODEOWNERS covers `*`, so every PR
needs the maintainer's approval). Read any diff touching `.github/workflows/`,
`.github/actions/`, `.github/rulesets/`, `.github/CODEOWNERS`, `Makefile`, or
`scripts/bot-*.sh` with that in mind. [ADR 0016](../decisions/0016-provider-neutral-issue-runner.md)
records the tradeoff and the runner-side containment (an explicit per-run opt-in plus a
`ci-change` label, so such a change can never arrive unremarked).

Unlike the default `GITHUB_TOKEN` inside Actions (see the recursion caveat below), an
installation token used from a developer machine **does** trigger the required checks.

One coupling to remember: `claude-code-action` skips any run triggered by a bot unless
that bot is allowlisted, so `.github/workflows/claude-code-review.yml` sets
`allowed_bots: 'rune-agent[bot]'`. Without it the ADR 0015 review silently no-ops on
every agent PR — and since `claude-review` is not a required check, nothing would fail
to tell you. Renaming the app means updating that allowlist.

### Handling stale branches

Because strict status checks are on, an agent PR that has fallen behind `main` is
blocked from merging until it is brought current. The agent that owns the branch
updates it — since the branch is exclusively its own, it may rebase onto current `main`
and push with `--force-with-lease` (see the force-push rule in `AGENTS.md`), which
re-triggers the required checks against the new base. A merge commit from `main` is not
an option here (linear history is required). Executable worktree/rebase automation is
out of scope for this contract and lives in the issue runner
([ADR 0016](../decisions/0016-provider-neutral-issue-runner.md), implemented by #186).

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
