/**
 * The security regression test (#243).
 *
 * Everything else in this directory tests the *code*. This tests the **workflows**, because the
 * security property of ADR 0015 does not live in the code — it lives in the fact that the job
 * holding the credential never executes pull-request-controlled content. That property is one
 * careless edit away at all times: a `checkout` with the wrong `ref:`, an `npm ci` added to make
 * a linter happy, a `permissions:` widened to fix an unrelated 403. None of those would fail a
 * unit test, and all of them would silently reopen the hole the two-workflow split exists to
 * close.
 *
 * So the workflows themselves are the fixture, and these assertions are the guard rail. Reuses
 * the #199 scanner rather than parsing YAML a second time.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { CLAUDE_CLI } from "./config.js";
import { scan, splitComment } from "../ci-policy/scan.js";

const read = (name) => readFileSync(new URL(`../../.github/workflows/${name}`, import.meta.url), "utf8");

/**
 * The workflow with its comments removed.
 *
 * These assertions ask "does this workflow *do* X", and a comment explaining why it deliberately
 * does not do X is not doing X. Grepping the raw file conflates the two — and it did, on the
 * first run of this suite: the comment in ai-review.yml explaining why `pull_request_target` is
 * unusable failed the test asserting `pull_request_target` is unused. Scan what runs, not what
 * is written about what runs.
 */
const effective = (text) =>
  text
    .split("\n")
    .map((line) => splitComment(line)[0])
    .join("\n");

const PREPARE = read("ai-review-prepare.yml");
const REVIEW = read("ai-review.yml");
const PREPARE_RUNS = effective(PREPARE);
const REVIEW_RUNS = effective(REVIEW);

const nodes = (text) => scan(text);
const keysNamed = (text, key) => nodes(text).filter((n) => n.key === key);
/** Nodes belonging to one job — the review workflow now has two (the reviewer, and the
 *  prepare-failure reporter that guarantees a required check can never hang pending). */
const inJob = (text, job, key) => nodes(text).filter((n) => n.path[0] === "jobs" && n.path[1] === job && n.key === key);
const permissionsOf = (text) =>
  nodes(text)
    .filter((n) => n.path.includes("permissions") && n.path[n.path.length - 2] === "permissions")
    .map((n) => [n.key, n.value]);

// --- STAGE 1: untrusted, and therefore harmless -------------------------------------------------

test("prepare holds no secrets — it sees hostile files, so it must have nothing worth stealing", () => {
  assert.equal(/secrets\./.test(PREPARE_RUNS), false, "the untrusted stage must never reference a secret");
});

test("prepare has no write permission of any kind", () => {
  const perms = permissionsOf(PREPARE);
  assert.deepEqual(perms, [["contents", "read"]]);
  assert.equal(perms.some(([, v]) => v === "write"), false);
});

test("prepare never runs anything that came out of the pull request", () => {
  // The forbidden verbs: each would execute head-controlled code (a Makefile, a build.rs, a
  // package.json `prepare` script, a binary).
  for (const forbidden of ["make ", "cargo ", "npm ci", "npm install", "npm run", "npx ", "./scripts/", "bash scripts/"]) {
    assert.equal(
      PREPARE_RUNS.includes(forbidden),
      false,
      `prepare must never execute PR content (found ${JSON.stringify(forbidden)})`,
    );
  }
  // What it *may* do: git (hardened) and node running our own base-ref script.
  assert.ok(PREPARE_RUNS.includes("node tools/ai-review/prepare.js"));
});

test("prepare checks out the BASE ref, so a PR cannot rewrite the code that prepares it", () => {
  assert.match(PREPARE, /ref:\s*\$\{\{\s*github\.event\.pull_request\.base\.sha\s*\}\}/);
  assert.match(PREPARE, /persist-credentials:\s*false/);
});

test("prepare's git commands disable hooks", () => {
  for (const r of keysNamed(PREPARE, "run").map((n) => n.value)) {
    if (r.includes("git ")) assert.match(PREPARE, /core\.hooksPath=\/dev\/null/);
  }
});

// --- STAGE 2: trusted, and therefore must not touch the pull request ----------------------------

test("review never checks out, fetches, or references the PR head", () => {
  // The single most important assertion in this file. If this ever fails, the credential and the
  // attacker's code are in the same job again, and the design is void.
  for (const forbidden of [
    "pull_request.head",
    "refs/pull/",
    "pull/${",
    "workflow_run.head_branch",
    "workflow_run.head_repository",
  ]) {
    assert.equal(REVIEW_RUNS.includes(forbidden), false, `the trusted stage must not reference PR head (${forbidden})`);
  }
  // Its checkout takes no `ref:` at all — for a workflow_run event that is the default branch.
  const checkout = nodes(REVIEW).find((n) => n.key === "uses" && n.value.startsWith("actions/checkout@"));
  assert.ok(checkout, "the trusted stage checks out something");
  const refKeys = nodes(REVIEW).filter((n) => n.key === "ref");
  assert.deepEqual(refKeys, [], "and it is not a ref anyone can choose");
});

test("review never builds, installs from, or executes the workspace or the artifact", () => {
  // `npm install --global <pinned package>` IS allowed (the reviewer CLI has to come from
  // somewhere) — installing a *published, pinned* package is not executing PR content. What is
  // forbidden is anything that installs from or builds the checkout: those run lifecycle scripts,
  // and that is the class of thing this stage must never do while holding a credential.
  for (const forbidden of ["make ", "cargo ", "npm ci", "npm run", "npx ", "eval ", "source ", "npm install ."]) {
    assert.equal(REVIEW_RUNS.includes(forbidden), false, `the trusted stage must not execute the tree (found ${forbidden})`);
  }

  const runs = inJob(REVIEW, "review", "run").map((n) => n.value);
  const [install, review, guarantee] = runs;
  assert.equal(runs.length, 3, "install the pinned reviewer, run ours, and guarantee a check on failure");

  // The install must be global and version-pinned. An unpinned install would let the harness that
  // enforces `--disallowed-tools` change under us without a reviewable diff.
  assert.match(install, /^npm install --global .* "@anthropic-ai\/claude-code@\d+\.\d+\.\d+"$/);
  assert.ok(
    install.includes(`@anthropic-ai/claude-code@${CLAUDE_CLI.version}`),
    `the workflow installs a different CLI version than config.js pins (${CLAUDE_CLI.version})`,
  );

  assert.equal(review, "node tools/ai-review/cli.js ai-review-input", "and the only thing that reads the artifact is ours");

  // The failure fallback talks to the GitHub API and nothing else. It must never read the
  // artifact: it runs precisely when we do not know whether the artifact was ever trustworthy.
  assert.equal(guarantee, "|", "the fallback is a block scalar");
  const block = REVIEW_RUNS.slice(REVIEW_RUNS.indexOf("existing=$(gh api"));
  assert.equal(block.includes("ai-review-input"), false, "the fallback must not touch the artifact");
  assert.equal(/\bnode\b/.test(block), false, "and it must not run our reviewer either");
});

test("the prepare-failure reporter exists, and can only write a check", () => {
  // Without it, a failed *prepare* run leaves no `AI Review` check at all — and a REQUIRED check
  // that never reports leaves the PR permanently pending, which is worse than red. It gets
  // `checks: write` and nothing else: it has no artifact to read and no review to post.
  const perms = nodes(REVIEW)
    .filter((n) => n.path[1] === "report-prepare-failure" && n.path[n.path.length - 2] === "permissions")
    .map((n) => [n.key, n.value]);
  assert.deepEqual(perms, [["checks", "write"]]);

  const runs = inJob(REVIEW, "report-prepare-failure", "run");
  assert.equal(runs.length, 1);
  assert.match(REVIEW, /workflow_run\.conclusion != 'success'/, "it fires exactly when the reviewer's job cannot");
});

test("review's write permissions are exactly the two it needs to publish, and no more", () => {
  const perms = new Map(permissionsOf(REVIEW).filter(([k]) => ["contents", "actions", "pull-requests", "checks", "id-token"].includes(k)));
  const writes = [...perms].filter(([, v]) => v === "write").map(([k]) => k);

  assert.deepEqual(writes.sort(), ["checks", "pull-requests"]);
  assert.equal(perms.get("contents"), "read", "read — it may never push a commit");
  assert.equal(perms.has("id-token"), false);
  assert.equal(perms.has("actions") && perms.get("actions") === "write", false);
});

test("review only accepts runs of the prepare workflow, which succeeded, in this repository", () => {
  assert.match(REVIEW, /workflows:\s*\["AI Review Prepare"\]/);
  assert.match(REVIEW, /workflow_run\.conclusion == 'success'/);
  assert.match(REVIEW, /workflow_run\.event == 'pull_request'/);
  assert.match(REVIEW, /workflow_run\.repository\.full_name == github\.repository/);
});

test("the model credential exists only in the trusted stage", () => {
  assert.match(REVIEW_RUNS, /CLAUDE_CODE_OAUTH_TOKEN:\s*\$\{\{\s*secrets\.CLAUDE_CODE_OAUTH_TOKEN\s*\}\}/);
  assert.equal(PREPARE_RUNS.includes("API_KEY"), false);
  assert.equal(PREPARE_RUNS.includes("OAUTH_TOKEN"), false);
});

// --- neither stage may ever be the thing ADR 0015 exists to forbid -------------------------------

test("neither workflow uses pull_request_target", () => {
  // `make ci-lint` (#199) enforces this repository-wide; asserted here too, because for *these*
  // two workflows it is not a policy preference, it is the entire threat model.
  assert.equal(PREPARE_RUNS.includes("pull_request_target"), false);
  assert.equal(REVIEW_RUNS.includes("pull_request_target"), false);
});

test("neither workflow interpolates attacker-controlled event data into a shell", () => {
  for (const [name, text] of [
    ["prepare", PREPARE],
    ["review", REVIEW],
  ]) {
    for (const node of keysNamed(text, "run")) {
      const body = node.value === "|" ? text.split("\n").slice(node.line, node.line + 6).join("\n") : node.value;
      assert.equal(
        /\$\{\{\s*github\.event/.test(body),
        false,
        `${name}: event data must reach the shell through env:, never through interpolation`,
      );
    }
  }
  // Both do pass event data — through `env:`, where the shell sees it as a quoted variable.
  assert.match(PREPARE, /PR_NUMBER:\s*\$\{\{\s*github\.event\.pull_request\.number\s*\}\}/);
});

test("the interim reviewer, which held a credential AND checked out the PR, is gone", () => {
  assert.throws(() => read("claude-code-review.yml"), /ENOENT/, "claude-code-review.yml must not come back");
});
