/**
 * Publishing: a non-approving PR review, and the `AI Review` check run.
 *
 * The single most important line in this file is the conclusion rule:
 *
 *   > A completed review **passes** the check, however many findings it contains.
 *   > An infrastructure failure **fails** it.
 *
 * That is ADR 0015's "required to run, advisory findings", and it is backwards from what people
 * expect a check to mean, so it is worth saying plainly: this check is not a verdict on the
 * code. It is a verdict on whether the *review happened*. Making findings fail the check would
 * hand an unproven, probabilistic tool a merge veto — the thing ADR 0015 explicitly defers until
 * the measurement window closes and a human-approved ADR promotes a category. Letting an outage
 * pass would be worse: a skipped review is indistinguishable from a clean one, which is exactly
 * the failure the runner's `report` subcommand exists to detect today.
 */

import { CAPS, CHECK_NAME, REVIEWER_VERSION, SEVERITIES } from "./config.js";
import { countBySeverity, neutralize } from "./schema.js";

/** The PR review body. Findings are neutralized: the model read a diff an attacker wrote. */
export function reviewBody({ findings, manifest, provider, model, dropped = [], findings_truncated = false }) {
  const lines = [];
  lines.push(`### 🤖 AI review — ${findings.length} finding${findings.length === 1 ? "" : "s"}`);
  lines.push("");
  lines.push(
    "_Advisory. This review cannot approve, block, or substitute for human approval " +
      "([ADR 0015](../blob/main/docs/decisions/0015-independent-ai-pr-review.md)); a human reviewer weighs these. " +
      "The reviewer saw only the diff and the base branch's own constraint documents — never this PR's._",
  );
  lines.push("");

  if (manifest.truncated) {
    const kinds = [...new Set(manifest.truncation.map((t) => t.kind))].join(", ");
    lines.push(`> ⚠️ **The reviewer did not see the whole change** (truncated: ${kinds}). Findings below are from a partial diff.`);
    lines.push("");
  }

  if (findings.length === 0) {
    lines.push("No defects, regressions, security issues, architecture violations, or missing tests found.");
    lines.push("");
    lines.push("_An empty review is a real result, not a passing grade: the reviewer is one opinion and it is not calibrated yet._");
  } else {
    for (const f of findings) {
      const where = f.path ? `\`${neutralize(f.path)}\`${f.line ? `:${f.line}` : ""}` : "_(no location)_";
      lines.push(`#### ${severityIcon(f.severity)} ${f.severity} · ${f.category} — ${neutralize(f.title)}`);
      lines.push("");
      lines.push(`**Where:** ${where}${f.off_diff_path ? ` _(model cited \`${neutralize(f.off_diff_path)}\`, which this diff does not touch)_` : ""}`);
      lines.push("");
      lines.push(`**Risk:** ${neutralize(f.risk)}`);
      lines.push("");
      lines.push(`**Recommendation:** ${neutralize(f.recommendation)}`);
      lines.push("");
      lines.push(`<sub>\`${f.id}\`</sub>`);
      lines.push("");
    }
  }

  if (findings_truncated) {
    lines.push(`> ⚠️ **The reviewer reported more than ${CAPS.findings} findings; only the ${CAPS.findings} most severe are shown.**`);
    lines.push("");
  }

  if (dropped.length > 0) {
    lines.push(
      `<sub>${dropped.length} malformed finding(s) were discarded: ${neutralize(dropped.map((d) => d.reason).join("; "))}</sub>`,
    );
    lines.push("");
  }

  lines.push(`<sub>reviewer ${REVIEWER_VERSION} · ${provider}/${model} · head \`${manifest.head_sha.slice(0, 12)}\`</sub>`);
  return lines.join("\n");
}

function severityIcon(severity) {
  return { critical: "🔴", high: "🟠", medium: "🟡", low: "⚪" }[severity] ?? "⚪";
}

/** The check-run summary: counts, truncation, reviewer identity, and completed-vs-infra status. */
export function checkSummary({ findings, manifest, provider, model, dropped = [], findings_truncated = false }) {
  const counts = countBySeverity(findings);
  const lines = [
    `**Completed.** reviewer ${REVIEWER_VERSION} · ${provider}/${model}`,
    "",
    `| ${SEVERITIES.join(" | ")} | total |`,
    `|${SEVERITIES.map(() => "---").join("|")}|---|`,
    `| ${SEVERITIES.map((s) => counts[s]).join(" | ")} | ${findings.length} |`,
    "",
    `Files reviewed: ${manifest.file_count} of ${manifest.changed_path_count} changed · input ${manifest.input_bytes} bytes`,
    manifest.truncated
      ? `⚠️ Truncated: ${manifest.truncation.map((t) => t.kind).join(", ")} — the reviewer saw a partial diff.`
      : "Not truncated: the reviewer saw the whole diff.",
    dropped.length > 0 ? `${dropped.length} malformed finding(s) discarded.` : "",
    findings_truncated ? `⚠️ More findings than the cap of ${CAPS.findings}; only the most severe are shown.` : "",
    "",
    "Findings are **advisory** and do not block this merge. This check reports only that the review *ran*.",
  ];
  return lines.filter(Boolean).join("\n");
}

/** The check-run summary for an outage. Fails the check — an outage is not a clean review. */
export function failureSummary(error, { manifest = null, provider = null } = {}) {
  return [
    "**Infrastructure failure — the review did not complete.**",
    "",
    "```",
    String(error?.message ?? error).slice(0, 800),
    "```",
    "",
    provider ? `Provider: ${provider}` : "",
    manifest ? `Head: \`${manifest.head_sha}\`` : "",
    "",
    "This is *not* a clean review: nothing was reviewed. Re-run the failed job once the cause is fixed. " +
      "A skipped or failed review must never be mistaken for a passing one — which is why this check fails " +
      "instead of quietly succeeding.",
  ]
    .filter(Boolean)
    .join("\n");
}

/**
 * Publishes both artifacts. `COMMENT`, never `APPROVE` and never `REQUEST_CHANGES`: the first
 * would satisfy a human-approval requirement it must not satisfy, and the second would be a
 * merge veto by an uncalibrated tool.
 */
export async function publish(gh, { prNumber, headSha, body, summary, conclusion, title }) {
  await gh.request("POST", `/repos/${gh.owner}/${gh.repo}/check-runs`, {
    name: CHECK_NAME,
    head_sha: headSha,
    status: "completed",
    conclusion,
    completed_at: new Date().toISOString(),
    output: { title, summary },
  });

  if (body !== null) {
    await gh.request("POST", `/repos/${gh.owner}/${gh.repo}/pulls/${prNumber}/reviews`, {
      commit_id: headSha,
      event: "COMMENT",
      body,
    });
  }
}
