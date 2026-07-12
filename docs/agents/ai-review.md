# The AI reviewer

Every pull request gets an independent AI review before a human spends attention on it
([ADR 0015](../decisions/0015-independent-ai-pr-review.md)). This document is how to operate
it: what it can and cannot do, how to configure it, and what to do when it breaks.

**Two things it is not.** It is not an approver — it posts a `COMMENT` review and has no
approval or merge authority, and no AI review, positive or negative, can satisfy the human
approval requirement. And it is not a gate on your code: the `AI Review` check reports only
that **the review ran**, not whether it liked what it saw. Findings are advisory.

## The shape, and why it has that shape

```text
  pull_request                                     workflow_run (Prepare succeeded)
       │                                                        │
┌──────▼───────────────────────────┐            ┌───────────────▼──────────────────┐
│ AI Review Prepare   (UNTRUSTED)  │  artifact  │ AI Review          (TRUSTED)     │
│                                  │ ─────────▶ │                                  │
│ • sees PR-controlled files       │  manifest  │ • holds the model credential     │
│ • contents: read, NO SECRETS     │  + hashes  │ • pull-requests + checks: write  │
│ • executes nothing from the PR   │            │ • NEVER checks out the PR head   │
│ • reads files, diffs, hashes     │            │ • verifies hashes and provenance │
│ • base-ref code and base-ref     │            │ • one bounded model call, NO     │
│   constraint docs only           │            │   tools, then publishes          │
└──────────────────────────────────┘            └──────────────────────────────────┘
```

The split exists for exactly one reason: **the job holding the credential must never execute
code the pull request controls.** That is the whole threat model. A single workflow using
`pull_request_target` — the obvious way to build this — runs with a read-write token and the
repository's secrets in the base context *while checking out the PR head*, which is the
standard secret-exfiltration path for "AI reviews your PR" workflows. It is not used, cannot
be used, and `make ci-lint` now rejects it repository-wide (#199).

Three consequences worth internalizing before editing either file:

- **Prepare is allowed to see hostile input because it has nothing to steal.** No secrets, no
  write scopes. If you ever add a secret to it, the design is void.
- **Prepare runs the base branch's `prepare.js`** — but *not* its workflow file. This is the
  subtlety worth understanding, because the obvious reading is wrong: a `pull_request` workflow
  executes the YAML **from the pull request's own head**. So a PR *can* rewrite
  `ai-review-prepare.yml`, and produce a well-formed, correctly-hashed artifact that simply lies.
  Provenance checks cannot catch that — the artifact really did come from our workflow name, in
  our repository, for that head SHA. The contents were forged, not the envelope.

  The trusted stage therefore re-establishes independently what it can, and does not take the
  untrusted stage's word for any of it:
  - the **constraint documents are re-read from the trusted job's own base checkout**, and the
    artifact's copies are discarded. The rules a change is judged against come from `main`,
    inside the job that holds the credential.
  - the **file list is cross-checked against the GitHub API**, which the pull request cannot
    rewrite. An artifact that hides a file is refused.

  What a hostile PR *can* still do: make its own review fail (a red check on itself), and corrupt
  the patch text of a file it legitimately touches. What it cannot do: make its review falsely
  clean by rewriting the rules, hide a file from the reviewer, or reach a secret — prepare has
  none to reach.
- **The reviewer has no tools.** Not "tools that are disabled" — the model request carries no
  tool definitions at all. A prompt injection in the diff can therefore make the review *wrong*;
  it cannot make the reviewer run a command, read a secret, or push a commit. The prompt also
  tells the reviewer that an injection attempt is itself a high-severity security finding.

`tools/ai-review/workflow-security.test.js` asserts every one of these properties against the
workflow files themselves, so a careless edit fails a test instead of silently reopening the
hole.

## Configuration

| Setting | Where | Notes |
|---|---|---|
| `CLAUDE_CODE_OAUTH_TOKEN` | Actions secret | **The default provider's credential.** Produced by `claude setup-token` and covered by a Pro/Max subscription — the reviewer runs the Claude Code CLI rather than the metered API, so a review costs subscription usage, not per-token billing. Only ever read by the **trusted** stage. |
| `ANTHROPIC_API_KEY` | Actions secret | Only if you set the provider to `anthropic` (the raw Messages API — metered, and the *strongest* no-tools guarantee; see below). |
| `OPENAI_API_KEY` | Actions secret | Only if you set the provider to `openai`. |
| `RUNE_REVIEW_PROVIDER` | Actions **variable** | `claude` (default), `anthropic`, `openai`, or `local`. A variable, not a secret — which provider reviewed a PR is not sensitive, and it is recorded in the check summary. |
| `AI Review` | required check | In [`.github/rulesets/main.json`](../../.github/rulesets/main.json). Applying it is a repository setting — see below. |

### The one place the two provider kinds differ

`anthropic`/`openai` are **raw HTTPS requests**: the request carries no tool definitions, so there
is no tool loop to escape from. That is the strongest form of "the reviewer has no tools", and it
costs metered API tokens.

`claude` is the **Claude Code CLI in print mode** — what a Pro/Max subscription can actually
authenticate. The CLI *is* an agent harness, so here "no tools" is **enforced rather than
structural**: every built-in tool is denied, MCP is empty and strict, it runs in an empty scratch
directory with a scratch `HOME`, and its environment is an allowlist containing no `GITHUB_TOKEN`.
On top of that, the adapter **refuses any result that took more than one turn** — a tool call costs
a turn, so the turn count is a runtime observation of the property rather than a hope. Switching to
`anthropic` is one repository variable if you ever want the structural version instead.

**No provider is canonical.** The adapter boundary is the same idea as
[ADR 0016](../decisions/0016-provider-neutral-issue-runner.md)'s, applied to a role that must be
*weaker* than a coding provider: one bounded HTTPS request, no tool loop. `local` prescribes no
model, harness, or vendor — point `RUNE_REVIEW_CMD` at any command that reads a prompt on stdin
and writes the model's reply to stdout.

**Cost is bounded.** The diff and context are size-capped (`CAPS` in `tools/ai-review/config.js`),
output is capped, and a single job makes at most 3 model calls (retries included). A head SHA that
has already been **successfully** reviewed is never reviewed again, so re-running a *green* check
is free and a new push gets a fresh, independent review. Re-running a *failed* one does spend
another (up to) 3 calls — which is the point of re-running it, but it means a provider outage that
you retry repeatedly is the one way to spend real usage on nothing.

## Reading a review

- **Findings are advisory.** Weigh them; do not obey them. The reviewer is uncalibrated (ADR
  0015's measurement window is not complete, and #244 is what makes completing it possible), so
  it produces false positives and confident misses. Making any finding category merge-blocking
  requires a **new, human-approved ADR** — it cannot happen through configuration drift.
- **An empty review is not a passing grade.** It means one uncalibrated opinion found nothing.
- **A truncated review says so**, in both the PR comment and the check summary. A review that
  saw 300 of 900 changed files must never be mistaken for one that saw the whole change.
- **Findings are neutralized before posting.** The model read a diff someone else wrote, so its
  output can echo attacker-controlled text; markup, pipes, fences, and `@mentions` are defused
  so a finding cannot lie to you about what it is.

## When it breaks

**The check is red.** That is the design: an infrastructure failure — provider outage, timeout,
retries exhausted, a rejected artifact, a model reply that would not parse — **fails** the
`AI Review` check. A skipped or failed review must never be indistinguishable from a clean one,
which is precisely the flaw of the interim reviewer this replaces. The check summary says which
it was.

| Symptom | Cause | Do |
|---|---|---|
| `Infrastructure failure` in the check summary | provider outage or timeout | Re-run the failed `AI Review` job. The head SHA has not been reviewed, so it will actually review. |
| `stale head` | you pushed while the review was queued | Nothing. The push started a new prepare run; that one will publish. |
| `artifact was prepared by reviewer X` | prepare and review ran different commits of the tool | Re-run both jobs from the current head. |
| Check never appears | the prepare run failed, or `workflow_run` did not fire | Look at the **AI Review Prepare** run. `workflow_run` only fires for workflows on the default branch — a PR that *adds* the workflow will not trigger it until it merges. |
| Review is nonsense | an uncalibrated model | Record the disposition (#244) and move on. This is expected and is the reason findings are advisory. |

**Emergency disablement.** Remove `AI Review` from the required checks in the repository
ruleset (Settings → Rules → Rulesets), which unblocks merges immediately. Do **not** disable it
by deleting the workflow: `AI Review` is a required context, and a required context that never
reports leaves every PR blocked rather than unblocked. Removing the requirement is the switch;
the workflow can stay.

## Repository settings (cannot be committed)

`.github/rulesets/main.json` is the reviewable source of truth, but GitHub enforces rulesets
from repository settings, not from a file. After this merges:

```sh
# Verify the check is actually required — not merely written down as required.
gh api repos/ninthworld/rune/rulesets --jq \
  '.[] | select(.name=="main-protection") | .id' |
  xargs -I{} gh api repos/ninthworld/rune/rulesets/{} --jq \
  '[.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks[].context]'
```

It must list `AI Review` alongside `Engine`, `Client`, `E2E`, and `cargo-deny`. Re-import the
ruleset (Settings → Rules → Rulesets → the `main-protection` ruleset) if it does not — and note
that adding a required check that never reports blocks every PR, so add it **after** the first
`AI Review` run has published successfully on a real PR.
