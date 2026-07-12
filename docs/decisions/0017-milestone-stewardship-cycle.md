# ADR 0017: Repeatable milestone stewardship cycle

- Status: accepted
- Date: 2026-07-12
- Issue: #187

## Context

`docs/roadmap.md` is reconciled by hand: someone reads the milestone's exit criteria,
skims closed issues, ticks boxes, and writes the next milestone's feature table. When
that someone is an AI agent doing it in one unverified pass — audit the milestone,
decide it's done, decompose the next one, and write issues to GitHub — the whole
sequence has no checkpoint. A bad exit-criterion call, a missed gap, or an over- or
under-decomposed next milestone all ship silently, because nothing in the sequence is
independently checked and nothing requires a human to actually decide anything. This
is the same failure class ADR 0015 named for PR review (the agent that produced a
judgment is the worst-placed party to catch its own blind spot) and ADR 0016 named for
GitHub mutations (a provider must not own workflow state) — applied here to a task
that is strictly higher-stakes than either: closing a milestone and creating a wave of
issues both change what the *project* believes is true, not just what one PR contains.

The trigger for writing this down now is mundane and recurring: **RUNE periodically
runs out of ready issues.** M1 and M2's engine tracks shipped; M3's table (#146–#160)
was hand-decomposed and filed in one sitting. The next time the ready queue empties —
at the end of M3, and at the end of every milestone after it — the same decomposition
work has to happen again, and today nothing but a maintainer's attention stops it from
happening as a single unverified pass.

Three decisions already made constrain this one and are not reopened here:

- **ADR 0015** established independent AI review with no shared context as the pattern
  for catching one model's blind spots, findings-are-advisory until calibrated, and
  human approval as the irreplaceable gate. This ADR reuses that pattern for audit and
  plan review rather than inventing a second one.
- **ADR 0016** established the provider-neutral adapter contract (a subprocess that
  edits or emits inside a bounded contract, and never touches GitHub, credentials, or
  anything outside its working copy), the runner-observed-vs-provider-reported split,
  the sanitized versioned summary schema, and the `agent-runs` orphan branch as the
  durable, off-`main`, append-only audit surface. This ADR is a **second consumer of
  that same adapter boundary and the same telemetry conventions** — not a parallel
  design for provider invocation or run summaries. #200 is the reader of both.
- **This ADR's implementation is out of scope here**, exactly as ADR 0016 deferred
  building the runner. What follows is the decision the follow-up `agent-task` issue
  builds to, with PR-sized acceptance criteria at the end.

## Decision

RUNE adopts an explicit, mostly-deterministic **state machine** for milestone
stewardship, with exactly four AI roles (each a provider-neutral adapter invocation per
ADR 0016), two human approval gates, and one durable, sanitized event record per cycle.
No step past evidence collection is a single unchecked pass; no step that changes what
the project believes is true (closing a milestone, creating issues) happens without an
explicit human command.

```text
collect evidence (deterministic)
 → audit current milestone (AI: Auditor)
 → independently review audit (AI: Audit Reviewer, fresh context)
 → HUMAN GATE: approve/reject closeout
 → [if approved] plan next milestone (AI: Planner)
 → validate issue manifest (deterministic)
 → independently review manifest (AI: Plan Reviewer, fresh context)
 → HUMAN GATE: approve (all/subset)/reject plan
 → [if approved] create issues (deterministic, idempotent)
 → open roadmap reconciliation PR (deterministic)
 → human merges the PR and the cycle is closed
```

Everything marked "deterministic" is a script, not a model call — reused directly from
the ADR 0016 runner's own primitives (GitHub API reads/writes, `make verify`, `grep`,
`git`). Everything marked "AI" is one provider-neutral adapter invocation with a bounded
brief, exactly like an ADR 0016 provider: it receives a read-only checkout at a pinned
commit and a brief, and it returns a structured artifact. **No AI role may mutate
GitHub, push, or write outside its own working copy** — identical to the ADR 0016
provider boundary. Only the deterministic Evidence Collector (reads) and Applier
(the two explicitly human-gated writes) touch GitHub.

### When a cycle may start

- **Automatic — empty ready queue.** The milestone named on the milestone's own open
  issues has zero issues labeled `status:ready`. This is the actual symptom the issue
  names ("running out of ready issues"), so it is the primary trigger regardless of
  whether blocked issues remain — a blocked issue is by definition not something an
  agent can pick up next, so its presence doesn't change what "nothing to start" means.
- **Automatic — no open leaf issues.** The milestone has zero open issues at all
  (neither `status:ready` nor `status:blocked` nor in-progress). This is the
  unambiguous case: no ready queue is not empty *because* everything is done.
- **Manual — explicit maintainer request.** A maintainer names a milestone and invokes
  the start command directly, regardless of queue state. This is the only way to force
  an early audit (e.g., to check progress mid-milestone) or to re-run a cycle sooner
  than the automatic conditions would.
- **Blocked-queue behavior.** When the ready queue is empty but blocked issues remain,
  the cycle **still starts** — evidence collection and audit are cheap and largely
  mechanical, and the audit itself is often exactly what surfaces *why* something is
  blocked (a missing decision, a wrong dependency edge, scope that quietly grew). But
  the human closeout gate (below) is expected to reject closing the milestone in this
  state, and a **rejected closeout does not attempt to plan the next milestone** — the
  cycle terminates at "not ready," posts the audit to the planning issue, and stops.
  Planning the next milestone while the current one is known-incomplete would let the
  cycle quietly redefine "done" by moving on instead of surfacing the gap.
- **No auto-retrigger storm.** A cycle that ended at a rejected closeout does not
  restart itself on the next check against unchanged state — it re-triggers only when
  the base commit SHA has moved (new evidence exists) or a maintainer explicitly
  re-invokes it. This is a scoping decision, not a schema field: the start-condition
  check is expected to compare against the `cycle_id` and `terminal_at` of the most
  recent cycle summary for the milestone (below) before opening a new planning issue.

### Deterministic evidence collection

The **Evidence Collector is a script, not a model call**, run against one pinned
`base_commit_sha` (current `origin/main` at cycle start) so every downstream artifact
cites evidence from a single, fixed point in time. It produces an **Evidence Bundle**:

- `schema_version`, `cycle_id`, `milestone`, `base_commit_sha`, `collected_at`.
- `exit_criteria`: the verbatim checklist text and current checkbox state for the
  milestone, read from `docs/roadmap.md` — the criteria are quoted, never
  paraphrased, so the Auditor is auditing the actual sentence a human wrote.
- `issues` / `prs`: number, title, state, labels, milestone, and (for PRs) merge SHA
  and `Closes #N` linkage, for everything tagged to the milestone.
- `ci`: the required-check results (`Engine`, `Client`, `E2E`, `cargo-deny`) for the
  relevant merged PRs, and a **fresh `make verify` run against `base_commit_sha`
  itself** — CI history for old PRs proves those PRs passed at the time, not that
  `main` still passes today.
- `tests`: pass/fail counts per crate/suite (counts only — not full test output).
- `rules_coverage`: the `docs/rules-coverage.md` rows whose CR citation falls in the
  milestone's stated scope.
- `adr_protocol_state`: status (`proposed`/`accepted`/`superseded`) of every ADR the
  milestone's exit criteria name, and whether `docs/protocol.md` documents the shapes
  those criteria require.
- `todos_and_stubs`: a grep sweep (`TODO`, `todo!`, `unimplemented!`, `dbg!`-adjacent
  markers, and known-stub comments) scoped to the paths the milestone's criteria touch,
  each as `file:line`.
- `documented_gaps`: existing "Partial: …" / known-gap prose already in
  `docs/roadmap.md`'s "Where we are" section for this milestone — carried forward
  verbatim rather than re-derived, since a human already wrote it down once.

Nothing in this bundle is an opinion. It is the raw material the Auditor reasons over,
and it is retained (see "Durable audit surface") so a disagreement can always be traced
back to what was actually true at `base_commit_sha`.

### The exit-criterion audit schema

The Auditor consumes the Evidence Bundle and the milestone's exit criteria and
produces, per criterion, exactly one of four statuses — no others exist:

| Status | Meaning | Required evidence |
|---|---|---|
| `met` | Fully satisfied, as evidenced. | At least one Evidence Bundle citation of the kind the criterion's domain demands (below). |
| `partial` | Some of it is real; a concrete piece is missing. | Citations for what's done, **and** a named, specific gap — mirrors the "Partial: …" convention `docs/roadmap.md` already uses. |
| `unmet` | Nothing in the bundle satisfies it. | A citation of the *absence* — e.g. "grep found no implementation under `crates/rune-engine/src/combat/`," not silence. |
| `obsolete` | The criterion no longer applies. | A citation of the decision that superseded it (an ADR, a later roadmap edit). Never a bare assertion — and `obsolete` is a **proposal**, not a fact: only the human closeout gate can retire a criterion. |

**Issue closure is never sufficient evidence on its own**, for any status — an issue
can close without its acceptance criteria being met, and a criterion can span several
issues or predate issue tracking entirely. What counts as sufficient is domain-specific:

- **Code** — a test anchor (crate, test name) plus, for rule behavior, the
  corresponding `docs/rules-coverage.md` row with its CR citation.
- **Protocol** — the `docs/protocol.md` section documenting the shape, plus a
  round-trip test in `rune-protocol`.
- **Browser** — a named, currently-green `E2E` job/spec (ADR 0011's tiers), not a
  merged-PR reference to a spec that may have since been deleted or skipped.
- **Usability** — a citation to a scripted usability run against
  `design/ui-requirements.md`'s named criteria (M4's exit condition names this
  explicitly) — a feature existing is not evidence it is *followable*.
- **Legal** — an explicit citation of the relevant `docs/brief.md` "Legal
  Considerations" clause and why the shipped content satisfies it (e.g., the M3
  starter-set criterion) — this is the one domain where "no test" is expected and a
  human sign-off citation is the evidence.
- **Operational** — a citation of the CI job/workflow definition that enforces the
  behavior going forward (e.g., a required check, a cron), not just a one-time PR.

Each criterion's audit entry cites the Evidence Bundle by field/path, never by
re-quoting its contents — the audit is a judgment layered on fixed evidence, not a
second copy of it.

### Independent audit review

The Audit Reviewer is a **separate adapter invocation with zero shared context** with
the Auditor — same Evidence Bundle and criteria text, no visibility into the Auditor's
reasoning, exactly ADR 0015's independence property (context isolation first, model/
provider diversity second where configuration allows it). Its job is narrow: for each
criterion, **agree**, **disagree** (with its own proposed status and citation), or flag
**insufficient evidence** (neither verdict is supportable from the bundle as given).

**Disagreements are surfaced, never silently resolved.** The Review record is additive
— it never rewrites the Auditor's record. Both stand side by side, in the schema and on
the planning issue, and a disagreement on any criterion is enough for the Review's
overall recommendation to be `not-ready` regardless of how many other criteria agree.
Reconciling a disagreement is explicitly the human closeout gate's job, not the
Reviewer's — a reviewer that "fixes" the first audit reintroduces exactly the single-
pass, unverified failure mode this ADR exists to remove.

### The human closeout gate

**No model or script may close a milestone or rewrite an exit criterion's status.**
The gate is one explicit maintainer command naming the `cycle_id` and a decision:

- **Approve** — closes the milestone. The command may include per-criterion
  *overrides* (e.g., ratifying an `obsolete` the Auditor proposed, or downgrading a
  disputed `met` to `partial`) — every override is recorded as `{criterion_id, from,
  to}` so the audit trail shows exactly what the human changed and why it differs from
  both AI records.
- **Reject** — the milestone is not done. The cycle terminates here: no planning phase
  runs, the audit and review are posted to the planning issue as the actionable output
  (gaps to file as issues, by hand or as new `agent-task` items), and the planning issue
  stays open until the maintainer is ready to try again.

There is no default and no timeout-based auto-approval. The gate exists precisely
because "is this milestone actually done" is a judgment about the project's own
claims about itself, which is a strictly human decision in this repository.

### Next-milestone planning boundaries

Only on an **approved** closeout does the Planner run, and it decomposes **only the
milestone immediately after the one just closed** — never two ahead. This matches
`docs/roadmap.md`'s own existing convention: M1–M3 carry PR-sized feature tables, M4–M7
stay prose-level "coarse" outcomes. A milestone two or more out is speculative — its
scope will have shifted by the time work reaches it, and detailed issues filed against
it would rot unreviewed, which is precisely the "expanding distant milestones" this
ADR's issue lists as out of scope. The Planner's input is: the just-closed milestone's
audit (so known gaps carry forward as day-one issues in the new milestone rather than
being dropped), the next milestone's prose outcome and any existing coarse exit
criteria from `docs/roadmap.md`, and the current open-issue set (so it does not
re-propose work already tracked).

### The issue-manifest schema

The Planner emits a manifest: `schema_version`, `cycle_id`, `milestone`, `created_at`,
and a list of items, each:

```
plan_id            stable, human-readable, unique within the manifest
                    (e.g. "M3-oracle-printing-split") — embedded verbatim in the
                    created GitHub issue as an HTML comment marker, so re-application
                    can find it after a partial run.
goal               one sentence — "what exists after this that doesn't exist now"
                    (same framing as the agent-task issue template's Goal field).
serves_criterion   [criterion_id, ...] — every item traces back to at least one
                    exit criterion from the audit; a criterion with no covering
                    item is a gap the Plan Reviewer must catch.
depends_on         [plan_id, ...] — other manifest items, or an existing issue
                    number for a dependency that already exists.
labels             from the allowed set (docs/agents/workflow.md's Labels section).
milestone          the target milestone name.
acceptance_criteria  checkable statements, CI-verifiable wherever possible.
verification       the command(s) that prove it (`make check`, a named test).
scope              { in: [...], out: [...] } — files/areas expected to change,
                    and what must not.
risks              known risks/unknowns, if any.
state              ready | blocked — computed from depends_on, not asserted.
```

This is the same shape the `agent-task` issue template already asks a human to fill in
by hand — the manifest schema exists so the Planner fills it in per item, consistently,
before any of it becomes a live GitHub issue.

### Granularity rubric

**Not a fixed issue-count quota.** A plan item is correctly sized when: (a) it is one
coherent, independently-mergeable outcome — the same "what exists after this that
doesn't exist now" test the `agent-task` template already uses; (b) it is reviewable by
a human in one sitting — the existing diff-size buckets from the ADR 0016 run summary
(`xs`/`s`/`m`/`l`, `tools/agent-task/summary.js`) are the concrete proxy: target `s`/`m`,
and an item the Planner expects to land `l` should usually be split; (c) it does not
bundle unrelated areas (an item spanning `area:engine` and `area:client` is a smell
unless it is genuinely one integration step, per `AGENTS.md`'s "split engine/client
work into two PRs" rule); (d) dependencies are named edges, not folded into a bigger
item to avoid drawing the edge.

**Wave cap.** One generated manifest is capped at a **reviewable size** — in the same
order of magnitude as M3's hand-decomposed wave (#146–#160, fifteen issues reviewed as
one block). The Planner must not silently drop coherent-outcome work to stay under the
cap: anything cut is logged in the manifest as `deferred_items` (goal + reason), so the
next cycle — or a human, immediately — can pick it up rather than it quietly vanishing.

### Deterministic manifest validation

Before any AI review of the manifest, a **script** checks it mechanically:

- every required field is present on every item;
- `plan_id` is unique within the manifest;
- the `depends_on` graph (including edges to existing issue numbers) is acyclic — a
  cycle is reported naming the loop;
- every label is drawn from the allowed set;
- every `serves_criterion` value exists in the current audit;
- no item's `plan_id` marker already exists on a live (open or closed) issue, and no
  item's goal/title is a near-duplicate of an existing open issue (exact `plan_id`
  collision is a hard failure; a fuzzy title match is a soft warning surfaced to the
  Plan Reviewer, not a blocker on its own).

A manifest that fails validation does not proceed to plan review — validation is
deterministic and idempotent, so it is simply re-run after a fix.

### Independent plan review

A separate adapter invocation, fresh context, same independence property as the audit
review. It checks: **scope overlap** between manifest items and with existing open
issues; **missing work** (audit criteria with no `serves_criterion` coverage, or gaps
named in `documented_gaps`/`todos_and_stubs` that no item addresses); **architecture
sequencing** (a `depends_on` edge that is wrong — missing a real prerequisite, or
claiming one that isn't actually required); and **issue size** against the granularity
rubric. Output has the same shape as the audit review — per-item verdict, a
disagreements list that never rewrites the manifest, and an overall recommendation.

### Human approval before issue creation

**Explicit, and may be partial.** The maintainer's command names the `cycle_id` and the
approved subset of `plan_id`s — approving 10 of 13 proposed items is a normal outcome,
not an escape hatch. Nothing is created for a `plan_id` that is not in the approved
set. As with closeout, there is no default and no timeout-based approval.

### Idempotent application and recovery

The **Applier is a script**, not a model call, and reuses the ADR 0016 runner's GitHub
mutation conventions (issue creation as `rune-agent[bot]`, never the maintainer's own
credentials). For each approved `plan_id`, in dependency order (so a dependent item can
reference its prerequisite's real issue number once created):

1. Search open and closed issues for the `<!-- rune-plan-id: <plan_id> --%>` marker.
   If found, **skip creation** and record the existing issue number — this is what
   makes a re-run after a partial failure safe rather than duplicative.
2. Otherwise, create the issue from the manifest item (`agent-task` template fields,
   the marker embedded in the body, `status:ready` or `status:blocked` per its computed
   `state`, milestone assigned).
3. Record the result: `{plan_id, issue_number, created | skipped_existing}`.

A crash mid-wave is recoverable by re-running application: steps 1–3 are naturally
idempotent, so already-created issues are found and skipped, and only the remaining
approved `plan_id`s are attempted. An item that fails to create (API error) is recorded
as `failed` with the reason and is **not** silently retried — it surfaces to the human,
the same way a failed run in ADR 0016 stays resumable rather than being swallowed.

### Roadmap reconciliation

Once application completes, a deterministic step opens a PR (via the same
`bot-push.sh`/`bot-pr.sh` path as any agent PR) that:

- ticks exactly the exit-criterion checkboxes the human closeout decision approved (no
  more — a criterion the human left `partial` stays unticked even if the Auditor
  proposed `met`);
- updates the "Last reconciled" date;
- adds the new milestone's feature table, with **every row's issue link the exact
  issue number `application` actually created or matched** — never a placeholder,
  since the whole point of the Applier's recorded mapping is that this step has it;
- links the `cycle_id` and the planning issue in the PR body.

This PR is not special-cased in `main`'s ruleset: `docs/roadmap.md` falls under
CODEOWNERS' `*` entry, so it needs the same human approval and the same required
checks as any other PR. **A human merges it, and merging it is what closes the cycle.**
No script closes the planning issue itself before that merge.

### State, reporting, and what stays out of tracked paths

Each cycle has **one GitHub issue** (created by the Evidence Collector at cycle start,
labeled `agent-task`-adjacent — e.g. `stewardship-cycle` — plus the milestone) that
accumulates the cycle's stages as it runs: a comment for the evidence summary, the
audit, the audit review, the closeout decision, the plan, the plan review, the plan
approval, the created-issue list, and the roadmap PR link. This is the **single
human-facing surface** — a maintainer reads one issue top to bottom to see the whole
cycle, the same way `scripts/agent-task status` is the single surface for one run.

What does **not** go into the repository at any point: prompts, full Evidence Bundles,
verification logs, and adapter working copies. These live exactly where ADR 0016's run
artifacts live — outside the repo, under
`${XDG_STATE_HOME:-~/.local/state}/rune/cycles/<cycle_id>/` — and are never committed,
never added to a PR diff, and never pasted into the planning issue wholesale. The
planning issue gets *summaries* (the audit table, the manifest table); the full bundle
is referenced by `base_commit_sha` and can be regenerated deterministically from it.

### The cycle summary/event schema

A **versioned, sanitized** JSON record per cycle, deliberately built on the same
conventions as the ADR 0016 run summary (`tools/agent-task/summary.js`) so #200 reads
both with one mental model:

```
schema_version, cycle_id, resume_of, supersedes
milestone
created_at, terminal_at
lifecycle: [{state, at}]   // collecting_evidence, auditing, audit_review,
                           // awaiting_closeout, closeout_approved|closeout_rejected,
                           // planning, plan_validation, plan_review,
                           // awaiting_plan_approval, plan_approved|plan_rejected,
                           // applying, applied, roadmap_pr_open, cycle_closed
evidence: { base_commit_sha, ref }             // pointer, not the bundle itself
audit: { criteria_counts: {met, partial, unmet, obsolete}, ref }
audit_review: { disagreement_count, disagreements: [{criterion_id, auditor_status,
                reviewer_status}], recommendation }
human_closeout: { decision, overrides: [{criterion_id, from, to}], actor, at }
plan: { item_count, deferred_count, ref }
plan_validation: { ok, failures: [{type, detail}] }   // deterministic
plan_review: { disagreement_count, disagreements: [...], recommendation }
human_plan_approval: { decision, approved_plan_ids: [...], actor, at }
application: { created: [{plan_id, issue_number}],
               skipped_existing: [{plan_id, issue_number}],
               failed: [{plan_id, reason}] }
duplicate_prevention: { collisions_found }
roadmap_pr: { number, issue_numbers_referenced: [...] }
terminal_outcome   // cycle_closed | closeout_rejected | plan_rejected |
                   // validation_failed | application_partial | abandoned
provider_usage     // optional, provider-reported, same allowlisted fields as ADR 0016
```

**Runner-observed vs. provider-reported** is preserved exactly as ADR 0016 defines it:
every field above except `provider_usage` is observed by the deterministic Evidence
Collector/Validator/Applier or is a structural fact about a human command, and is
authoritative; `provider_usage` is an adapter's own claim, advisory, and never
comparable across providers.

**Never in the record**, matching ADR 0016's "never a payload" rule exactly: prompts,
full Evidence Bundles, source diffs, adapter reasoning, secrets, environment values, or
full provider logs. The audit and manifest *content* live on the planning issue for
humans; the event record is a sanitized pointer-plus-counts artifact for machines.

### Durable audit surface and retention

The record is published to the **same `agent-runs` orphan branch** ADR 0016 already
established, under a sibling prefix: `cycles/<milestone>/<cycle_id>.json`. Reusing the
branch rather than creating a second one is deliberate — #200 gets one durable,
off-`main`, append-only surface to read instead of two, and the branch's existing
properties (orphan history, one-file-per-record, fast-forward-only writes) apply
unchanged. **Records are append-only**: a correction is a new record naming
`supersedes`, never a rewrite — the same discipline ADR 0016 applies to run summaries,
for the same reason (an audit trail that can be quietly edited is not an audit trail).
Retention is indefinite; records are small.

### Provider roles and adapter reuse

The four AI roles (Auditor, Audit Reviewer, Planner, Plan Reviewer) are each one
invocation of the **exact ADR 0016 provider-neutral adapter contract**: cwd is a
read-only checkout pinned at `base_commit_sha`; the brief is a bounded file outside the
working copy; the adapter's only output contract is its exit code plus a structured
result file (the audit, review, manifest, or plan-review artifact) at a known path,
analogous to `$RUNE_RESULT`; the environment is the same allowlist (no GitHub
credentials, no push access, no network beyond what the checkout itself needs); a
reviewer role is, in addition, a **separate process invocation with zero shared state**
with the role it reviews — ADR 0015's independence property, not merely a different
prompt in the same context. **No AI role may mutate GitHub, push, or write outside its
working copy** — identical to the ADR 0016 provider boundary, because these roles are
adapters in exactly ADR 0016's sense, not a new category of trusted code.

### Permissions

The tool this ADR describes may only **propose**: every AI-role output and every
deterministic Planner/Validator artifact is advisory until a human acts on it.
**Only an explicit maintainer command** may:

1. approve (or reject) milestone closeout, including any per-criterion override;
2. approve (or partially approve) the issue manifest for application;

and merging the roadmap reconciliation PR — the normal PR gate, no special-casing —
is what actually closes the cycle. No script or AI role may perform any of these three
on its own initiative, on a timer, or as a side effect of a passing check.

## Alternatives considered and rejected

- **Status quo: one-shot audit-plan-write pass.** This is the problem statement, not an
  alternative — named here because it is the thing every gate above exists to remove.
- **Automatic closeout/application gated only by post-hoc PR review.** Rejected: PR
  review (ADR 0015) is calibrated to catch defects in a diff, not to decide whether a
  milestone's own claims about itself are true. Folding that judgment into "did CI pass"
  would make the merge-gate carry a decision it was never designed to carry.
- **A single AI role that both audits and reviews its own audit.** Rejected for the
  same reason ADR 0015 requires a fresh-context reviewer: a model reasoning about its
  own prior conclusion shares the blind spot that produced them.
- **Fixed issue-count quota per generated wave (e.g., "always exactly N issues").**
  Rejected in favor of the granularity rubric: a quota produces artificially split or
  bundled issues depending on how much a milestone actually needs, and M1–M3 already
  show wave sizes varying by an order of magnitude for good reasons.
- **Fully decomposing all remaining milestones in one planning pass.** Rejected:
  `docs/roadmap.md` already treats M4–M7 as coarse on purpose; issues filed against
  them today would describe scope that has not been decided yet and would rot before
  work reaches them.
- **A dedicated new orphan branch for cycle telemetry.** Rejected in favor of reusing
  `agent-runs` with a `cycles/` prefix — one durable surface for #200 to read, not two
  with slightly different shapes to reconcile.
- **Embedding full audit/manifest text in the sanitized event record.** Rejected: it
  would bloat the schema and risk leaking evidence contents or model reasoning into a
  machine-read surface. Full content belongs on the human-facing planning issue; the
  event record stays a sanitized pointer-plus-counts artifact, exactly ADR 0016's
  "never a payload" rule applied here.

## Consequences

- **Easier.** Running out of ready issues stops being an event that requires a
  maintainer to sit down and do an unverified audit-plan-write pass by hand or trust a
  single model pass to do it. The cycle is resumable at every stage (a rejected
  closeout, a failed validation, a partially applied manifest all leave a legible,
  re-enterable state), reuses infrastructure that already exists (the ADR 0016 adapter
  boundary, the `agent-runs` branch, the `agent-task` template shape) rather than
  inventing parallel mechanisms, and gives #200 one more provider-neutral, sanitized
  telemetry stream to read.
- **Harder / given up.** Four AI roles plus two human gates is more moving parts than
  one prompt, and every one of those parts needs building, testing, and — like the
  ADR 0016 runner — recovering correctly from partial failure. The human maintainer is
  now a required, synchronous participant at two points per cycle (closeout, plan
  approval); this ADR treats that as the point, not a cost to engineer away. Distant
  milestones (M4+) get no early decomposition, so a maintainer wanting to look ahead
  still does that by hand.
- **Governance.** No AI role gains authority to close a milestone, create a live issue,
  or merge a PR. The only durable, tracked-repository state this cycle changes without
  an explicit human command is the append-only telemetry record on `agent-runs` — which,
  per ADR 0016's own rule, confers no authority on its own.
- **Deferred (the follow-up implementation task).** Building this is out of scope here,
  as building the ADR 0016 runner was out of scope in that ADR; it lands as a separate
  `agent-task` issue with acceptance criteria informed by everything decided above,
  landing as these PR-sized slices, in order:
  1. **Evidence Collector + schema.** The deterministic script producing the Evidence
     Bundle (exit criteria, issues/PRs, CI, a real `make verify` run, tests, rules
     coverage, ADR/protocol state, TODO/stub sweep, documented gaps), with the schema
     versioned and tested against a fixture milestone.
  2. **Audit + Audit Review adapters, and the closeout gate.** The Auditor and Audit
     Reviewer as ADR 0016-contract adapter invocations; the four-status audit schema;
     the disagreement-surfacing Review schema; the human closeout command (approve with
     overrides / reject) and its effect (or non-effect) on `docs/roadmap.md`.
  3. **Planner, manifest validation, and Plan Review.** The issue-manifest schema; the
     granularity rubric applied as a wave-size cap with logged `deferred_items`; the
     deterministic validator (fields, duplicate `plan_id`s, dependency cycles, invalid
     labels, marker/title collisions); the Plan Reviewer adapter and its schema.
  4. **Applier and roadmap reconciliation.** Idempotent, marker-based issue creation in
     dependency order with recorded create/skip/fail outcomes and resume-safety; the
     roadmap reconciliation PR generator referencing exact created issue numbers; the
     human plan-approval command (full or partial).
  5. **Cycle summary schema, `agent-runs` publication, and the planning issue.** The
     versioned event schema; publication to `agent-runs` under `cycles/`; the planning
     issue's stage-by-stage reporting; `docs/agents/workflow.md` updated to describe the
     cycle and point at the new command, mirroring how it documents `scripts/agent-task`
     today.
