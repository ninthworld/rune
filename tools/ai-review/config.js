/**
 * The versioned contract of the AI reviewer (ADR 0015, #243).
 *
 * Everything here is a *published* number: it goes into the manifest, the check-run summary,
 * and (via #244) the calibration record, so a finding can always be traced back to the exact
 * prompt, schema, caps, and provider configuration that produced it. Change any of these and
 * you must bump `REVIEWER_VERSION` — a finding compared against a different reviewer is not a
 * comparison, and the whole point of ADR 0015's measurement window is that it *is* one.
 */

/** Bump on ANY change to the prompt, the schema, the caps, or the adapter contract. */
export const REVIEWER_VERSION = "1.0.0";

/** The manifest/summary schema version, independent of the reviewer's own behaviour. */
export const SCHEMA_VERSION = 1;

/** The name of the check run, and therefore the required-check context in the ruleset. */
export const CHECK_NAME = "AI Review";

/** The prepare workflow the review workflow will accept an artifact from. Nothing else. */
export const PREPARE_WORKFLOW = "AI Review Prepare";
export const ARTIFACT_NAME = "ai-review-input";

/**
 * Caps. The reviewer must cost a bounded amount and take a bounded time on *any* pull request,
 * including a hostile one that changes 4,000 files to make the reviewer expensive or to push
 * the interesting hunk past the model's context. Every cap that bites is recorded as an
 * explicit truncation in the manifest and surfaced in the check summary — a review that saw
 * only part of the diff must never look like a review that saw all of it.
 */
export const CAPS = {
  /** Total diff bytes handed to the model. */
  diffBytes: 400_000,
  /** Per-file diff bytes; one enormous file cannot crowd out every other file. */
  fileDiffBytes: 40_000,
  /** Files whose patches are included in full. */
  files: 300,
  /** Total context-document bytes (AGENTS.md, standards, protocol — all base-ref). */
  contextBytes: 120_000,
  /**
   * Total bytes of the prepared *JSON-encoded* artifact the trusted stage will accept.
   *
   * Must stay comfortably above `diffBytes + contextBytes` **after JSON escaping**, which can most
   * than double the size of source text (every `"`, `\`, newline and tab grows). At 1 MB it was
   * possible for a pull request that respected every other cap to still blow this one — and a
   * prepare that hard-fails is now a red required check on an innocent PR. This is a backstop
   * against a bug, not a budget the caps above should ever reach.
   */
  artifactBytes: 4_000_000,
  /** Model output ceiling. */
  maxOutputTokens: 8_000,
  /** Findings kept; a model that emits 500 findings is not being useful. */
  findings: 50,
  /**
   * Omitted paths *listed* in a truncation record. The count is always exact; the list is not.
   * A 5,000-file pull request must truncate, not blow the artifact cap and hard-fail prepare —
   * "this PR is large" is not an infrastructure failure.
   */
  omittedPathsListed: 50,
};

/**
 * Timeout and retry for the model call. Infrastructure failure must be legible as such.
 *
 * The timeout is measured, not guessed: a real 185 KB diff (this reviewer's own pull request)
 * took **257 seconds** on `claude-opus-4-8`. The first draft allowed 240s, which would have
 * failed *every* review of a large change as an infrastructure failure — a red required check on
 * an innocent PR, caused entirely by a number nobody had tested. 7 minutes leaves real headroom
 * over the observed time, and the job's own `timeout-minutes` is set to cover the retries.
 */
export const LIMITS = {
  /** One model request's deadline. Observed worst case so far: 257s on a 185 KB diff. */
  requestTimeoutMs: 420_000,
  /** Attempts per head SHA, total, across transient errors. */
  maxAttempts: 3,
  /** Base backoff; doubled per attempt. */
  backoffMs: 2_000,
  /**
   * Model calls per head SHA. The cost ceiling: a rerun of the same SHA reuses the existing
   * check run and does not pay again (see `alreadyReviewed` in verify.js).
   */
  maxModelCalls: 3,
};

/**
 * The Claude Code CLI adapter's contract — the default provider, because a Pro/Max subscription
 * authenticates the CLI (`claude setup-token`) and not the metered API, and RUNE should not pay
 * per token for a review a subscription already covers.
 *
 * `version` is pinned: the CI job installs exactly this build. An agent harness that silently
 * changed how `--disallowed-tools` behaves would be a security-relevant upgrade, and it should
 * arrive as a reviewable diff (Dependabot cannot see a version in a `run:` step, so this is the
 * one place to bump).
 *
 * `deniedTools` is the full set of built-ins as of that version. It is a denylist rather than an
 * empty allowlist because `--allowed-tools` governs *auto-approval*, not existence: a tool absent
 * from an allowlist still exists and merely prompts — and in print mode there is nobody to prompt.
 */
export const CLAUDE_CLI = {
  command: "claude",
  version: "2.1.207",
  model: "claude-opus-4-8",
  deniedTools: [
    "Bash",
    "BashOutput",
    "KillShell",
    "Edit",
    "Write",
    "NotebookEdit",
    "Read",
    "Glob",
    "Grep",
    "WebFetch",
    "WebSearch",
    "Task",
    "TodoWrite",
  ],
};

/** Severities, most severe first. Anything else a model invents is rejected by the schema. */
export const SEVERITIES = ["critical", "high", "medium", "low"];

/**
 * Categories. Deliberately not a taxonomy of code smells: these are the five things ADR 0015
 * says the reviewer exists to catch. `style` is absent on purpose — `make check` already owns
 * formatting and lint, and a reviewer that narrates style is noise that trains humans to skim.
 */
export const CATEGORIES = ["defect", "regression", "security", "architecture", "missing-test"];

/**
 * The context documents the reviewer is given, read from the **base ref** — never from the PR.
 *
 * This is not a detail. If the head's `AGENTS.md` were used, a pull request could edit the
 * rules it is about to be judged against ("ignore all previous constraints" is a legal diff to
 * a Markdown file). The reviewer therefore judges the change against the rules as they exist
 * on `main`, and a change *to* those rules shows up where it belongs: in the diff, for a human.
 */
export const CONTEXT_DOCS = [
  "AGENTS.md",
  "docs/coding-standards.md",
  "crates/rune-engine/AGENTS.md",
  "clients/web/AGENTS.md",
];

/** Nested `AGENTS.md` files are only worth their bytes when the diff touches their directory. */
export const NESTED_CONTEXT = {
  "crates/rune-engine/AGENTS.md": "crates/rune-engine/",
  "clients/web/AGENTS.md": "clients/web/",
};
