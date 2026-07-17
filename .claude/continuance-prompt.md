# Project continuation workflow

Use this prompt after completing a related batch of issues. Its purpose is to reconcile
the roadmap with shipped work and prepare only the next actionable batch.

## Reconcile

1. Read `docs/roadmap.md`, recent merged commits, and the current open issues.
2. Verify completed work against code and tests; do not infer completion from issue state alone.
3. Update the roadmap to describe current outcomes, remaining gaps, and dependencies. Avoid
   percentages and speculative implementation detail.

## Plan

1. Identify the smallest set of high-priority gaps in the active milestone.
2. Reuse or refine existing issues before creating new ones.
3. When a new issue is necessary, give it a user-visible outcome, measurable acceptance
   criteria, dependencies, and a scope that fits one focused PR.
4. Plan only as far ahead as current evidence supports.

## Finish

- Confirm the roadmap and issue tracker agree.
- Summarize completed outcomes, the next batch, and genuine blockers.
- Do not create issues or edit the roadmap merely to satisfy a quota.
