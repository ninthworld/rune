import { LABELS } from "./config.js";

/** `agent/<issue>-<slug>` — the canonical branch name, and therefore the claim (ADR 0016). */
export function branchName(issue) {
  const slug = issue.title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/, "");
  return `agent/${issue.number}-${slug}`;
}

/**
 * Issue numbers this issue declares itself blocked by.
 *
 * Reads the `Blocked by:` list the agent-task template produces, taking the `#N` links
 * that follow it. Anything before that heading (a "Closes #N", a prose reference) is not
 * a dependency and must not be treated as one.
 */
export function declaredDependencies(body) {
  const match = /blocked by:?\s*\n([\s\S]*?)(?:\n\s*\n|\n#{1,6}\s|$)/i.exec(body || "");
  if (!match) return [];
  const numbers = [...match[1].matchAll(/#(\d+)/g)].map((m) => Number(m[1]));
  return [...new Set(numbers)];
}

export function hasAcceptanceCriteria(body) {
  return /^\s*[-*]\s*\[[ xX]\]/m.test(body || "");
}

/**
 * Decides whether an issue may be claimed. Reads only — every check here runs before the
 * first GitHub mutation, so a rejected task leaves no trace.
 *
 * `openDependencies` is supplied by the caller (which fetches them) to keep this pure.
 */
export function preflight(issue, openDependencies = []) {
  const errors = [];
  const warnings = [];
  const labels = (issue.labels || []).map((l) => (typeof l === "string" ? l : l.name));

  if (issue.state !== "open") errors.push(`issue #${issue.number} is ${issue.state}`);
  if (issue.pull_request) errors.push(`#${issue.number} is a pull request, not an issue`);

  if (labels.includes(LABELS.blocked)) errors.push(`labelled ${LABELS.blocked}`);
  if (labels.includes(LABELS.inProgress)) errors.push(`already claimed (${LABELS.inProgress})`);
  if (labels.includes(LABELS.review)) errors.push(`already in review (${LABELS.review})`);
  if (!labels.includes(LABELS.ready)) errors.push(`not labelled ${LABELS.ready}`);

  if (openDependencies.length > 0) {
    errors.push(`blocked by open ${openDependencies.map((n) => `#${n}`).join(", ")}`);
  }

  if (!issue.body || issue.body.trim() === "") {
    errors.push("malformed: empty body — nothing to build a task brief from");
  } else if (!hasAcceptanceCriteria(issue.body)) {
    warnings.push("no acceptance-criteria checkboxes found; the PR body cannot map criteria to evidence");
  }

  return { ok: errors.length === 0, errors, warnings };
}
