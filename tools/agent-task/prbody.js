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
 * The key both sides of the criterion-to-evidence join are compared on.
 *
 * The provider is asked to copy each criterion exactly, and a language model copying prose into
 * a JSON string does not: on #191 it dropped the inline code spans, so all six criteria that had
 * one reported as unmapped while their evidence sat in the result file matching nothing. Byte
 * equality is the wrong contract to hold a model to when the issue template's own criteria are
 * written in markdown. Compare on the words instead — code spans, emphasis, whitespace runs,
 * case, and trailing punctuation all fall out of the key.
 */
function joinKey(text) {
  return String(text ?? "")
    .toLowerCase()
    .replace(/[`*_]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[.,;:]+$/, "");
}

/**
 * Joins the issue's criteria to the provider's reported evidence.
 *
 * One claim satisfies at most one criterion: a claim is consumed when it is matched, so evidence
 * is never shown twice and never attributed to a criterion that already has its own. Whatever is
 * left over is returned, not discarded — a claim matching nothing is either a reworded criterion
 * or an invented one, and both are things the reviewer needs to see.
 */
function matchEvidence(criteria, reported) {
  const unclaimed = new Map();
  for (const claim of reported ?? []) {
    const criterion = String(claim.criterion ?? claim.text ?? "").trim();
    const evidence = claim.evidence ? String(claim.evidence).replace(/\s+/g, " ").trim() : "";
    if (!evidence) continue;

    const key = joinKey(criterion);
    if (!unclaimed.has(key)) unclaimed.set(key, []);
    unclaimed.get(key).push({ criterion, evidence });
  }

  const rows = criteria.map((criterion) => ({
    criterion,
    evidence: unclaimed.get(joinKey(criterion))?.shift()?.evidence,
  }));

  return { rows, unmatched: [...unclaimed.values()].flat() };
}

/**
 * Maps each acceptance criterion to the evidence for it.
 *
 * The mapping is **provider-reported** and labelled as such: the runner can observe that the
 * gates went green and which files moved, but it cannot know which criterion a given hunk was
 * meant to satisfy. Criteria the provider did not map are listed as unmapped rather than
 * quietly dropped — an unmapped criterion is the most useful thing on this page, because it
 * is where the human reviewer should look first. That only holds if the warning is trustworthy,
 * so the join tolerates reformatting (`joinKey`) and nothing the provider reported is dropped.
 */
function criteriaTable(criteria, reported) {
  if (criteria.length === 0) return "_The issue states no acceptance criteria._";

  const { rows, unmatched } = matchEvidence(criteria, reported);
  const unmapped = rows.filter((r) => !r.evidence).length;

  const table = [
    unmapped > 0
      ? `> ⚠️ **${unmapped} of ${criteria.length} criteria have no reported evidence.** Read those first.`
      : `All ${criteria.length} criteria have reported evidence. The mapping is the provider's claim, not the runner's finding — verify it.`,
    "",
    "| | Criterion | Evidence (provider-reported) |",
    "|---|---|---|",
    ...rows.map(
      ({ criterion, evidence }) =>
        `| ${evidence ? "☑" : "⚠️"} | ${criterion} | ${evidence ?? "**unmapped — no evidence reported**"} |`,
    ),
  ];

  if (unmatched.length > 0) {
    table.push(
      "",
      `> ⚠️ **${unmatched.length} reported ${unmatched.length === 1 ? "claim matches" : "claims match"} no criterion in this issue.**`,
      "> Reworded, or answering something the issue never asked. Reproduced here rather than dropped:",
      ">",
      ...unmatched.map(({ criterion, evidence }) => `> - _${criterion || "(no criterion given)"}_ — ${evidence}`),
    );
  }

  return table.join("\n");
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
