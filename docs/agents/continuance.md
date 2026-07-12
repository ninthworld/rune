# AI project continuance playbook

RUNE is built mostly by AI agents working in short-lived sessions, and it has to keep
advancing safely across them: no session remembers the last one, and any of them may be
running a different model. This document is the **operating model** that makes that work
— how a milestone outcome becomes issues, an issue becomes a pull request, and a pull
request becomes merged behavior, with the triggers, states, evidence, and human gates
that separate the three.

Two properties are deliberate:

- **Provider-neutral.** Nothing here makes Claude, Codex, or a local model canonical. A
  provider is an interchangeable code-editing subprocess ([ADR 0016][adr16]); the
  project's rules live in this repository, not in a vendor's runtime.
- **Tool-independent.** Every automated step below has a **manual equivalent**, stated
  next to it. Continuance must not stop because a program is unbuilt or a provider is
  down. Where automation exists, it is a faster way to execute this playbook — never a
  different one.

**This is not a command reference.** Flags, sandbox setup, labels, and GitHub settings
live in [`workflow.md`](workflow.md) and change far more often than policy does. This
file states what must be true; `workflow.md` states how to spell it today.

---

## 1. Authority

When two sources disagree, the higher-authority one is right and the other is a defect —
fix it in the same PR, or file an issue for it. Never resolve a conflict by picking the
convenient answer.

| Question | Authoritative source | Explicitly *not* authoritative |
|---|---|---|
| What work exists, who holds it, what state it is in | **GitHub issues** and their labels | any file that lists work |
| Whether a milestone outcome is met | [`docs/roadmap.md`](../roadmap.md) exit criteria, ticked only by a human closeout (§3) | closed issues, a merged PR, an agent's summary |
| Architecture and cross-cutting design | [`docs/decisions/`](../decisions/) — an ADR with `Status: accepted` | prose anywhere else, including this file |
| The client/server contract | [`docs/protocol.md`](../protocol.md) + `crates/rune-protocol` | client code, server code |
| What behavior actually shipped | the **tests on `main`**, green under the required checks | roadmap narration, PR bodies, issue titles |
| How to write code here | [`AGENTS.md`](../../AGENTS.md) + the nearest nested `AGENTS.md` + [`docs/coding-standards.md`](../coding-standards.md) | this file |
| The lifecycle: milestone → issue → PR | **this file** | — |
| Commands, flags, labels, GitHub settings | [`workflow.md`](workflow.md) | this file |

A model-produced artifact — an audit, a plan, a review, a run summary — is **evidence,
never authority**. It informs a decision; it does not make one.

---

## 2. The three loops

```text
┌─ MILESTONE LOOP ─ (§3) ─────────────────────────────────────────────────┐
│  ready queue empties → evidence → audit → review → HUMAN closeout gate  │
│                                → plan → review → HUMAN approval gate    │
│                                → create issues → roadmap PR             │
└────────────────────────────────────────┬────────────────────────────────┘
                                         │ produces leaf issues
┌─ ISSUE LOOP ─ (§4) ─────────────────────▼───────────────────────────────┐
│  status:ready → claim → status:in-progress → implement → verify         │
└────────────────────────────────────────┬────────────────────────────────┘
                                         │ produces one PR per leaf issue
┌─ PR LOOP ─ (§5) ────────────────────────▼───────────────────────────────┐
│  draft PR → required checks → AI review → HUMAN approval → squash merge │
│                                        └─ findings ─▶ back to ISSUE LOOP│
└─────────────────────────────────────────────────────────────────────────┘
```

Each loop hands the next one a *reviewable artifact*, and each ends at a **human**. That
is the invariant the rest of this document elaborates.

### Who may decide what

Three kinds of step, and they must never be confused for one another:

| | **Deterministic** | **AI judgment** | **Human approval** |
|---|---|---|---|
| **What it is** | a script or a CI check | a bounded adapter invocation | an explicit maintainer command or click |
| **Trust** | authoritative — it observed the fact | advisory — it formed an opinion | decisive |
| **Examples** | `make verify`, the four required checks, the atomic claim, dependency/label validation, issue creation from an approved manifest, evidence collection | implementing an issue, auditing a milestone, planning the next one, reviewing a PR, reviewing another AI's audit or plan | closing a milestone, approving an issue plan, approving a PR, merging |
| **May it change what the project believes is true?** | only by executing a decision a human already made | **never** | yes — that is what it is for |

The rule this table exists to state: **an AI role proposes, a deterministic step observes
or applies, a human decides.** No model closes a milestone, creates a live issue,
approves a PR, or merges — not its own work and not another agent's. No script escalates
its own authority because a check went green.

---

## 3. The milestone loop

A milestone is an **outcome checkpoint** with exit criteria ([`docs/roadmap.md`](../roadmap.md)),
not a work phase. The milestone loop is how RUNE decides a milestone is finished and what
the next wave of issues should be — the stewardship cycle accepted in [ADR 0017][adr17].

### When the loop runs — the queue-exhaustion decision tree

The trigger is the symptom: **there is nothing an agent can pick up next.**

```text
Is any issue in the current milestone `status:ready` and dependency-free?
├─ YES ──▶ Do not start a cycle. Work it (§4).
└─ NO
   ├─ Are there `status:in-progress` / `status:review` issues?
   │  └─ YES ──▶ Claimed work is in flight. A cycle may still start (its audit is
   │             read-only), but expect closeout to be rejected: the milestone
   │             cannot be done while its own work is unmerged.
   ├─ Are the remaining open issues `status:blocked`?
   │  └─ YES ──▶ START a cycle anyway. A blocked issue is by definition not
   │             something an agent can begin, so the queue is empty in the only
   │             sense that matters — and the audit is frequently what surfaces
   │             *why* it is blocked (a missing decision, a wrong dependency edge,
   │             scope that quietly grew). Closeout is still expected to be
   │             rejected; see "when planning must stop" below.
   ├─ Is a blocking `decision` issue open?
   │  └─ YES ──▶ The decision is the work. Resolve it (an ADR, §4) before
   │             anything downstream of it can become `status:ready`.
   └─ Are there no open issues in the milestone at all?
      └─ YES ──▶ START a cycle. This is the unambiguous case.
```

A maintainer may also start a cycle **on demand** at any time — to audit progress
mid-milestone, for instance. That is the only way to force an early cycle.

A cycle that ended in a rejected closeout does **not** restart itself against unchanged
state. It re-triggers only when `main` has moved (new evidence exists) or a maintainer
re-invokes it.

### The phases

Each phase consumes the previous phase's artifact. The two **HUMAN GATE**s are the only
places the project's own claims about itself can change.

| # | Phase | Kind | Produces | May it write? |
|---|---|---|---|---|
| 1 | **Evidence** | deterministic | Evidence Bundle, pinned to one `base_commit_sha` | reads only |
| 2 | **Audit** | AI (Auditor) | per-criterion status: `met` / `partial` / `unmet` / `obsolete`, each with a citation | its working copy only |
| 3 | **Audit review** | AI (Audit Reviewer, **fresh context**) | per-criterion agree / disagree / insufficient-evidence, plus an overall recommendation | its working copy only |
| 4 | **HUMAN GATE: closeout** | human | approve (with per-criterion overrides) **or** reject | the milestone's exit-criterion boxes |
| 5 | **Plan** | AI (Planner) | an issue manifest for **the next milestone only** | its working copy only |
| 6 | **Plan validation** | deterministic | pass/fail on fields, duplicate `plan_id`s, dependency cycles, invalid labels, collisions with live issues | reads only |
| 7 | **Plan review** | AI (Plan Reviewer, **fresh context**) | scope overlap, missing work, sequencing errors, oversized items | its working copy only |
| 8 | **HUMAN GATE: plan approval** | human | approve the manifest, in full **or a subset** | — |
| 9 | **Apply** | deterministic, idempotent | the approved issues, created in dependency order | GitHub issues |
| 10 | **Roadmap PR** | deterministic | a PR reconciling `docs/roadmap.md` with the exact issue numbers created | a normal PR (§5) |

The cycle closes when a **human merges the roadmap PR** — the ordinary PR gate, not a
special case.

### The rules that hold across every phase

- **Issue closure is never milestone evidence.** An issue can close without its
  acceptance criteria being met; a criterion can span several issues or predate issue
  tracking entirely. Sufficient evidence is domain-specific: a named test for code, a
  `docs/protocol.md` section plus a round-trip test for protocol, a currently-green `E2E`
  spec for browser behavior, a scripted run against
  [`docs/design/ui-requirements.md`](../design/ui-requirements.md) for usability, a cited
  [`docs/brief.md`](../brief.md) legal clause for legal, an enforcing CI job for
  operational. A merged PR is not evidence that `main` still works — only a fresh gate run
  at the audited commit is.
- **No model closes a milestone, and no model approves its own plan.** Phases 4 and 8
  have no default and no timeout-based auto-approval. An `obsolete` verdict is a
  *proposal*: only the human gate can retire an exit criterion.
- **A reviewer never rewrites what it reviews.** The Audit Reviewer and Plan Reviewer are
  separate invocations with **zero shared context** with the role they review ([ADR
  0015][adr15]'s independence property). Disagreements are recorded side by side and
  reconciled by the human gate. A reviewer that "fixes" the first pass reintroduces
  exactly the single-unverified-pass failure this cycle exists to remove.
- **Only the next eligible milestone is decomposed in detail.** Later milestones stay
  outcome-level prose (the roadmap already does this: M1–M3 carry issue tables, M4–M7 do
  not). Issues filed two milestones out describe scope that has not been decided and rot
  before work reaches them.
- **Gap issues** come from the audit, not from a separate sweep. Every criterion the
  human closeout leaves `partial` or `unmet`, and every gap the audit named
  (`documented_gaps`, TODO/stub sweep), becomes either a day-one leaf issue in the next
  milestone's manifest or an explicitly deferred item — never nothing. Work cut to keep a
  wave reviewable is logged as a deferred item with its reason; it is not silently
  dropped.
- **When planning must stop instead of advancing.** A **rejected closeout terminates the
  cycle.** No planning phase runs. The audit and its review are the actionable output —
  file the gaps as issues and try again. Planning the next milestone while the current one
  is known-incomplete would let the cycle redefine "done" by moving on, which is the exact
  failure the gate exists to catch. Planning also stops when plan validation fails
  (fix and re-run — validation is deterministic and idempotent) and when the human
  approves only a subset (nothing is created for an unapproved `plan_id`).

### Automation, and the manual fallback

Phase 1 is built. Phases 2–10 are not ([#189](https://github.com/ninthworld/rune/issues/189),
slices [#225](https://github.com/ninthworld/rune/issues/225)–[#228](https://github.com/ninthworld/rune/issues/228)).

```sh
scripts/agent-cycle collect <milestone>   # phase 1: the Evidence Bundle, read-only
scripts/agent-cycle show <cycle-id>       # summarize a collected bundle
```

Until the rest lands, **the maintainer runs phases 2–10 by hand**, and the playbook is
unchanged by that: collect the evidence with the command above (or gather the same facts
by hand — the roadmap's criteria verbatim, the milestone's issues and merged PRs, a fresh
`make verify` at the audited commit, test counts, rules coverage, ADR/protocol state, TODO
sweep, documented gaps); audit each criterion against it and write the four-status table
into the planning issue; get a second opinion from a **fresh session** that has not seen
the first; decide closeout yourself; decompose the next milestone into issues under the
granularity rubric (§4); have a fresh session review that decomposition; approve it; file
the issues; open the roadmap PR. The gates are the point, not the program: a hand-run
cycle that skips the independent review or the closeout gate is not this cycle.

Bundles, prompts, and logs live **outside the repository**
(`$XDG_STATE_HOME/rune/cycles/<cycle-id>/`) and never enter a diff, a PR body, or a
telemetry record. The single human-facing surface is one **planning issue** per cycle,
read top to bottom.

---

## 4. The issue loop

### Issue roles

| Role | Label | Maps to a PR? | Example |
|---|---|---|---|
| **Leaf** | `agent-task` + one `area:*` | **Exactly one PR.** | [#193](https://github.com/ninthworld/rune/issues/193) |
| **Parent / tracking** | `area:*`, no `agent-task` | **Never.** It is closed by its children closing. | [#195](https://github.com/ninthworld/rune/issues/195), [#189](https://github.com/ninthworld/rune/issues/189) |
| **Decision** | `decision` | Its PR adds an **ADR**, not behavior. | [#182](https://github.com/ninthworld/rune/issues/182) → [ADR 0015][adr15] |
| **Bug** | `bug` + `area:*` | One PR, and it **must** carry a regression test. | — |

**One leaf issue ↔ one PR, in both directions.** A PR closes exactly one leaf issue, and
a leaf issue is closed by exactly one PR. There is no exception for work that spans the
engine and the client: that is two leaf issues under a parent, not two PRs against one
issue. An outcome that cannot be a single reviewable PR is not a leaf issue — it is a
parent issue that has not been decomposed yet.

### What a leaf issue must carry

The [`agent-task` issue form](../../.github/ISSUE_TEMPLATE/agent-task.yml) collects these;
the semantics are here. ([#198](https://github.com/ninthworld/rune/issues/198) owns the
form's concrete fields and their deterministic validation — see §6.)

| Field | Semantics |
|---|---|
| **Outcome** | One sentence: *what exists after this that does not exist now.* Not a description of the work. |
| **Area** | Exactly one `area:*`. Two areas in one leaf issue is a decomposition smell (`AGENTS.md`). |
| **Parent / milestone** | The tracking issue and/or milestone this serves, or an explicit *none* for independent maintenance. |
| **Dependencies** | A **`Blocked by:`** heading followed by `#N` links — this exact convention, because it is machine-read (`tools/agent-task/preflight.js`). A `#N` anywhere else in the body is a reference, **not** a dependency. |
| **Authoritative context** | *Pointers* to the ADR, protocol section, or standard that governs the change. Never a paste of them; an agent can open a file. |
| **Constraints / non-goals** | In scope / out of scope. What must **not** change is as load-bearing as what must. |
| **Acceptance criteria** | Markdown **checkboxes** (`- [ ] …`) stating observable outcomes, not implementation narration. The checkbox form is required: the PR body's evidence table is built by joining against these lines. An issue with no checkboxes cannot produce that mapping. |
| **Verification** | The command that proves it (`make check`, `make verify`, a named test). |
| **Affected contracts** | Protocol, ADR, rules coverage, data/migration compatibility, client/server boundary, security, docs — each *changed* or *not applicable*, never blank. |
| **Evidence** | For a bug: a deterministic reproduction. For a rules change: the CR citation ([`docs/coding-standards.md`](../coding-standards.md)). |

**Stable plan IDs.** An issue generated by the milestone loop carries its manifest
`plan_id` verbatim in the body as an HTML comment marker
(`<!-- rune-plan-id: M3-oracle-printing-split -->`). It is what makes re-applying a
partially created wave idempotent: the applier finds the marker and skips, instead of
filing a duplicate. Never edit or remove one.

**Granularity rubric.** Not an issue count — a shape. An item is correctly sized when it
is (a) one coherent, independently mergeable outcome; (b) reviewable by a human in one
sitting; (c) not a bundle of unrelated areas; and (d) honest about its dependencies —
folding a prerequisite into a bigger item to avoid drawing the edge is how a wave becomes
unreviewable.

### Ready, and blocked

An issue is **`status:ready`** only when *all* of these hold. Anything else is
`status:blocked`.

- It is a **leaf** issue (`agent-task`), sized for one PR.
- Every issue under its **`Blocked by:`** is **closed**.
- No open `decision` blocks it. Decision-incomplete work is blocked work; an agent that
  implements one is making the decision by accident.
- It has checkable acceptance criteria and a stated scope.

> **Only a `status:ready`, dependency-free leaf issue may be implemented.** The runner
> enforces this before its first GitHub mutation, so a rejected task leaves no trace; a
> human-driven session must check it by hand. Picking up a blocked issue is not
> initiative — it is building on a decision nobody has made.

### Lifecycle

```text
                    ┌──────────────── release ────────────────┐
                    │                                         │
   status:blocked   │   ┌── failure ──┐ (claim + work kept)   │
        │           ▼   ▼             │                       │
   (deps close) ─▶ status:ready ─claim─▶ status:in-progress ──┴─▶ status:review ─▶ closed
                       ▲                     │       ▲                              (by merge)
                       │                     │       │
                       │              (no heartbeat) │
                       │                     ▼       │
                       └── release --force ── STALE ─┘   (human only)
```

| Transition | Trigger | Who |
|---|---|---|
| **claim** | Atomic creation of the remote branch `agent/<issue>-<slug>` from current `origin/main`. GitHub returns 422 if the ref exists, so the API call *is* the lock and exactly one claimant wins. (A GitHub App cannot be an issue assignee, so the branch — not an assignment — is the claim.) | runner / agent session |
| **in-progress** | The claim succeeded. Recorded as a comment naming run, branch, provider, and time. | runner |
| **review** | **Only once the draft PR exists.** The label always points at a reviewable artifact, never an intention. | runner |
| **failure** | A gate failed. The issue **stays `status:in-progress`** and keeps its claim, branch, and working copy: a failed run holds a diff worth resuming, and dropping the claim on every stumble invites two agents to redo the same work. | runner |
| **release** | The claim is dropped and the issue returns to `status:ready`. | human or the agent that holds it |
| **stale recovery** | A claim that stopped heartbeating shows as `⚠️ STALE`. Taking it over is `release --force` — **a human's call, never another agent's**, since the two cannot distinguish "abandoned" from "slow". | human |
| **closed** | The PR merges with `Closes #N`. Nothing else closes a leaf issue. | merge |

### Doing the work

**With the runner** ([ADR 0016][adr16]; commands and sandbox setup in
[`workflow.md`](workflow.md)):

```sh
scripts/agent-task start 190 --provider <claude|codex|local>
```

It claims, hands an isolated working copy and a bounded brief to the provider, and then
**takes nothing the provider says on trust**: it inspects the diff itself, runs the
gates, rebases onto current `main` and re-verifies, commits, pushes, and opens the draft
PR. Every GitHub mutation is made as `rune-agent[bot]`. It never approves and never
merges.

**By hand** (an interactive agent session, or a human) — the same steps, in the same
order, and the ones that matter are the gates, not the tool:

1. `git fetch origin && git switch -c agent/<issue>-<slug> origin/main` — branch from
   **current** `main`, not from whatever was checked out.
2. Implement. `make check` constantly; `make verify` before review.
3. `scripts/bot-push.sh` and `scripts/bot-pr.sh "<title>" "<body>"` — **never** plain
   `git push` / `gh pr create`. A PR is authored by whoever's token calls the API, and a
   PR authored by the maintainer is one the maintainer is forbidden to approve, which
   leaves the Admin bypass as the only way to merge it (see [`workflow.md`](workflow.md),
   "Bot-authored PRs").
4. Move the issue's label to `status:review` once the PR exists.

### Choosing a provider

No provider is canonical, and none is a dependency. Claude Code, Codex, and any local
harness sit behind the same adapter contract; the runner's `--provider` flag is the only
thing that changes between them, and `local` prescribes no model, harness, or vendor. If
a provider is unavailable, another provider — or a human — runs the same loop against the
same brief and produces the same artifacts.

Two rules constrain the choice rather than the vendor:

- **A reviewer must not be the implementer.** Independence is context isolation first and
  model diversity second ([ADR 0015][adr15]). A model reviewing its own diff shares the
  blind spot that produced it.
- **A provider is a code-editing function, not a participant.** It edits an isolated
  working copy and returns. Claim, verification, rebase, commit, push, PR, labels, and
  comments belong to the runner (or to the human running the loop by hand) — never to the
  provider, and never with the maintainer's credentials.

---

## 5. The pull-request loop

### What a PR must pass

| Gate | Kind | Blocking? | Owner |
|---|---|---|---|
| `Engine`, `Client`, `E2E`, `cargo-deny` | deterministic | **yes** — required checks | CI (`make verify` is the local mirror) |
| Up-to-date with `main` | deterministic | **yes** — strict required checks | the branch's owner |
| Review conversations resolved | deterministic | **yes** | reviewer + author |
| Independent AI review | AI judgment | **must run; findings advisory** ([ADR 0015][adr15]) | the review workflow |
| **≥ 1 human approval, from someone other than the author** | human | **yes** | @ninthworld (sole code owner) |
| Squash merge | human | — | @ninthworld |

> **AI review status today:** the interim `claude-review` workflow runs on agent PRs but
> is **not a required check**, so a silently skipped review is indistinguishable from a
> passing one — which is why the runner *observes* whether it actually ran rather than
> assuming it. ADR 0015's required-to-complete `AI Review` check lands with
> [#202](https://github.com/ninthworld/rune/issues/202). Its findings are advisory by
> design until calibrated, and no AI review — positive or negative — can approve a PR,
> dismiss a human's review, or substitute for the human approval.

### The evidence a PR body must carry

This is the **information contract**, not the template's layout
([#198](https://github.com/ninthworld/rune/issues/198) owns the template itself). A PR is
where an agent's claims meet a human's attention, and every field below exists because a
reviewer cannot reconstruct it from the diff.

| Field | Semantics |
|---|---|
| **Closing issue** | Exactly **one** `Closes #N`, and `#N` is a **leaf** issue. Two closing issues means the PR is two PRs. |
| **Related issues** | Parent/tracking, decision, and context issues as **plain references** (`#195`) — never `Closes`, which would close a parent its children have not finished. |
| **Acceptance mapping** | Each of the issue's criteria, verbatim, joined to the concrete evidence for it: the file, the test, the doc, the gate. A criterion the PR did **not** satisfy is listed as unmet — **explicitly, not by omission.** An unmapped criterion is the most useful line on the page: it is where the reviewer looks first. |
| **Verification** | The commands actually run and their **results** — not a checked box asserting that CI passed. |
| **Material assumptions** | Every decision the issue did not make for you. If you guessed, say so here; a silent assumption is a defect the reviewer cannot see. |
| **Compatibility impact** | Data/schema migrations, protocol shape changes, saved-state compatibility — or *not applicable*. |
| **Risks** | Where a reviewer should look hardest, and what you are least sure of. |
| **Contract & documentation impact** | Protocol, ADR, rules coverage, user-facing docs, dependencies, security — each *changed* or *not applicable*. A protocol change without a `docs/protocol.md` change is a rule violation (`AGENTS.md`), not an omission. |
| **CI-governance changes** | If the diff touches `.github/workflows/`, `.github/actions/`, `.github/rulesets/`, `.github/CODEOWNERS`, `Makefile`, or `scripts/bot-*.sh`: every touched path, at the top of the body, with the `ci-change` label. The bot holds `workflows: write` — it can weaken the very checks reporting green on that PR — so such a change may never arrive as an unremarked hunk. |

### Findings, and staleness

- **Review findings return the work to the implementing agent**, not to a reviewer with a
  patch. The agent that owns the branch pushes fixes to the same branch and the same PR;
  the reviewer re-reviews. A second agent does not take over another's open PR, and
  conflicts between agents are resolved by humans.
- **Any new commit dismisses the approval.** Stale-approval dismissal is on, so the
  recorded approval always describes the code that merged. This includes commits pushed to
  fix a review finding *and* the rebase that brings a behind-branch current — re-approval
  is required, and that is deliberate: an approval of code that is not the code being
  merged is not an approval.
- **Green checks on a stale base prove nothing.** Strict up-to-date-before-merge means a
  behind-branch cannot merge until it is rebased onto current `main` and the checks re-run
  against that base.

### Merge, and follow-through

The maintainer merges — **squash only**, so the PR title becomes the commit and must be a
Conventional Commit; linear history is required, so a merge commit from `main` is never
an option for updating a branch.

**Implementation agents and AI reviewers never merge their own work, and never approve
anyone's.** An agent's job ends at "green required checks + a PR ready for review." That
is not a courtesy: authors cannot approve their own PRs, so an agent that merges is an
agent that bypassed the gate.

After the merge:

| Step | Who |
|---|---|
| The leaf issue closes via `Closes #N`; its `status:review` label goes with it | GitHub |
| The branch is deleted; the run directory is cleaned (`scripts/agent-task cleanup <issue>`) | runner / author |
| A parent issue is ticked or closed **only when its children are actually done** | human |
| [`docs/rules-coverage.md`](../rules-coverage.md) gains a row when engine rule behavior changed — with its CR citation | the PR itself (definition of done) |
| [`docs/protocol.md`](../protocol.md) matches `rune-protocol` — in the same PR, always | the PR itself (hard rule) |
| [`docs/roadmap.md`](../roadmap.md) exit criteria — **not ticked here.** Only the milestone loop's human closeout gate ticks a box. A merged PR is not a met criterion. | §3 |

### Recovery

| Situation | What happens |
|---|---|
| **Red CI** | The PR's author investigates its own failure and pushes a fix. If it is genuinely unrelated (flake, infra), say so **in a comment with evidence** — never retry blindly, and never merge red. |
| **A conflict with `main`** | The branch's owner rebases onto current `origin/main` and force-pushes **with `--force-with-lease`, never `--force`**, via `scripts/bot-push.sh` so the branch stays bot-owned and the lease ref exists. This is the *only* legitimate force-push: an `agent/<issue>-<slug>` branch nobody else commits to. Never rewrite `main` or a shared branch. Re-verify after the rebase — a rebase can break a build that passed. |
| **A conflict between two agents' work** | Resolved by a **human**. An agent does not adjudicate another agent's diff. |
| **An abandoned PR / stale claim** | The claim shows as `⚠️ STALE`. A human takes it over (`release --force`) or closes it. The work is never silently deleted: a failed run keeps its branch and working copy. |
| **A partial GitHub mutation** (crashed mid-run: claimed but no PR; issues half-created) | Every mutating step is **idempotent and resumable**, which is why it is safe to simply re-enter it. `scripts/agent-task resume <issue>` picks up whatever is in the workspace now. The milestone applier finds its `plan_id` markers and skips what already exists. A failed create is **recorded and surfaced, never silently retried**. |
| **A provider is unavailable or the runner is broken** | Nothing is blocked. Switch `--provider`, or run the loop by hand (§4) — the artifacts and the gates are identical either way. That is what "tool-independent" is for. |
| **The bot cannot push** | The push is rejected atomically, after the work is done. Re-open the PR under maintainer credentials for that diff; do not weaken the app's permissions to route around it. |

---

## 6. The field contract for [#198](https://github.com/ninthworld/rune/issues/198)

[#198](https://github.com/ninthworld/rune/issues/198) implements the GitHub issue forms
and the PR template. This playbook owns their **semantics**; #198 owns their **shape and
their deterministic validation**. The two must not be redesigned in the same place.

What #198 must implement, and where its authority comes from:

| Contract | Defined in |
|---|---|
| Leaf-issue fields: outcome, area, parent/milestone, dependencies, context, constraints/non-goals, acceptance criteria, verification, affected contracts, evidence | §4, *What a leaf issue must carry* |
| The dependency convention — a **`Blocked by:`** heading followed by `#N` links, and nothing else read as a dependency | §4 (already machine-read by `tools/agent-task/preflight.js`) |
| Acceptance criteria as **markdown checkboxes** — the join key for the PR evidence table | §4, §5 |
| `status:ready` ⇔ leaf + decision-complete + every `Blocked by:` closed | §4, *Ready, and blocked* |
| PR evidence: one closing leaf issue, related references, acceptance mapping, commands and results, assumptions, compatibility, risks, contract/doc impact, CI-governance callout | §5, *The evidence a PR body must carry* |
| AI-review status recorded, never self-approved | §5 + [ADR 0015][adr15] |
| Human approval and no-self-merge preserved | §5 |

**Deterministic validation may check structure, never judgment.** It may reject a missing
or duplicated closing issue, a malformed `Blocked by:` reference, a missing acceptance
mapping, or missing verification results. It must **not** try to decide, by keyword
matching, whether a criterion is *truly* satisfied or a risk is *adequately* stated. That
is what the AI reviewer and the human are for, and a validator that pretends otherwise
manufactures exactly the false confidence this playbook is built to prevent.

---

[adr15]: ../decisions/0015-independent-ai-pr-review.md
[adr16]: ../decisions/0016-provider-neutral-issue-runner.md
[adr17]: ../decisions/0017-milestone-stewardship-cycle.md
