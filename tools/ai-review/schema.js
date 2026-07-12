/**
 * The finding schema, and the stable IDs that make findings comparable across runs.
 *
 * A model's output is untrusted input like any other. It arrives as text, it may be malformed,
 * it may invent a severity, it may claim a file that is not in the diff, and it may — because
 * the diff it read is attacker-controlled — try to smuggle instructions or markup into a field
 * that ends up in a PR comment. Everything below assumes that and validates rather than trusts.
 */

import { createHash } from "node:crypto";

import { CAPS, CATEGORIES, REVIEWER_VERSION, SEVERITIES } from "./config.js";

export class InvalidFindings extends Error {}

/**
 * A finding's stable ID: the same defect, reported on the same head SHA by the same reviewer
 * version, gets the same ID on a rerun. That is what makes a rerun idempotent and what lets
 * #244 attach a human disposition to a finding that survives across records.
 *
 * Deliberately derived from *content*, not from a counter: a counter renumbers every finding
 * when the model happens to emit them in a different order, which would orphan every
 * disposition attached to them.
 */
export function findingId({ headSha, category, path, line, title }) {
  const key = [REVIEWER_VERSION, headSha, category, path ?? "", line ?? "", title].join(" ");
  return `f_${createHash("sha256").update(key).digest("hex").slice(0, 12)}`;
}

/** Trims a model-supplied string to a bounded, single-purpose scalar. */
function scalar(value, max) {
  if (typeof value !== "string") return null;
  const clean = value.replace(/\s+/g, " ").trim();
  return clean === "" ? null : clean.slice(0, max);
}

/**
 * Neutralizes text that will be posted into a PR comment.
 *
 * The model read an attacker-controlled diff, so its output may echo attacker-controlled text.
 * Posting that verbatim into a Markdown comment lets a PR author inject a fake `@mention`, a
 * `<!-- -->` comment that hides content from a reader, or a fenced block that breaks out of the
 * table this lands in. None of that is an exploit of *us*; all of it is a way to make a human
 * reviewer read something other than what the reviewer said.
 */
export function neutralize(text) {
  return String(text)
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\|/g, "\\|")
    .replace(/`/g, "'")
    .replace(/@/g, "@​") // zero-width space: renders as @, never notifies
    .replace(/\r?\n/g, " ");
}

/**
 * Validates and normalizes what the adapter returned.
 *
 * Rejects the whole payload when it is not the agreed shape — a reviewer whose output we cannot
 * parse is an infrastructure failure, not a clean review, and the difference is exactly what
 * ADR 0015 requires the check run to preserve.
 *
 * Individual *findings* are dropped rather than fatal, because one malformed finding among
 * twelve good ones should not throw the review away — but every drop is counted and reported,
 * so a reviewer quietly losing half its output is visible instead of silent.
 */
export function normalizeFindings(raw, { headSha, changedPaths = null } = {}) {
  if (raw === null || typeof raw !== "object" || !Array.isArray(raw.findings)) {
    throw new InvalidFindings("adapter result has no `findings` array");
  }

  const dropped = [];
  const findings = [];

  for (const [i, item] of raw.findings.entries()) {
    if (item === null || typeof item !== "object") {
      dropped.push({ index: i, reason: "not an object" });
      continue;
    }

    const title = scalar(item.title, 200);
    const severity = scalar(item.severity, 20)?.toLowerCase();
    const category = scalar(item.category, 40)?.toLowerCase();
    const risk = scalar(item.risk, 1_000);
    const recommendation = scalar(item.recommendation, 1_000);

    if (!title) {
      dropped.push({ index: i, reason: "no title" });
      continue;
    }
    if (!SEVERITIES.includes(severity)) {
      dropped.push({ index: i, reason: `severity ${JSON.stringify(item.severity)} is not one of ${SEVERITIES.join("/")}` });
      continue;
    }
    if (!CATEGORIES.includes(category)) {
      dropped.push({ index: i, reason: `category ${JSON.stringify(item.category)} is not one of ${CATEGORIES.join("/")}` });
      continue;
    }
    if (!risk || !recommendation) {
      dropped.push({ index: i, reason: "a finding must state both a risk and a recommendation" });
      continue;
    }

    const path = scalar(item.path, 300);
    const line = Number.isInteger(item.line) && item.line > 0 ? item.line : null;

    // A finding about a file the diff never touched is either a hallucination or the model
    // wandering off the change under review. Report it, but strip the location: an inline
    // comment anchored to a line that is not in the diff would fail to post anyway.
    const inDiff = path !== null && (changedPaths === null || changedPaths.includes(path));

    findings.push({
      id: findingId({ headSha, category, path: inDiff ? path : null, line: inDiff ? line : null, title }),
      severity,
      category,
      path: inDiff ? path : null,
      line: inDiff ? line : null,
      off_diff_path: !inDiff && path !== null ? path : undefined,
      title,
      risk,
      recommendation,
    });
  }

  findings.sort((a, b) => SEVERITIES.indexOf(a.severity) - SEVERITIES.indexOf(b.severity));

  const truncated = findings.length > CAPS.findings;
  return {
    findings: findings.slice(0, CAPS.findings),
    dropped,
    findings_truncated: truncated,
  };
}

/** Counts by severity, for the check-run summary. */
export function countBySeverity(findings) {
  const counts = Object.fromEntries(SEVERITIES.map((s) => [s, 0]));
  for (const f of findings) counts[f.severity]++;
  return counts;
}
