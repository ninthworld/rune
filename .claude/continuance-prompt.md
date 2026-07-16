# Project Continuance Prompt

Use this prompt when starting a continuance session after completing a batch of issues. It standardizes the review, planning, and issue-creation workflow.

---

## Phase 1: Assess Completed Work

1. Review `docs/roadmap.md` to understand the current milestone structure and exit criteria
2. Check the git log since the last continuance to see what was implemented
3. For each completed issue/feature, verify it matches the roadmap milestone it was targeting
4. Update `docs/roadmap.md`:
   - Mark completed items as ✓ Done
   - Note any deviations from the plan
   - Update milestone status (% complete)

## Phase 2: Validate Current Milestone Exit Criteria

1. For the **current active milestone** (the one we're working toward):
   - List all exit criteria from the roadmap
   - Check if any are already satisfied by completed work
   - Identify which remain open
2. If exit criteria are too vague (e.g., "implement spell system"), break them into measurable parts:
   - What user-facing behavior proves this is done?
   - What edge cases must it handle?
   - What performance/quality bar must it meet?

## Phase 3: Define Next Batch of Issues

1. Identify the **highest-priority remaining work** for the current milestone
2. For each priority item, create a detailed issue spec with:
   - **Title**: concise, user-facing outcome (not "implement X", but "enable players to cast spells")
   - **Acceptance Criteria**: testable conditions that prove it's done
   - **Dependencies**: what must be done first
   - **Estimated Scope**: rough T-shirt size (S/M/L) to guide focus
3. Create these issues in GitHub with the spec details in the body

## Phase 4: Roadmap Update & Summary

1. Update `docs/roadmap.md` to reflect:
   - Progress on the current milestone (% complete, blockers if any)
   - Planned issues for the next batch (link to GitHub issue URLs)
   - Visible next 2–3 milestones (don't plan beyond your visibility)
2. Provide a summary: "**Completed**: [list], **Next Batch**: [list], **Current Blockers**: [if any]"

---

## Success Criteria

- ✓ Roadmap is updated to current state
- ✓ Exit criteria for current milestone are granular & measurable
- ✓ 3–5 GitHub issues created with acceptance criteria
- ✓ Issues are scoped small enough to complete in a single session
- ✓ Summary report shows clear continuity from last session

---

## Usage

After finishing a batch of issues, run Claude with this exact prompt (you can reference it as "the continuance prompt in `.claude/continuance-prompt.md`"). The structure repeats each session, ensuring consistent review and planning.
