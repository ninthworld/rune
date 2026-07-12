# Agent workflow and GitHub configuration

**This file is the *how*: commands, labels, and GitHub settings.** The *what* and the
*why* — the lifecycle from a milestone outcome to a merged PR, its state transitions, the
evidence each handoff requires, and the human gates between them — are the
[continuance playbook](continuance.md), which is the authority when the two appear to
disagree. In one paragraph: issues are the queue; a `status:ready` leaf issue becomes one
branch and one PR; every required check must be green against current `main`; a human
other than the author approves and merges — always.

## The issue runner

`scripts/agent-task` runs one issue through that lifecycle
([ADR 0016](../decisions/0016-provider-neutral-issue-runner.md)). It performs every GitHub
mutation as `rune-agent[bot]`, and it never approves or merges.

    scripts/agent-task doctor              # can this machine run agent tasks?
    scripts/agent-task start 186 --provider claude
    scripts/agent-task logs 186 --follow   # watch what the provider is doing, from another window
    scripts/agent-task status              # lifecycle state of active runs
    scripts/agent-task resume 186          # re-enter a failed run; the claim and the work survive
    scripts/agent-task report 186          # record what CI actually did, once it settles
    scripts/agent-task release 186         # drop the claim, return the issue to status:ready

`--provider` selects the implementation agent and is the only thing that differs between
them; no provider is canonical. To drive a run with a local model instead of a cloud one,
use the `local` adapter: `RUNE_LOCAL_CMD='<your harness> "$(cat "$RUNE_BRIEF")"'`. It
prescribes no model, harness, or vendor — and unlike the old `local-agent.sh` example it
replaced, it does not push or open the PR as you. Running the loop **without** the runner
at all is the manual path in the [playbook](continuance.md#doing-the-work); it produces
the same artifacts and passes the same gates.

### Setting up the sandbox (one-time)

The runner refuses to start a provider it cannot contain, because the `rune-agent` private key is
readable by whatever UID runs the provider — without a boundary, a provider can mint its own token
and open its own PRs ([ADR 0016](../decisions/0016-provider-neutral-issue-runner.md)). A container
is the easiest boundary to get:

    docker build -t rune/provider -f tools/agent-task/Dockerfile .
    export RUNE_PROVIDER_IMAGE=rune/provider

It builds from the **repository root**, not from `tools/agent-task/`, because it needs
`clients/web/package-lock.json` to pin the E2E browser to the Playwright the client actually uses.
Rebuild the image when that lockfile changes.

The image carries the **whole toolchain** — Rust, Node, `cargo-deny`, and the pinned Chromium — not
just the model CLI, because verification runs inside the same boundary: `make verify` executes
provider-controlled code by construction (a doctored `Makefile` or `build.rs` runs right there),
which is exactly why it must not run as you. The browser is **baked into the image** rather than
installed per run: `make e2e-browser` runs `playwright install --with-deps`, which needs root to
`apt-get` its system libraries, and the sandbox is unprivileged by design. For the non-container
isolation modes, run `make e2e-browser` once on the host instead.

Then give the provider a token. Its interactive login lives under your real `HOME`, which the
sandbox deliberately replaces — the same isolation that hides the app key also hides `~/.claude` —
so a headless run needs a token rather than a `/login` session:

    claude setup-token                  # once; works for Pro/Max subscribers
    export CLAUDE_CODE_OAUTH_TOKEN=…    # or ANTHROPIC_API_KEY / OPENAI_API_KEY for codex

`scripts/agent-task doctor` tells you which of these is missing. The alternative to a container is
a second UID (`RUNE_PROVIDER_USER=<user>`, needs passwordless sudo). The last resort is
`--unsafe-same-uid`, which runs the provider **as you, with the key readable** — it warns, and it
is recorded as `isolation: same-uid` in the run summary so a run made without the boundary is never
mistaken for one made with it.

What the container gets: the run directory (workspace, scratch `HOME`, brief, logs) and a shared
build cache. That is all. No `~/.config/rune`, no `~/.config/gh`, no `~/.claude`, no host `PATH`,
and no credential beyond its own model token. It runs as your UID rather than root, so the files it
writes are yours, and its `origin` is a local path, so it has no remote to push to.

### Watching a run

`start` streams what the provider is doing as it does it — one line per tool call, per decision,
per result:

    ▸ session started (claude-opus-4-8)
      Reading the engine to find where damage is applied.
    ▸ Grep: fn apply_damage
    ▸ Edit: crates/rune-engine/src/combat.rs
    ▸ Bash: make check
    ▸ finished: success, 14 turns, $0.37

This is not decoration. Claude Code's print mode emits **nothing at all** until the run is over, so
a forty-minute run looks exactly like a hung one — and the first thing anyone does with a silent
agent is kill it. The runner asks for `--output-format stream-json` and renders the events.

From another window, or after the fact: `scripts/agent-task logs <issue> --follow`. The rendered
log is `logs/provider.log` in the run directory; the raw event stream is kept verbatim next to it in
`logs/provider.jsonl` (`logs --raw`), because rendering is best-effort and a provider's event schema
is not something the runner controls. Both are redaction-scrubbed. `start --quiet` turns the live
stream off.

Claiming is **atomic**: the runner creates the issue's `agent/<issue>-<slug>` branch from
current `main`, and GitHub's 422 on an existing ref means exactly one runner can win. (A
GitHub App cannot be an issue assignee, so the branch — not an assignment — is the lock.)
A losing runner mutates nothing. Every preflight check runs before the first mutation, so a
task that is closed, blocked, dependency-blocked, malformed, or already claimed leaves no
trace.

After the provider stops, nothing it says is taken on trust. The runner inspects the diff
itself (no-op, out-of-scope paths, secrets, generated directories, provider-created commits),
runs the verification gates, rebases onto current `main` and re-verifies, then makes the
commit, pushes, and opens the **draft PR** — and only then moves the issue to `status:review`.

A diff touching **CI-governance paths** (`.github/workflows/`, `.github/actions/`,
`.github/rulesets/`, `.github/CODEOWNERS`, `Makefile`, `scripts/bot-*.sh`) is **refused**
unless the run was started with `--allow-ci`. The bot holds `workflows: write`, so a change
there can weaken the checks reporting green on that very PR; a permitted one is labelled
`ci-change` and called out at the top of the PR body.

A **failed run keeps its claim**, its branch, and its working copy, so it is resumable rather
than lost: `resume` picks up whatever is in the workspace now — what the provider left, or what
you fixed by hand after reading the gate output — and `resume --rerun-provider` hands it back to
the provider first. A claim whose run stopped heartbeating shows as `⚠️ STALE` in `list`;
taking one over is `release --force`, which is a human's call, never the runner's.

### Run summaries

Every run that ends — success *or* failure — publishes a sanitized, versioned summary to the
**`agent-runs`** orphan branch (one JSON file per run, append-only; a correction is a new record
that `supersedes` an earlier one, never a rewrite). This is the audit surface #200 consumes.

Summaries record **runner-observed** facts (terminal outcome, normalized failure stage, per-gate
results, PR author, whether the ADR 0015 review actually ran) separately from **provider-reported**
usage, which is advisory and not comparable across providers. They never contain prompts, briefs,
diffs, logs, environment values, or secrets.

`report <issue>` re-observes a finished run once CI has settled. It exists because at the moment
a PR opens, its checks have not run yet — and `claude-review` is **not** a required check, so a
silently skipped review is indistinguishable from a passing one unless somebody looks.

## Milestone stewardship

When a milestone runs out of `status:ready` issues, its closeout and the next milestone's
decomposition follow the state machine in
[ADR 0017](../decisions/0017-milestone-stewardship-cycle.md). The phases, the two human
gates, what counts as evidence, and the manual procedure for the stages that are not built
yet are the [playbook's milestone loop](continuance.md#3-the-milestone-loop); the commands
are here.

### Evidence collection

The cycle's first stage is built (#224); the rest of it (#225–#228) is not. What exists
today gathers the evidence and hands it to a human:

    scripts/agent-cycle collect M3      # snapshot one commit; build the Evidence Bundle
    scripts/agent-cycle show <cycle-id> # summarize a collected bundle
    scripts/agent-cycle list            # cycles collected on this machine

`collect` pins the cycle to current `origin/main`, clones *that commit*, and reads it:
the milestone's exit criteria **verbatim** out of `docs/roadmap.md`, its issues (from
GitHub's milestone tag and the roadmap's own tables, with the drift between them
recorded rather than resolved), the PRs that closed them with their merge SHAs, the
required checks each of those PRs got, **a fresh gate run against the audited commit
itself**, test counts, the `docs/rules-coverage.md` rows in the milestone's CR scope,
the status of every ADR the criteria name, `TODO`/`unimplemented!` locations under the
paths they name, and the "Partial: …" gaps a human already wrote down.

It **reads only** — no label, comment, branch, PR, or model call — and the bundle it
writes lives outside the repository, under `$XDG_STATE_HOME/rune/cycles/<cycle-id>/`,
so a bundle can never land in a diff. Nothing in it is a verdict: a closed issue is
recorded as a closed issue, never as a satisfied criterion, because an issue can close
without its acceptance criteria being met. Two facts it deliberately keeps apart: the
green checks on a milestone's merged PRs prove those PRs passed *then*; only the fresh
gate run says anything about `main` *now*.

Until the audit and the gates that follow it land, milestone reconciliation in
`docs/roadmap.md` is still done by hand, by a human — reading, now, the same evidence
the Auditor will eventually be handed.

## Labels

`agent-task`, `agent` (on PRs), `bug`, `decision`, `dependencies` (Dependabot),
`ci-change` (on PRs touching CI-governance paths — see below),
`area:{engine,protocol,server,cli,client,docs,ci}`,
`status:{ready,in-progress,review,blocked}`, `good-first-task`.

The `status:*` labels **are** the issue lifecycle, so they are not decoration:
[ADR 0016](../decisions/0016-provider-neutral-issue-runner.md) has a run claim a
`status:ready` issue by atomically creating its `agent/<issue>-<slug>` branch, move it to
`status:in-progress`, and move it to `status:review` only once the draft PR exists. What
each state means and when it may change is the
[playbook's issue loop](continuance.md#lifecycle). Work blocked on an open decision is
`status:blocked` like any other blocked work — there is no separate "needs decision" state.

## Definition of done

The full exit contract — including the evidence a PR body must carry and what must follow
a merge — is the [playbook's PR loop](continuance.md#5-the-pull-request-loop). The short
form:

- Acceptance criteria of the linked issue met, each mapped to its evidence in the PR body.
- `make check` green throughout implementation; `make verify` green before final review
  (the full `Engine` + `Client` + `E2E` + `cargo-deny` surface) wherever the browser
  suite can run, and all four checks green in CI against current `main`.
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
