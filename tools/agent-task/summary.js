import { redact } from "./redact.js";

export const SUMMARY_SCHEMA_VERSION = 1;

/**
 * Terminal outcome → the stage it failed at.
 *
 * Normalized on purpose: #200 reports "where do runs die", and it cannot do that if one run
 * says `verification_failed` and another says `make check exploded`. `null` means the run did
 * not fail.
 */
const FAILURE_STAGE = {
  review: null,
  released: null,
  claim_lost: "claim",
  provider_failed: "provider",
  provider_timeout: "provider",
  cancelled: "provider",
  no_op: "diff_inspection",
  scope_rejected: "diff_inspection",
  ci_change_refused: "diff_inspection",
  verification_failed: "verify",
  rebase_conflict: "rebase",
  push_failed: "push",
  pr_failed: "pr",
};

export function failureStage(outcome) {
  // An unknown outcome is a bug, but it must not be silently reported as a success.
  return outcome in FAILURE_STAGE ? FAILURE_STAGE[outcome] : "unknown";
}

/** Diff size, bucketed. The count is useful; the contents are never telemetry. */
export function sizeBucket(fileCount) {
  if (fileCount <= 2) return "xs";
  if (fileCount <= 8) return "s";
  if (fileCount <= 20) return "m";
  return "l";
}

export function area(labels = []) {
  const names = labels.map((l) => (typeof l === "string" ? l : l.name));
  return names.filter((n) => n.startsWith("area:")).map((n) => n.slice("area:".length));
}

/**
 * Provider-reported usage, narrowed to fields that mean something.
 *
 * An allowlist, because a provider can write anything it likes into `result.json` — including,
 * accidentally, a chunk of the source it just read. Values are redacted as well as narrowed:
 * belt and braces on the one field in the schema that a provider controls.
 */
function usage(reported) {
  if (!reported || typeof reported !== "object") return null;
  const allowed = ["model", "tokens", "input_tokens", "output_tokens", "cost_usd", "turns", "duration_ms"];
  const out = {};
  for (const key of allowed) {
    const value = reported[key];
    if (typeof value === "number") out[key] = value;
    else if (typeof value === "string") out[key] = redact(value).slice(0, 200);
  }
  return Object.keys(out).length > 0 ? out : null;
}

/**
 * Builds the sanitized, versioned run summary (ADR 0016).
 *
 * Two rules hold this together. **Runner-observed beats provider-reported**: outcome, gates,
 * diff size, PR identity, and whether the ADR 0015 review ran are things the runner watched
 * happen; `provider_usage` is a claim, is marked as one, and is explicitly non-comparable
 * across providers. And **nothing here is a payload**: no prompts, no briefs, no diffs, no
 * logs, no environment, no secrets — a run summary is a record of *what happened*, not of what
 * was said.
 *
 * A summary confers no authority. Nothing may read one and thereby approve, merge, or pick a
 * provider.
 */
export function buildSummary(run, { issue, pr = null, review = null, now = new Date() } = {}) {
  return {
    schema_version: SUMMARY_SCHEMA_VERSION,
    run_id: run.run_id,
    resume_of: run.resume_of ?? null,
    supersedes: run.supersedes ?? null,

    issue: run.issue,
    issue_area: area(issue?.labels),
    issue_size: run.files ? sizeBucket(run.files.length) : null,

    provider: run.provider,
    isolation: run.isolation ?? null,
    gate_set: run.gate_set ?? null,
    allow_ci: Boolean(run.allow_ci),

    branch: run.branch,
    base_sha: run.base_sha ?? null,
    head_sha: run.head_sha ?? null,

    created_at: run.created_at,
    terminal_at: now.toISOString(),
    lifecycle: (run.events ?? []).map((e) => ({ state: e.state, at: e.at })),

    // Runner-observed.
    terminal_outcome: run.state,
    failure_stage: failureStage(run.state),
    gates: (run.gates ?? []).map((g) => ({ gate: g.gate, ok: g.ok, duration_ms: g.duration_ms })),
    ci_paths_touched: run.ci_paths ?? [],
    pr: pr ? { number: pr.number, author: pr.author, draft: pr.draft } : null,
    // ADR 0015's review is not a required check, so a silent skip is indistinguishable from a
    // pass unless someone looks. This is that look.
    review: review ?? { observed: false, ran: null, conclusion: null },

    // Provider-reported. Advisory, non-comparable, never decisive.
    provider_usage: usage(run.provider_usage),
  };
}
