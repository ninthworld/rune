/**
 * The deterministic half of RUNE's workflow gate (#199).
 *
 * actionlint answers "is this a valid workflow?" — schema, expressions, shellcheck. It does
 * not answer "is this workflow *allowed here*?", and the answers RUNE needs are the ones a
 * supply-chain attack turns on: a mutable Action reference, a token with more power than the
 * job's work requires, untrusted event text interpolated into a shell. Those are policy, not
 * validity, so they live here — mechanical, testable, and merge-blocking.
 *
 * Every rule rejects *structure*, never judgment (`docs/agents/continuance.md` §6). It can
 * see that `id-token: write` carries no explanation; it does not pretend to decide whether
 * the explanation is a good one. That is what the human reviewer is for.
 */

import { MalformedWorkflow, scan } from "./scan.js";

/** `owner/repo@<40-hex>`, optionally with a subdirectory (`owner/repo/path@<sha>`). */
const PINNED = /^[\w.-]+\/[\w.-]+(?:\/[\w./-]+)?@[0-9a-f]{40}$/;
/** The tag comment that keeps a pinned SHA reviewable — `# v4.3.1`, `# v2`. */
const VERSION_COMMENT = /^v?\d+(?:\.\d+)*\b/;
/** Event context an attacker controls: PR titles, bodies, branch names, comment text. */
const UNTRUSTED_EVENT = /\$\{\{\s*github\.event\b[^}]*\}\}/;

const RULES = {
  PINNED_ACTIONS: "pinned-actions",
  VERSION_COMMENT: "version-comment",
  PERMISSIONS_DECLARED: "permissions-declared",
  WRITE_JUSTIFIED: "write-justified",
  NO_PULL_REQUEST_TARGET: "no-pull-request-target",
  NO_UNTRUSTED_INTERPOLATION: "no-untrusted-interpolation",
  MALFORMED: "malformed",
};

export { RULES };

/** True when this node is a key inside a `permissions:` block anywhere in the file. */
function inPermissionsBlock(node) {
  return node.path.length >= 2 && node.path[node.path.length - 2] === "permissions";
}

/**
 * A `write` grant must say why, right where it is granted.
 *
 * Accepts a trailing comment on the same line or a comment line directly above it — the
 * point is that a reviewer reading the permission reads the reason without leaving it,
 * which is the acceptance criterion ("beside the step or job that requires it").
 */
function hasJustification(node, lines) {
  if (node.comment && node.comment.length > 0) return true;
  const above = lines[node.line - 2]; // node.line is 1-indexed; -2 is the line before it
  return typeof above === "string" && above.trim().startsWith("#") && above.trim().length > 1;
}

/**
 * @param {string} name  the workflow's filename, for messages
 * @param {string} text  its contents
 * @returns {Array<{rule: string, line: number, message: string}>} findings, empty if clean
 */
export function checkWorkflow(name, text) {
  const findings = [];
  const lines = text.split("\n");
  const add = (rule, line, message) => findings.push({ rule, line, message, workflow: name });

  let nodes;
  try {
    nodes = scan(text);
  } catch (err) {
    if (err instanceof MalformedWorkflow) {
      add(RULES.MALFORMED, err.line, err.message);
      return findings; // nothing further can be trusted about a file we cannot read
    }
    throw err;
  }

  // --- Immutable Action references -----------------------------------------------------
  for (const node of nodes) {
    if (node.key !== "uses") continue;
    const ref = node.value.replace(/^['"]|['"]$/g, "");

    // A local action is already as trusted as the repository itself; a repository SHA
    // would be the wrong reference for it, so local paths are exempt by design.
    if (ref.startsWith("./")) continue;

    if (!PINNED.test(ref)) {
      add(
        RULES.PINNED_ACTIONS,
        node.line,
        `\`${ref}\` is not pinned to a full-length commit SHA. A tag or branch is mutable: ` +
          "whoever can move it can change what runs in CI. Pin the 40-character SHA and keep " +
          "the release tag in a trailing comment.",
      );
      continue;
    }
    if (!node.comment || !VERSION_COMMENT.test(node.comment)) {
      add(
        RULES.VERSION_COMMENT,
        node.line,
        `\`${ref}\` is pinned but carries no version comment. Add the release tag (\`# v4.3.1\`) ` +
          "so the pin stays reviewable and Dependabot can bump it.",
      );
    }
  }

  // --- Least privilege ------------------------------------------------------------------
  const topLevelPermissions = nodes.some((n) => n.path.length === 1 && n.key === "permissions");
  const jobs = nodes.filter((n) => n.path.length === 2 && n.path[0] === "jobs");
  const jobsWithPermissions = new Set(
    nodes.filter((n) => n.path.length === 3 && n.path[0] === "jobs" && n.key === "permissions").map((n) => n.path[1]),
  );
  for (const job of jobs) {
    if (!topLevelPermissions && !jobsWithPermissions.has(job.key)) {
      add(
        RULES.PERMISSIONS_DECLARED,
        job.line,
        `job \`${job.key}\` declares no \`permissions:\`, and neither does the workflow. ` +
          "An undeclared token inherits the repository default, which can be widened without " +
          "touching this file. Declare it explicitly, starting from `contents: read`.",
      );
    }
  }

  for (const node of nodes) {
    if (!inPermissionsBlock(node) || node.value !== "write") continue;
    if (!hasJustification(node, lines)) {
      add(
        RULES.WRITE_JUSTIFIED,
        node.line,
        `\`${node.key}: write\` is granted with no stated reason. Every write scope must name ` +
          "the step that needs it, in a comment beside it.",
      );
    }
  }

  // --- Untrusted input ------------------------------------------------------------------
  for (const node of nodes) {
    if (node.path.length >= 2 && node.path[0] === "on" && node.key === "pull_request_target") {
      add(
        RULES.NO_PULL_REQUEST_TARGET,
        node.line,
        "`pull_request_target` runs with a read-write token and repository secrets in the base " +
          "context. Combined with a checkout of the PR head it is the standard secret-" +
          "exfiltration path (ADR 0015). Use `pull_request`, and split trusted work into a " +
          "`workflow_run` job that never executes PR code.",
      );
    }
    if (node.path.length === 1 && node.key === "on" && node.value.includes("pull_request_target")) {
      add(RULES.NO_PULL_REQUEST_TARGET, node.line, "`pull_request_target` is not permitted (see ADR 0015).");
    }
  }

  for (const node of nodes) {
    if (node.key !== "run") continue;
    // `run: |` bodies are opaque to the scanner, so check the raw block instead.
    const body = node.value === "|" || node.value === ">" ? blockBody(lines, node) : node.value;
    if (UNTRUSTED_EVENT.test(body)) {
      add(
        RULES.NO_UNTRUSTED_INTERPOLATION,
        node.line,
        "`${{ github.event... }}` is interpolated straight into a shell script. Event text is " +
          "attacker-controlled (a PR title is enough), and interpolation happens before the " +
          "shell sees it, so this is script injection. Pass it through `env:` and quote the " +
          "variable instead.",
      );
    }
  }

  return findings;
}

/** The raw text of a `key: |` block scalar, for rules that must see inside one. */
function blockBody(lines, node) {
  const out = [];
  for (let i = node.line; i < lines.length; i++) {
    const raw = lines[i];
    if (raw.trim() === "") continue;
    const indent = raw.length - raw.trimStart().length;
    if (indent <= node.indent) break;
    out.push(raw);
  }
  return out.join("\n");
}
