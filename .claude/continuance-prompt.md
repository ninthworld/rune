# Project Continuance Prompt

Use this prompt after completing a related batch of issues. This is a project-reconciliation
and planning session: establish what actually shipped, correct the roadmap and issue tracker,
and prepare the next coherent batch of work. Perform the authorized documentation and GitHub
updates rather than stopping at recommendations.

## Objective

Leave the project with an evidence-backed account of its current state and an actionable next
batch. The roadmap, repository, and issue tracker should agree about:

- which user-visible outcomes are currently possible;
- which milestone criteria are satisfied, partial, blocked, or invalid;
- which gaps are most important next;
- which GitHub issues own those gaps; and
- how the next issues depend on one another.

## Session inputs

Use any issue numbers, pull requests, milestone, comparison commit, or constraints supplied by
the operator. If no review window is supplied, infer the most recent coherent batch from merged
history and tracker activity, choose a bounded comparison point, and state that assumption in
the final report. Do not let an ambiguous batch boundary prevent evidence-based reconciliation
of the current project state.

## Operating rules

- Read and follow the repository's `AGENTS.md` files before making changes.
- Treat code, tests, and reproducible behavior as stronger evidence than roadmap wording,
  issue state, commit messages, or previous summaries.
- A merged PR or closed issue is not proof that its intended outcome works. Verify the actual
  implementation and the most relevant test or user path.
- Distinguish current implementation, near-term priority, long-term roadmap, and project-wide
  exclusions. Do not turn a deferred or low-priority feature into a non-goal.
- A per-issue `Non-goals` section limits that issue only. It must not redefine the project's
  long-term scope.
- Preserve architectural rationale in ADRs. Update or add an ADR only when the architecture or
  an accepted decision changes; do not rewrite historical decisions merely to match current
  implementation wording.
- Prefer updating or refining an existing issue over creating a duplicate.
- Do not create issues, roadmap entries, or milestone work merely to meet a quota.
- Do not use subjective completion percentages. Describe verified outcomes and concrete gaps.
- Plan only as far ahead as current evidence supports. Keep later milestones visible without
  inventing premature implementation detail.
- Keep final documentation declarative. Do not paste exploratory reasoning, review dialogue,
  or a chronological thought process into the roadmap.

## Phase 1: Establish the baseline

1. Read:
   - `docs/brief.md` for product scope and durable constraints;
   - `docs/roadmap.md` for milestones, claimed state, and stated priorities;
   - relevant specifications and ADRs for the active area; and
   - applicable nested `AGENTS.md` files.
2. Review the repository history since the previous continuance:
   - merged commits and pull requests;
   - files and contracts changed;
   - tests added or modified; and
   - follow-up fixes or regressions that changed the apparent outcome.
3. Review the current GitHub tracker:
   - open issues in and around the active milestone;
   - recently closed issues from the completed batch;
   - dependencies, duplicates, and superseded issues; and
   - review comments or follow-up issues that expose incomplete acceptance criteria.
4. Identify the active milestone from product evidence, not merely from its label. Later work
   may have landed while an earlier user-visible outcome remains incomplete.
5. State any unavoidable assumptions used to define the review window or active milestone.

## Phase 2: Assess completed work

For each issue or feature in the completed batch:

1. Restate the intended user-visible outcome and its acceptance criteria.
2. Locate implementation evidence in the repository.
3. Locate tests that exercise the relevant behavior at the appropriate boundary. A unit test
   is not sufficient evidence for a browser, protocol, persistence, or multi-client outcome
   when the risk exists at an integration boundary.
4. Check whether documentation and wire contracts match the implementation.
5. Classify the outcome:
   - **Satisfied:** the outcome and material edge cases are implemented and evidenced.
   - **Partial:** useful work landed, but a stated criterion or user path is missing.
   - **Blocked:** a concrete dependency or external constraint prevents completion.
   - **Invalid:** the criterion no longer describes the desired product or architecture.
6. Record deviations that affect future work, including changed APIs, newly discovered risks,
   shortcuts, regressions, or follow-up requirements.

If a closed issue lacks evidence for its claimed outcome, do not silently treat it as done.
Correct the roadmap and update, reopen, or replace the issue as appropriate. If implementation
exceeds the original issue, document the resulting capability without inflating unrelated
milestone claims.

## Phase 3: Validate milestone outcomes

For the active milestone:

1. List each outcome or exit criterion.
2. Map it to concrete repository and test evidence.
3. Identify missing user paths, integration seams, edge cases, quality thresholds, and
   operational requirements.
4. Challenge vague criteria by asking:
   - What can a user do when this is complete?
   - What observable result proves it?
   - Which failures or edge cases materially undermine that outcome?
   - Which layer owns the behavior?
   - What test boundary is capable of proving it?
5. Split criteria only when separate work can be delivered and reviewed independently. Do not
   fragment one coherent behavior into administrative micro-issues.
6. Reopen an earlier milestone when current evidence disproves its user-visible outcome, even
   if its original issue list is closed.

## Phase 4: Reconcile the roadmap

Update `docs/roadmap.md` so it describes the project as it exists now:

1. Correct the current-state summary with verified capabilities and limitations.
2. Mark outcomes as shipped only when their observable result is supported by evidence.
3. Describe remaining gaps without embedding temporary debugging notes or a historical
   correction narrative.
4. Order immediate priorities by dependency and product impact.
5. Link active work to canonical GitHub issues.
6. Keep the next milestones visible at an outcome level, but avoid speculative file-level
   plans for distant work.
7. Preserve established long-term capabilities such as deck construction and team formats
   unless an explicit product or architectural decision removes them.
8. Remove stale claims, duplicate task lists, and issue references that no longer own work.

The roadmap is a current planning source, not a changelog. Git history owns the record of how
its wording changed.

## Phase 5: Define the next batch

Select the smallest coherent set of highest-priority work that advances the active milestone.
Prioritize, in order:

1. blockers that prevent the primary user path from functioning;
2. correctness, security, privacy, data-loss, and contract risks;
3. missing affordances or feedback that make shipped behavior unusable;
4. dependencies that unlock several later issues; and
5. isolated enhancements that do not block the milestone.

Before creating an issue, search for an existing owner. Update an existing issue when its core
outcome is still correct; create a new issue when the outcome is distinct or the old issue is
closed around genuinely completed scope.

Each issue must be small enough for one focused pull request and detailed enough that another
agent can implement it without reconstructing this continuance session. Include:

### Issue format

- **Title:** concise, scoped, and phrased as an outcome or defect.
- **Context:** what currently happens, why it matters, and the evidence that exposed the gap.
- **Outcome:** the user-visible or architectural result required.
- **Scope:** the layers and contracts expected to change, without prescribing incidental code
  structure unless architecture requires it.
- **Acceptance criteria:** observable, testable conditions. Include relevant failure paths,
  redaction rules, reconnect behavior, accessibility, or performance bounds.
- **Test evidence:** the minimum test boundary that can prove the outcome and the repository
  gates that must remain green.
- **Dependencies:** prerequisite issues, contracts, or decisions; state whether work may run in
  parallel.
- **Estimated scope and risk:** a rough size and the primary source of implementation or review
  risk. Use this for batching, not as a substitute for acceptance criteria.
- **Non-goals:** only genuinely adjacent work excluded from this issue. Never list established
  roadmap capabilities as project-wide exclusions.
- **Documentation:** specifications, ADRs, or agent instructions that must change with the code.

Use labels, milestone assignments, and cross-links consistently with the existing tracker.
Create as many issues as the natural batch requires—possibly none, and never an arbitrary
fixed count. Make dependency order explicit when issues cannot be implemented independently.

## Phase 6: Final consistency check

Before finishing:

1. Confirm the roadmap, issue tracker, and repository describe the same current capabilities.
2. Confirm every new or materially revised issue has measurable acceptance criteria and a
   clear owner outcome.
3. Confirm dependencies and ordering are represented in issue bodies and roadmap priorities.
4. Check that deferred roadmap features were not accidentally converted into non-goals.
5. Check documentation links, terminology, milestone names, and issue references.
6. Review the documentation diff for speculative prose, repeated information, and accidental
   deletion of durable rationale.
7. Run validation proportionate to the changes and follow the repository's required gate before
   publishing a pull request.

## Required final report

Return a concise report with these sections:

1. **Evidence reviewed:** repository range, relevant tests, roadmap sections, and tracker scope.
2. **Completed outcomes:** what is demonstrably available now, with deviations noted.
3. **Roadmap changes:** claims added, removed, reopened, or reordered and why.
4. **Tracker actions:** issues created, updated, reopened, closed, or superseded, with links.
5. **Next batch:** ordered issues, dependencies, and the milestone outcome each advances.
6. **Blockers and uncertainties:** only genuine unresolved constraints or evidence gaps.
7. **Validation:** checks run and their results.

## Success criteria

- The roadmap reflects verified repository behavior rather than tracker optimism.
- Active milestone outcomes are concrete and measurable.
- The next batch is prioritized, dependency-aware, and implementable as focused pull requests.
- Existing issues are reused where appropriate and no duplicate or quota-driven issues exist.
- Long-term roadmap scope is preserved unless explicitly changed by a decision.
- Documentation contains conclusions and contracts, not the session's exploratory thought
  process.
- The final report provides a clear handoff from completed work to the next implementation
  batch.
