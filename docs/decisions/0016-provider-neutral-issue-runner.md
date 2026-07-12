# ADR 0016: Provider-neutral issue runner

- Status: accepted
- Date: 2026-07-11
- Issue: #185

## Context

RUNE's agent loop (`docs/agents/workflow.md`) is a contract, not a program. A
maintainer picks a ready issue, starts a harness by hand, watches it edit the
checkout, and then runs the Git and GitHub steps themselves. Everything that makes
the loop trustworthy — claiming, isolation, verification, rebasing onto current
`main`, committing, pushing, opening the PR — is today either a human habit or a
paragraph of prose. `scripts/local-agent.sh.example` is the closest thing to
automation and it is explicitly an example: it pushes and opens the PR with the
maintainer's own `gh` login, which (see below) produces a PR the maintainer is
forbidden from approving.

We want one stable command per issue, with the implementation provider selectable
(Claude Code, Codex CLI, or a local harness) and *everything else identical across
providers*. The failure mode to design against is the obvious one: a provider that
owns workflow state. If the harness decides what "claimed" means, what "verified"
means, and when a PR opens, then swapping providers swaps the governance, and the
project's rules live in a vendor's runtime instead of in this repository.

### Given: the runner acts as `rune-agent[bot]`

The runner's GitHub identity was settled by #205/#206 and is not reopened here. All
GitHub mutations (claim, push, PR, labels, comments) use a short-lived installation
token for the maintainer-owned **`rune-agent`** GitHub App, minted by
`scripts/bot-token.sh`. Commits keep their real author; only the push and the PR
carry the bot identity. This is what makes "only a human other than the PR author may
approve" satisfiable at all — a PR is authored by whoever's token calls the API, so a
runner using the maintainer's `gh` login would produce PRs @ninthworld cannot approve,
leaving the Admin bypass as the only way to merge.

Three consequences of that identity constrain this design. Each was re-verified
against the live app while writing this ADR, because one of them had already gone
stale in the issue text:

1. **A GitHub App cannot be an issue assignee.** `GET /assignees/rune-agent[bot]`
   returns 404 and `POST /issues/N/assignees` returns 403. Claim-by-assignment is
   impossible, not merely disfavored — a fact worth recording so it is not
   "restored" later by someone who assumes it was an oversight.
2. **The app now holds `workflows: write`** (verified at both the app and installation
   level, and confirmed by pushing a `.github/workflows/` edit as the bot). This
   *reverses* the premise issue #185 was written against, and with it the
   `needs-human-push` terminal state the issue proposed. The bot is no longer
   structurally unable to edit CI; it is now able to rewrite the very checks that gate
   its own PR. The containment for that has to be designed, not inherited from a
   missing permission — see "CI-governance paths" below.
3. **The ADR 0015 review can be skipped invisibly.** `claude-code-action` ignores
   bot-triggered runs unless allowlisted; `.github/workflows/claude-code-review.yml`
   sets `allowed_bots: 'rune-agent[bot]'`, so it does run today. But `claude-review`
   is not a required check, so a future skip — a renamed app, a dropped allowlist, a
   provider outage — is indistinguishable from a passing review. The runner must
   therefore *observe* whether the review actually ran rather than assume it.

The surrounding gates are fixed points: `main` requires one human code-owner approval
with stale approvals dismissed on push, four required checks (`Engine`, `Client`,
`E2E`, `cargo-deny`), strict up-to-date-before-merge, linear history, squash-only
(#183); `make verify` is the local mirror of that surface (#184); #200 wants durable,
sanitized run telemetry; #187 wants to reuse this adapter boundary for milestone
stewardship.

## Decision

RUNE builds a **core runner that owns the entire GitHub and Git lifecycle**, and
**thin provider adapters whose only job is to edit files inside an isolated working
copy**. The runner is a **dependency-free Node 20 program** invoked as
`scripts/agent-task`. **Implementing it is out of scope for this ADR** — it lands as
#186, to the acceptance criteria at the end of this document.

The organizing rule, from which most of what follows is derived:

> **The provider is a code-editing function, not a participant in the workflow.** It
> receives a working copy and a brief; it returns an edited working copy. Every
> decision with a consequence outside that directory — claim, verify, rebase, commit,
> push, PR, label, comment — belongs to the runner, and is made as `rune-agent[bot]`,
> never as the maintainer.

### Runtime: dependency-free Node 20

Node 20 is already a required toolchain for `clients/web`, so the runner adds no new
prerequisite. Its standard library covers every hard part with zero dependencies:
`node:crypto` (`createSign('RSA-SHA256')`) for the App JWT, `fetch` for the GitHub
API, `child_process` with process-group signalling for provider supervision,
`AbortSignal.timeout` for deadlines, native JSON for the run schema, and `node:test`
for the runner's own tests.

- **Bash is rejected** despite the existing scripts being Bash. The runner is a
  lifecycle state machine that must parse and emit versioned JSON, supervise a
  long-running child with timeouts and cancellation, terminate process *groups*
  reliably, and recover from partial GitHub mutations. Bash has no JSON, no structured
  error handling, and famously fragile process supervision; the resulting program would
  be a `jq`-and-`trap` construction that nobody can safely modify.
- **A Rust workspace tool is rejected** despite Rust being the primary language. It
  would put a compile step in the tooling inner loop, and an HTTP+GitHub client drags
  dependencies into a workspace whose `cargo-deny` gate is a feature, not an accident.
  Type safety buys little for an orchestrator that is almost entirely I/O and
  subprocess management.
- **The credential path stays in Bash.** The runner shells out to the existing
  `scripts/bot-token.sh` rather than reimplementing the JWT exchange in Node. That
  keeps exactly one implementation of the credential path — the one already reviewed
  and already hardened against the bare-URL/`--force-with-lease` failure (#208) — and
  keeps the manual path working for interactive agent sessions. These scripts are not
  deleted by the runner; they *become* its credential layer.

### Command surface

```sh
scripts/agent-task start <issue> --provider <claude|codex|local> [--allow-ci] [--timeout 45m]
scripts/agent-task status [<issue>|<run-id>]     # lifecycle state, gates, links
scripts/agent-task list                          # active and recent runs
scripts/agent-task resume <issue>                # re-enter a failed run, claim retained
scripts/agent-task release <issue> [--force]     # drop the claim; --force takes over a stale one
scripts/agent-task cleanup [<issue>|--all]       # remove run directories and local state
scripts/agent-task doctor                        # preflight: CLIs, auth, key, isolation mode
```

`start` is the only subcommand that mutates GitHub without an explicit human command
behind it, and it never merges or approves. `doctor` exists because the most common
failure — a missing provider CLI, an unreadable key, an isolation mode the machine
cannot support — should be discoverable *before* an issue is claimed, not after.

### Claim: creating the canonical branch is the lock

Because assignment is impossible (consequence 1), **the claim is the atomic creation
of the remote branch**. The runner resolves current `origin/main`, then calls
`POST /repos/:owner/:repo/git/refs` to create `refs/heads/agent/<issue>-<slug>` at
that SHA with the bot's token. GitHub returns `422` if the ref already exists, so the
API call *is* a compare-and-swap: exactly one runner can win, with no lease file, no
lock service, and no race window. The winner then moves the issue to
`status:in-progress` and comments with the run ID, branch, provider, actor, and claim
time. A losing runner exits before doing any work.

Everything else about the run is local until the claim succeeds — the claim is the
first GitHub mutation and the first irreversible step.

### Issue lifecycle and labels

```
status:ready ──claim──▶ status:in-progress ──draft PR opened──▶ status:review
                   │                    │
                   │                    ├── failure ──▶ stays status:in-progress (resumable)
                   │                    └── release ──▶ status:ready (claim + branch dropped)
                   └── preflight reject ──▶ status:blocked (unchanged, never claimed)
```

A **failed run keeps its claim**. This is deliberate: a run that failed in verification
holds a worktree, a branch, and a diff that are worth resuming, and dropping the claim
on every stumble invites two runners to redo the same work. The failure is recorded as
an issue comment naming the normalized failure stage, and the run stays resumable until
someone runs `release`. A claim whose run has not heartbeated for a configurable
interval is **stale**, and `release --force` — a human command — takes it over.

`status:review` is set **only after the draft PR exists**, so the label always
reflects a reviewable artifact rather than an intention.

### Isolation: a per-run clone, not a `git worktree`

Each run gets its own **local clone** of a runner-owned mirror, at
`${XDG_STATE_HOME:-~/.local/state}/rune/runs/<run-id>/repo` — outside the repository,
so run state can never be captured in a diff, and never in the maintainer's checkout,
which the runner does not touch at all.

It is a clone and **not `git worktree add`**, because a worktree *shares `.git/config`
and `.git/hooks` with its parent*. A provider with filesystem access to its worktree
can write `.git/hooks/pre-push` or set a `credential.helper` — and those take effect in
the maintainer's own checkout. Worktrees isolate the working tree, which is the part we
do not need isolated; they share the repository configuration, which is exactly the part
we do. A clone (objects hardlinked from the local mirror, so it stays cheap) gives each
run its own `.git` entirely, and cleanup is `rm -rf` with no shared bookkeeping to
corrupt.

The run clone's `origin` points at the **local mirror path, not GitHub**. The provider
therefore has no network remote to push to even if it tries. Concurrency falls out for
free: runs are keyed by run ID, they share only a read-mostly package cache, and two
runs cannot collide on an issue because the claim already excluded that.

### The provider boundary

An adapter is invoked as a subprocess. The contract is deliberately small:

**In** — cwd is the run clone; `RUNE_BRIEF` is the path to the task brief (a file
*outside* the working copy, so it cannot leak into the diff); `RUNE_RUN_ID`,
`RUNE_ISSUE`, and `RUNE_LOG_DIR` locate the run. The environment is an **allowlist**,
not the maintainer's environment minus a few keys — `PATH`, a **per-run scratch
`HOME`**, the provider's own model credential, and the `RUNE_*` variables above.
Nothing else.

**Out** — the exit code is the only contract signal: `0` means "I am done, inspect the
tree." Everything the runner reports about the *outcome* it observes for itself. An
adapter may optionally write `$RUNE_RESULT` with provider-reported metadata (model,
tokens, turns); this is advisory, is labelled provider-reported in the run summary, and
is never trusted for an outcome.

**Timeout and cancellation** — a wall-clock deadline (default 45m, `--timeout` to
override). On expiry or on `release`/Ctrl-C, the runner sends `SIGTERM` to the
provider's **process group**, waits a grace period, then `SIGKILL`s the group. Adapters
run in their own process group precisely so a provider that spawns children cannot
outlive its run.

Built-in adapters: `claude` (Claude Code's non-interactive mode), `codex`
(`codex exec`), and `local` (a configured command — no model, harness, or vendor
prescribed). All three are the same ~20 lines around the same contract; a provider that
needs more than this contract is a provider we do not support.

An adapter may **not**: mutate GitHub, push, open a PR, merge, approve, or touch
anything outside its working copy.

### Enforcing the boundary — the part that is not a rule

"The provider may not push" is a sentence, and a sentence is not a boundary. The app's
private key sits at `~/.config/rune/rune-agent.pem`, readable by the same UID that runs
the provider, and `scripts/bot-pr.sh` is sitting right there in the working copy. Issue
#185 correctly calls this blocking. Two **independent** invariants close it, and both
are required — either alone is insufficient:

**(1) The provider cannot reach the credential.** The provider runs under a **separate,
unprivileged UID** (`systemd-run --uid=` / `sudo -u`, or a rootless container, which is
stronger and preferred where available). The key is mode `600` owned by the maintainer's
UID, so the provider's UID cannot read it. The env allowlist means `BOT_TOKEN`,
`GH_TOKEN`, `GITHUB_TOKEN`, and `RUNE_BOT_*` are **never exported into the provider's
environment**, and the scratch `HOME` means `~/.config/rune`, `~/.config/gh`, and the
maintainer's git credential helper are not on its path either. Under this invariant
`scripts/bot-pr.sh` remaining in the working copy is harmless: the provider can execute
it and it will fail, because the key is unreadable and no token is in the environment.
This is why the bot scripts do not need deleting — the exposure was never the scripts,
it was the key, and deleting the scripts would only have removed the most convenient
spelling of an attack that is ten lines of `openssl` to rewrite.

**(2) No credentialed command ever runs in a provider-controlled repository.** This is
the same structural rule ADR 0015 applies to CI — *the job holding the secret never
executes untrusted code* — and it is needed because invariant (1) protects the key, not
the runner. The provider's clone is a directory whose `.git/config`, `.git/hooks`, and
`Makefile` it may have rewritten. So: the runner runs every Git command in a run clone
with hooks disabled (`-c core.hooksPath=/dev/null`), and it **pushes from the trusted
mirror, never from the run clone** — it fetches the finished branch from the run clone
into the mirror (objects only; a commit cannot carry configuration) and pushes it from
there, where the config and hooks are the runner's own. Verification (`make verify`)
*does* execute provider-controlled code by construction, which is precisely why it runs
in the sandbox, under the provider's UID, with no credential in reach.

A `--unsafe-same-uid` escape hatch exists for machines that can offer neither a second
UID nor a container. It prints a warning, and it records `isolation: "same-uid"` in the
run summary so that a run made without the boundary is never silently indistinguishable
from one made with it. The env allowlist, the scratch `HOME`, the local-only `origin`,
and push-from-mirror still hold in that mode.

### CI-governance paths, now that the bot can write them

`workflows: write` is granted. The bot can edit `.github/workflows/`, which means it
can edit the checks that run against its own PR — including the ADR 0015 review of its
own diff. Removing a required job does not help an attacker (the required context never
reports, and the PR stays blocked), but *keeping the job's name and hollowing out its
body* produces a green `Engine` on a PR that ran no engine tests. Nothing in GitHub
prevents this. Only a human reading the diff does.

We keep the permission — RUNE has real `area:ci` work that an agent should be able to do,
and revoking it would push the runner into the "hand it to the maintainer" path for a
whole category of legitimate issues. The containment is therefore at the runner and at
review, and it is deliberately loud:

- A diff touching **CI-governance paths** — `.github/workflows/`, `.github/actions/`,
  `.github/rulesets/`, `.github/CODEOWNERS`, `Makefile`, `scripts/bot-*.sh` — is
  **refused at the diff-inspection gate**, before commit and push, unless the run was
  started with an explicit **`--allow-ci`**. The maintainer opts in per run; a provider
  cannot opt itself in, because the flag is consumed by the runner before the provider
  ever starts.
- A run that produces a CI-governance diff *without* `--allow-ci` terminates as
  `ci_change_refused` with the work preserved as a patch in the run directory. It is a
  legible outcome, not a crash, and it is resumable with the flag.
- A permitted CI change is **labelled `ci-change`** on the PR, gets a dedicated
  **"CI-governance changes"** section at the top of the PR body enumerating every
  touched path, and sets `ci_paths_touched` in the run summary. The point is that this
  can never arrive as an unremarked hunk in the middle of a 40-file diff.
- The runner records, per run, whether the **required check *definitions*** differ from
  base — the specific thing a reviewer must look at when `ci-change` is present.

This is a real tradeoff and we are taking the more permissive side of it: capability in
exchange for visibility, backed by the fact that CODEOWNERS covers `*` and every PR —
CI-touching or not — needs the maintainer's approval before it can merge.

### The task brief

Bounded, and built by **reference rather than by inlining**. The brief is the issue
(title, body, labels, milestone), the *titles and states* of blocking issues (not their
bodies), the branch and run ID, the verification command, and the prohibitions
(no GitHub mutations, no push, no PR, no secrets, CI paths off-limits unless
`--allow-ci`). It **points at** `AGENTS.md`, the nested `AGENTS.md` for the area it
touches, and `docs/coding-standards.md` by path — it does not paste them in. The
provider is a coding agent sitting inside the repository; it can open a file. Copying
the documentation into the prompt is how a task brief becomes 40 KB of context that
crowds out the actual issue, and it would *break* nested-`AGENTS.md` behavior by
flattening a mechanism the harnesses already implement. The runner must not synthesize a
system prompt that competes with `AGENTS.md`. The brief is capped (~8 KB); an
oversized issue body is truncated with an explicit pointer, never silently.

### Verification, diff inspection, and publication

In order, all by the runner, none of it trusting a provider's claim to have done it:

1. **Diff inspection.** Reject a no-op diff, changes outside the issue's plausible
   scope, committed secrets, generated directories (`target/`, `node_modules/`), and
   any evidence of a provider-created commit-push-PR. Apply the CI-governance gate.
2. **Verify** with the #184 contract (`make verify` — the whole `Engine` + `Client` +
   `E2E` + `cargo-deny` surface), in the sandbox, capturing **per-gate** results. A
   failure is a resumable terminal state, not a discarded run: the diff survives, the
   claim survives, `resume` re-enters it.
3. **Rebase onto current `origin/main`** per #183, since strict up-to-date-before-merge
   means a stale branch cannot merge. The branch is exclusively the runner's, so this is
   the one place `--force-with-lease` is legitimate — and it is a lease, never `--force`.
   **Re-verify after the rebase**, because a rebase can break a build that passed.
4. **Commit, push, open a draft PR** — the runner, never the provider. The push goes out
   from the trusted mirror, via `scripts/bot-push.sh`, to the *named remote* (#208: a
   bare URL creates no remote-tracking ref and silently robs `--force-with-lease` of its
   lease).
5. **The PR body maps every acceptance criterion in the issue to evidence** — the
   commit, the files, the test, the gate that proves it. Criteria the run did *not*
   satisfy are listed explicitly as unmet. A `Closes #N`-only body is a defect: the
   repository has a PR template and the mapping is the artifact a human reviews against.
6. **Then, and only then, `status:review`.**

### Run summaries: schema, surface, retention

The runner emits a **versioned, provider-neutral, sanitized** JSON summary per run —
`schema_version`, stable `run_id` (and `resume_of` for continuations), issue, provider
and adapter identity, `isolation` mode, branch, base/head SHAs, timestamps for every
lifecycle transition, issue area (from labels) and size (from diff buckets), per-gate
verification results, `terminal_outcome`, a **normalized `failure_stage`**
(`preflight | claim | brief | provider | diff_inspection | verify | rebase | push | pr`),
`ci_paths_touched`, and — because of consequences (2) and (3) — the **PR author login**
and **whether the ADR 0015 review actually ran** (observed via the `actions`/`checks`
read permissions the app already holds, not assumed from a green checklist).

**Runner-observed vs provider-reported** is a first-class distinction in the schema.
Outcome, gates, diff stats, PR identity, and review-ran are observed by the runner and
are authoritative. Model, tokens, cost, and turns are provider-reported, optional, and
explicitly non-comparable across providers.

The **audit surface is an orphan branch, `agent-runs`**, holding one JSON file per run
at `runs/<issue>/<run-id>.json`, written via the Git Data API with the bot's `contents`
permission. It is in the repository (so #200 does not depend on undocumented files in
one maintainer's checkout), it is off `main` (so telemetry never touches the reviewed
history or triggers CI), and one-file-per-run means concurrent runs cannot conflict on
content. The local run directory is the write-ahead log; the branch is written at
durable transitions and at the terminal outcome. Records are **append-only**: a
correction is a *new* record that `supersedes` an earlier `run_id`, never a rewrite, so
the audit trail cannot be quietly edited. Retention is indefinite (records are ~1 KB).

Never in a summary: prompts, briefs, source diffs, secrets, environment values, model
reasoning, or provider logs. Provider logs stay local, and are **redaction-scrubbed**
(`ghs_`, `ghp_`, `github_pat_`, PEM blocks) before they are written even there.

A run summary is a record. It confers **no authority**: it cannot approve, cannot merge,
and cannot select a provider automatically. #200 reads it; nothing acts on it.

### The terminal invariant

> A successful run ends at **a draft PR, open, authored by `rune-agent[bot]`, with the
> required checks green** — never approved, never merged, never self-reviewed.

`main-protection` still carries an Admin bypass (`bypass_mode: always`), so this is
enforceable-in-principle rather than mechanically guaranteed; the runner's own
credentials cannot bypass the ruleset, and the runner never asks.

## Consequences

- **Easier.** One command per issue, and the provider becomes the only variable in the
  system. Claiming, isolation, verification, rebasing, and publication behave
  identically whether the work was done by Claude Code, Codex, or a local model, so
  swapping providers is no longer swapping governance. Concurrent runs stop being
  dangerous, a failed run stops being a lost run, and #200 gets a real telemetry
  substrate instead of anecdotes. #187 reuses the adapter boundary rather than inventing
  a second one.
- **Harder / given up.** The runner is now the single point of failure for all agent
  work, and it is a genuinely stateful program: worktrees, claims, partial GitHub
  mutations, and resumability are the hard parts, and they are hard in every
  implementation. Per-run clones mean cold build caches, which is a real wall-clock cost
  paid on every run in exchange for the isolation. The separate-UID requirement is
  friction on a fresh machine, and the `--unsafe-same-uid` hatch will be tempting — it is
  recorded in the summary precisely because it will be used. The `agent-runs` orphan
  branch is one more thing to not accidentally garbage-collect.
- **Security posture is a boundary now, not a rule.** The provider cannot reach the key,
  and no credentialed command runs in a directory it controlled. That closes the gap
  #185 flagged as blocking. It does *not* make the provider trusted: verification
  executes its code by construction, which is why that happens in the sandbox.
- **The `workflows: write` tradeoff is taken knowingly.** The bot can rewrite the CI that
  gates it. We accept that in exchange for agents being able to do CI work at all, and we
  pay for it with an explicit per-run opt-in, a loud label, a dedicated PR-body section,
  and a recorded diff of the required-check definitions. The residual risk — a hollowed-
  out job that keeps a required check's name — is caught by human review or not at all.
  If that ever feels too thin, the mitigation is to revoke `workflows: write` and restore
  a `needs-human-push` terminal state, which is why consequence (2) above is written down.
- **The bot scripts stay.** `bot-token.sh`, `bot-push.sh`, and `bot-pr.sh` are not
  superseded by the runner — they *become* its credential layer (invoked from the trusted
  parent, never from the provider's sandbox) and they remain the supported manual path for
  interactive agent sessions, which will keep existing alongside the runner. #186 extends
  `bot-push.sh` with an explicit-refspec mode (the runner pushes from a bare mirror, which
  has no "current branch") and `bot-pr.sh` with `--head`/`--draft`; it does not replace
  them. What *is* superseded is `scripts/local-agent.sh.example` and
  `docs/agents/local-ai-setup.md`: the `local` adapter is their replacement, and unlike
  them it does not push as the maintainer. #186 removes both, once there is something to
  remove them in favour of.

### PR-sized implementation slices (acceptance criteria for #186)

Building the runner is out of scope here; #186 implements it, and its existing criteria
are refined by this ADR where the two differ. It should land as these PRs, in order —
each independently reviewable, each green on `make verify`:

1. **Skeleton + claim + lifecycle.** `scripts/agent-task` (Node 20, ESM, zero deps) with
   `doctor`, `start` through the claim, `status`, `list`, `release [--force]`,
   `cleanup`. Atomic claim via `POST git/refs` (422 ⇒ lost). Labels and the claim
   comment. Preflight rejects closed, blocked, dependency-blocked, malformed, or
   already-claimed issues *before* any mutation. Tests with the GitHub API faked at the
   `fetch` boundary.
2. **Sandbox + adapter contract.** Runner-owned mirror; per-run clone with local-only
   `origin`; separate-UID/container execution with the env allowlist, scratch `HOME`, and
   `--unsafe-same-uid` recorded in the summary; process-group timeout and cancellation;
   redaction-scrubbed logs; the `claude`, `codex`, and `local` adapters; the bounded task
   brief. A test proving a provider **cannot** read the key or mint a token, and that
   `git` in the run clone runs with hooks disabled.
3. **Gates + publication.** Diff inspection (no-op, out-of-scope, secrets, generated
   dirs, provider-created pushes), the CI-governance gate and `--allow-ci` with the
   `ci_change_refused` outcome, `make verify` per-gate capture, rebase-onto-`main` with
   `--force-with-lease` and re-verification, push from the mirror via `bot-push.sh`
   (plus its refspec mode), draft-PR creation via `bot-pr.sh` (plus `--head`/`--draft`),
   the acceptance-criteria-to-evidence PR body, the `ci-change` label and PR section, and
   `status:review` only after the PR exists.
4. **Run summaries + resume.** The versioned schema, the `agent-runs` orphan branch with
   append-only supersede-on-correction, runner-observed vs provider-reported field
   separation, PR-author and review-actually-ran capture, normalized failure stages,
   `resume`, and stale-claim takeover. Removes `scripts/local-agent.sh.example` and
   `docs/agents/local-ai-setup.md`, and points `docs/agents/workflow.md` at the runner.
