# ADR 0015: Independent AI code review after PR submission

- Status: accepted
- Date: 2026-07-12
- Issue: #182

## Context

RUNE is built primarily by AI agents across short-lived sessions (ADR 0001), and
its governance rests on two load-bearing gates: `make check` (the `Engine` +
`Client` unit gate) plus the separate `E2E` job (ADR 0011), and a **mandatory
human review** before merge (`docs/agents/workflow.md`: "A human reviews and merges
— always"; branch protection requires one human approval, authors cannot approve
their own PRs). Those gates catch what a compiler, a test suite, and one human
reviewer catch. They do not systematically catch the failure mode this project is
most exposed to: an implementation agent that produces green, plausible-looking code
which quietly violates a hard rule (`AGENTS.md`: zero game logic in the client, zero
I/O in the engine, protocol changes are contract changes), regresses behavior the
tests don't pin, ships a security mistake, or omits the tests the change needed. The
agent that wrote the diff is the worst-placed party to catch its own blind spot,
because it shares the context and assumptions that produced the miss.

An independent AI reviewer — one that sees only the final diff and the repository's
own constraints, with none of the implementing agent's session context — is a cheap,
tireless second opinion aimed squarely at that gap. But adding it is not free, and it
is deliberately **decoupled from the human-review requirement**: human approval is a
deterministic governance rule that stands on its own, while an AI reviewer drags in
vendor choice, cost, permissions, prompt-injection exposure, and false-positive
calibration. Coupling the two would let a decision about an unproven, probabilistic
tool ride on top of a settled governance guarantee. This ADR decides the AI reviewer
in isolation; it changes nothing about human approval.

The forces that constrain the decision are the project's own rules and the security
reality of running review tooling against agent- and (eventually) contributor-authored
diffs:

- **The security boundary is the hard part, not the model call.** GitHub's
  `pull_request_target` trigger runs with a **read-write token and repository secrets
  in the base-repo context** while checking out the PR head. Any step that then
  *executes* PR-controlled code — `npm install`, a build, a doctored `Makefile`, a
  malicious `build.rs` — can exfiltrate those secrets. This is the dominant, well-
  documented risk of "AI reviews your PR" workflows, and issue #182 names it as a
  required criterion. The design must make it structurally impossible for untrusted
  PR code to touch write credentials or the model API key.
- **Repo-owned and auditable beats a black box.** RUNE keeps its governance in the
  repository where agents and humans can read it: rules in `AGENTS.md`, decisions in
  ADRs, CI in `.github/workflows/`. A reviewer whose prompt, permissions, timeout, and
  threat model live in the repo can be inspected, versioned, and cited the same way.
  A managed SaaS bot hides all of that behind a vendor.
- **Provider neutrality is already the stated direction.** The sibling decisions #185
  (provider-neutral issue runner) and #187 (stewardship cycle) both call for a
  provider-neutral adapter contract so Claude, Codex, or a local model are
  interchangeable and no vendor owns workflow state. An AI reviewer that hard-wires one
  vendor's GitHub App would contradict that direction; the reviewer should be one more
  consumer of the same adapter boundary.
- **An unproven probabilistic check must not silently become a merge gate.** LLM
  reviewers hallucinate, narrate style, and miss real defects. Until this one's
  false-positive/false-negative behavior is measured on RUNE's actual diffs, its
  *findings* cannot be allowed to block merges or lull the human reviewer into rubber-
  stamping. What can be enforced from day one is only that the review **ran**.

## Decision

RUNE adds an **independent AI code-review step that runs on every pull request**,
implemented as a **repository-owned GitHub Actions workflow calling a provider-neutral
review adapter**, producing **both a PR review and a check-run summary**, where the
`AI Review` check is **required to run and complete but its findings are advisory**
(non-blocking) until calibration data justifies promoting specific finding categories
to merge-blocking. Human approval, `Engine`, `Client`, and `E2E` remain exactly as
they are. The rules below are what the codebase and CI will follow; **building the
workflow and adapter is out of scope for this ADR** (it lands as the follow-up task
below) — this decision is what that task builds to.

### Chosen approach: a repository-owned workflow, not a managed integration

The reviewer is a workflow under `.github/workflows/` that RUNE owns end to end: its
trigger, permissions, timeout, retry policy, and — critically — the **prompt and the
context it feeds the model** all live in-repo, reviewed like any other change. This is
option 3 in issue #182, chosen over option 1 (a managed GitHub reviewer/SaaS bot) and
option 2 (a single-vendor Claude GitHub App) because only a repo-owned workflow lets
RUNE fix the security boundary itself, feed the model RUNE's own constraints, bound
cost and permissions explicitly, and stay provider-neutral. A vendor GitHub App is not
rejected as a *provider* — it may sit behind the adapter — but it is rejected as the
*owner* of the enforcement, prompt, and threat model.

### Independence: the reviewer shares no context with the author

The reviewer runs in a **fresh invocation with no access to the implementing agent's
session, conversation, branch history rationale, or scratch state**. Its entire input
is the final PR diff plus the repository's own constraint documents (below). It must
be a **separate invocation from the one that produced the diff**, and should prefer a
**different model or provider** from the implementer where the runner configuration
makes both available, so a systematic blind spot in one model is less likely to be
shared by its own reviewer. Independence is a property of *context isolation first*
and *model diversity second*; the adapter contract (#185/#186) is the seam that makes
both configurable.

### What the reviewer sees: the diff plus RUNE's own constraints

The review input is assembled from **trusted, base-ref sources only** (see the security
boundary below) and consists of:

- the **final PR diff** (base…head), the artifact humans review;
- the **applicable `AGENTS.md` files** — root and any nested ones (`crates/rune-engine/`,
  `clients/web/`) whose directory the diff touches — so the reviewer checks against the
  hard rules that actually govern the changed code;
- **`docs/coding-standards.md`**, and the **ADRs / `docs/protocol.md` / test
  conventions** relevant to the changed area (e.g. protocol changes are contract
  changes; a rules change needs a CR-cited regression test per `docs/coding-standards.md`
  and `docs/rules-coverage.md`).

The reviewer is instructed to **focus on defects, regressions, security issues,
architecture-rule violations, and missing or inadequate tests — not style narration.**
Formatting and lint are already `make check`'s job; a reviewer that restates them is
noise. Each finding must name a concrete location and a concrete risk.

### Security boundary: a two-stage split so secrets never meet untrusted code

The workflow is split into two jobs so that **the job holding credentials never
executes PR-controlled code**, which is the only robust defense against the
`pull_request_target` exfiltration class:

1. **Prepare (trigger: `pull_request`, `permissions: contents: read`, no secrets).**
   Runs in the untrusted context with a **read-only token and no secrets whatsoever**.
   It computes the diff and gathers the constraint documents **by reading files, never
   by executing them** — no `npm install`, no build, no running the PR's scripts — and
   uploads the diff + gathered context as a workflow artifact. Even though this job
   sees PR-controlled file contents, it has nothing worth stealing.
2. **Review (trigger: `workflow_run` on Prepare's completion, `permissions:
   pull-requests: write`, holds the model API key).** Runs in the **trusted base-repo
   context**. It downloads the Prepare artifact and calls the review model on the diff
   **text**; it **never checks out or executes the PR head**. It then posts the PR
   review and the check-run summary. Because it only ever handles inert diff text, the
   API key and write token are never exposed to untrusted code.

This split is deliberately chosen over the single-workflow `pull_request_target`
pattern precisely because that pattern is the thing the threat model has to defend
against. `pull_request_target` is **not used**. The model call requires only the diff
text and static constraint docs — it never needs to run the change — so there is no
reason to ever put untrusted execution and secrets in the same job.

### Bounded cost, permissions, timeout, and retry

- **Permissions** are least-privilege per job: Prepare is `contents: read`; Review adds
  only `pull-requests: write` (to post the review/summary) and `checks: write` (to set
  the check run). Neither job gets `contents: write` or any deploy/package scope.
- **Cost** is bounded per run: the diff and context handed to the model are **size-
  capped**, with very large diffs summarized or truncated with an explicit note in the
  review rather than fanned out into unbounded token spend. One review pass per PR head
  SHA; pushing new commits re-reviews the new SHA, but the same SHA is not re-reviewed.
- **Timeout** is explicit (a `timeout-minutes` on the Review job) so a hung provider
  call fails the *run* fast instead of holding a slot.
- **Retry** is bounded: transient provider/network errors retry a small fixed number of
  times with backoff; after that the check reports an **infrastructure failure**
  distinct from "review completed with findings." A provider outage must be legible as
  an outage, not silently pass as a clean review.

### Enforcement: the review must run; its findings are advisory

- **Required to run.** `AI Review` is a **required check** in the sense that the job
  must complete successfully — the review actually executed and a result was posted.
  This is what stops the step from being quietly skipped or from an outage masquerading
  as approval. An infrastructure failure (provider down, timeout, retries exhausted)
  **fails the check** and is re-run like any other flaky infra failure.
- **Findings are advisory.** A completed review that *contains findings* does **not**
  fail the check and does **not** block merge. Findings are posted for the human
  reviewer to weigh. The merge-blocking required checks remain exactly `Engine`,
  `Client`, `E2E` (per its own governance), and **one human approval**.
- **Human approval is untouched and cannot be replaced.** No AI review, positive or
  negative, can approve a PR, dismiss a human's review, or satisfy the human-approval
  requirement. The AI reviewer has **no merge permission and no approval authority** —
  it comments, nothing more. This ADR does not alter branch protection.

### Calibration and the condition for making findings merge-blocking

Findings stay advisory until we have measured this reviewer on RUNE's real diffs:

- **Measurement window.** Over an initial window (target: at least ~30 reviewed PRs, or
  a fixed calendar span, whichever the maintainer sets when the workflow lands), each
  AI finding is labeled by a human as **true positive**, **false positive**, or
  **noise/style**, and each merged PR is checked for defects the reviewer **missed**
  (false negatives), using post-merge bug reports and human-review catches as signal.
- **Promotion condition.** A **specific, narrow category** of findings may be promoted
  to merge-blocking — via a new decision/ADR, not silently — only once that category's
  measured **false-positive rate is low** (target guidance: ≤ ~10% for the promoted
  category) **and** its recall on real defects is worth the friction. The natural first
  candidates are **objective, low-ambiguity hard-rule and security violations** (e.g.
  game logic detected in `clients/web/`, I/O introduced in `rune-engine`, a protocol
  shape changed without `docs/protocol.md`) rather than subjective design opinions.
- **No auto-promotion.** Promotion is a human decision recorded in an ADR; the workflow
  never escalates its own authority based on its own stats.

### Where the result shows up

The reviewer posts **both**: a **PR review** (a non-approving `COMMENT` review, so
findings sit inline in the conversation where human reviewers read) **and** a **check-
run summary** (a compact rollup — counts by severity, the infra/completed status —
surfaced in the checks UI). Both, because the PR review is where a human engages with
specifics and the check summary is where merge-gate status is read at a glance.

## Consequences

- **Easier.** Every PR gets a tireless, context-isolated second reviewer aimed exactly
  at RUNE's highest-risk failure mode — plausible green code that violates a hard rule,
  regresses untested behavior, or skips the tests it needed — before a human spends
  attention on it. Because the workflow, prompt, permissions, and threat model live in
  the repo, they are auditable and versioned like any other decision, and the provider-
  neutral adapter (shared with #185/#187) keeps RUNE from being married to one vendor.
  The human reviewer keeps full authority and now reads with a checklist of concrete,
  located concerns in hand.
- **Harder / given up.** A new external dependency and cost center enters CI: model API
  calls, a secret to manage, and a probabilistic step whose output must be read
  critically. The two-stage `pull_request` → `workflow_run` split is more moving parts
  than a single job and is easy to get subtly wrong — the whole security guarantee
  rests on the Review job never executing PR code, which must be preserved in every
  future edit to that workflow. LLM reviews will produce false positives and confident
  misses; until calibrated they add reading load without a hard guarantee, and there is
  a real risk of humans over-trusting a green AI review — which is exactly why findings
  start advisory and human approval is explicitly irreplaceable.
- **Governance — no change to existing gates.** This ADR **adds** a check; it does not
  weaken any. Human approval, `Engine`, `Client`, and `E2E` are untouched, and the AI
  reviewer has no merge or approval power. Making any AI finding merge-blocking is
  gated behind the calibration condition above and a future ADR — it cannot happen by
  configuration drift.
- **Deferred (the follow-up implementation task).** Building the workflow is out of
  scope here and lands as a separate PR-sized `agent-task`, with these concrete
  acceptance criteria:
  - A `.github/workflows/ai-review.yml` implementing the **two-stage split**: a Prepare
    job (`pull_request`, `contents: read`, **no secrets**, gathers diff + applicable
    `AGENTS.md`/ADR/protocol/standards context by reading files only, uploads an
    artifact) and a Review job (`workflow_run`, `pull-requests: write` + `checks:
    write`, holds the model key, **never checks out or executes PR head**, calls the
    review adapter on the diff text, posts a non-approving PR review **and** a check-run
    summary).
  - The Review job enforces an explicit `timeout-minutes`, a bounded retry-with-backoff
    on transient provider errors, a **size cap** on the model input with explicit
    truncation notes, and one-review-per-head-SHA (no duplicate reviews for an unchanged
    SHA).
  - The review adapter conforms to the provider-neutral contract from #185/#186 (Claude,
    Codex, or a configured local model selectable by config; no vendor hard-wired) and
    runs as a **fresh invocation with no implementing-agent session context**.
  - The reviewer prompt targets defects, regressions, security, architecture-rule
    violations, and missing tests, and is instructed **not** to narrate style/lint.
  - `AI Review` is added as a **required-to-complete** check while findings remain
    **advisory** (no finding fails the check or blocks merge); an infrastructure failure
    fails the check.
  - A short calibration note (window, labeling procedure, promotion threshold) is added
    under `docs/agents/` so the advisory-to-blocking transition is measurable and
    human-gated.
  - Secret handling and the `pull_request_target`-avoidance rationale are documented
    alongside the workflow; `make check` stays green (the follow-up touches CI/docs, not
    engine/client source).
