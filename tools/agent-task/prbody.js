/** The acceptance criteria the issue actually states, in order. */
export function acceptanceCriteria(body) {
  return [...(body || "").matchAll(/^\s*[-*]\s*\[[ xX]\]\s*(.+)$/gm)].map((m) => m[1].trim());
}

function gateTable(gates) {
  if (gates.length === 0) return "_No gates were run._";
  return [
    "| Gate | Result | Time |",
    "|---|---|---|",
    ...gates.map((g) => `| \`${g.gate}\` | ${g.ok ? "✅ pass" : "❌ fail"} | ${Math.round(g.duration_ms / 1000)}s |`),
  ].join("\n");
}

/**
 * Maps each acceptance criterion to the evidence for it.
 *
 * The mapping is **provider-reported** and labelled as such: the runner can observe that the
 * gates went green and which files moved, but it cannot know which criterion a given hunk was
 * meant to satisfy. Criteria the provider did not map are listed as unmapped rather than
 * quietly dropped — an unmapped criterion is the most useful thing on this page, because it
 * is where the human reviewer should look first.
 */
function criteriaTable(criteria, reported) {
  if (criteria.length === 0) return "_The issue states no acceptance criteria._";

  const claimed = new Map((reported ?? []).map((c) => [String(c.criterion ?? c.text ?? "").trim(), c.evidence]));
  const rows = criteria.map((criterion) => {
    const evidence = claimed.get(criterion);
    return `| ${evidence ? "☑" : "⚠️"} | ${criterion} | ${evidence ? String(evidence).replace(/\n/g, " ") : "**unmapped — no evidence reported**"} |`;
  });

  const unmapped = criteria.filter((c) => !claimed.has(c)).length;
  return [
    unmapped > 0
      ? `> ⚠️ **${unmapped} of ${criteria.length} criteria have no reported evidence.** Read those first.`
      : `All ${criteria.length} criteria have reported evidence. The mapping is the provider's claim, not the runner's finding — verify it.`,
    "",
    "| | Criterion | Evidence (provider-reported) |",
    "|---|---|---|",
    ...rows,
  ].join("\n");
}

export function buildPrBody({ issue, run, gates, files, ciPaths, providerUsage }) {
  const sections = [
    `Closes #${issue.number}.`,
    "",
    `Opened by \`scripts/agent-task\` (run \`${run.run_id}\`, provider \`${run.provider}\`, isolation \`${run.isolation}\`).`,
    "The runner produced this commit, ran the gates, and opened this PR; it cannot approve or merge.",
    "",
    "## Acceptance criteria",
    "",
    criteriaTable(acceptanceCriteria(issue.body), providerUsage?.criteria),
    "",
    "## Verification (runner-observed)",
    "",
    gateTable(gates),
    "",
    `## Changed files (${files.length})`,
    "",
    files.map((f) => `- \`${f}\``).join("\n"),
  ];

  if (ciPaths.length > 0) {
    // The run had to be started with --allow-ci to get here. The bot holds `workflows: write`,
    // so it can change the checks that gate this very PR — including hollowing out a required
    // job while keeping its name, which reports green. Only a human reading the diff catches
    // that, so it goes at the top and it is loud.
    sections.splice(
      3,
      0,
      "> ## ⚠️ CI-governance changes",
      ">",
      "> This PR modifies paths that gate the runner itself. A change here can weaken the checks",
      "> that are reporting green on this very PR. Read these hunks first, and read them carefully:",
      ">",
      ...ciPaths.map((p) => `> - \`${p}\``),
      "",
    );
  }

  if (providerUsage && Object.keys(providerUsage).length > 0) {
    sections.push(
      "",
      "<details><summary>Provider-reported usage (advisory, not comparable across providers)</summary>",
      "",
      "```json",
      JSON.stringify({ ...providerUsage, criteria: undefined }, null, 2),
      "```",
      "",
      "</details>",
    );
  }

  return `${sections.join("\n")}\n`;
}
