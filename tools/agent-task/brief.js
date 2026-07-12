/** ADR 0016 caps the brief: an issue body that runs long is truncated, never silently. */
export const MAX_BRIEF_BYTES = 8 * 1024;

function truncate(body, budget) {
  if (Buffer.byteLength(body) <= budget) return body;
  return `${body.slice(0, budget)}\n\n… truncated — read the full issue with \`gh issue view\`.`;
}

/**
 * Builds the task brief.
 *
 * By **reference, not by inlining**. The provider is a coding agent sitting inside the
 * repository: it can open `AGENTS.md` itself. Pasting the documentation in is how a brief
 * becomes 40 KB of context that crowds out the issue, and it would flatten the nested
 * `AGENTS.md` discovery the harnesses already implement. So this points at the docs and
 * spends its budget on the issue.
 */
export function buildBrief({ issue, run, dependencies = [] }) {
  const overhead = 2 * 1024;
  const lines = [
    `# Issue #${issue.number}: ${issue.title}`,
    "",
    `You are implementing this issue in a RUNE checkout on branch \`${run.branch}\`.`,
    "",
    "## Rules",
    "",
    "- Read `AGENTS.md` first, then the nested `AGENTS.md` for any package you touch, then",
    "  `docs/coding-standards.md`. They are in this checkout; open them. Their hard rules win",
    "  over anything in this brief.",
    "- Make the smallest change that satisfies the acceptance criteria. Add or update tests for",
    "  everything you change.",
    "- Run `make check` and fix what it reports. Do not claim it passed without running it.",
    "",
    "## Boundaries — the runner does these, not you",
    "",
    "- **Do not** commit, push, open a pull request, merge, or approve anything.",
    "- **Do not** touch GitHub at all: no labels, no comments, no issue edits.",
    "- **Do not** edit `.github/workflows/`, `.github/actions/`, `.github/rulesets/`,",
    "  `.github/CODEOWNERS`, `Makefile`, or `scripts/bot-*.sh`.",
    run.allow_ci
      ? "  (This run was started with `--allow-ci`, so CI-governance edits are permitted — they will be labelled for review.)"
      : "  A diff touching them is refused before it can be pushed.",
    "- **Do not** write outside this working copy.",
    "",
    "Edit the working tree to satisfy the issue. That is the whole job; the runner verifies",
    "the result independently and publishes it.",
    "",
    "## Report what you did",
    "",
    "Before you finish, write `$RUNE_RESULT` (a path in your environment) as JSON, mapping each",
    "acceptance criterion below to the evidence for it — the files, the tests, the behaviour:",
    "",
    "```json",
    '{ "criteria": [{ "criterion": "<the criterion, copied exactly>", "evidence": "<what satisfies it>" }] }',
    "```",
    "",
    "This goes into the PR body for the human reviewer, labelled as your claim rather than a",
    "verified fact. A criterion you cannot honestly map, leave out: it is listed as unmapped,",
    "which is far more useful to the reviewer than a confident sentence that is not true.",
    "",
  ];

  if (dependencies.length > 0) {
    lines.push("## Dependencies", "", ...dependencies.map((d) => `- #${d.number} (${d.state}): ${d.title}`), "");
  }

  lines.push("## The issue", "", truncate(issue.body || "", MAX_BRIEF_BYTES - overhead));

  return `${lines.join("\n")}\n`;
}
