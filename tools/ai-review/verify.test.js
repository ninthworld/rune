import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { CAPS, CHECK_NAME, REVIEWER_VERSION, SCHEMA_VERSION } from "./config.js";
import { ArtifactRejected, alreadyReviewed, verifyArtifact, verifyWorkflowRun } from "./verify.js";

const REPO = "ninthworld/rune";
const HEAD = "h".repeat(40);
const BASE = "b".repeat(40);

const event = (over = {}) => ({
  id: 777,
  name: "AI Review Prepare",
  event: "pull_request",
  conclusion: "success",
  head_sha: HEAD,
  repository: { full_name: REPO },
  pull_requests: [{ number: 42 }],
  ...over,
});

const pr = (over = {}) => ({ number: 42, base: { sha: BASE }, head: { sha: HEAD }, state: "open", ...over });

function artifact(over = {}) {
  const input = { schema_version: SCHEMA_VERSION, head_sha: HEAD, patches: [], context: [], changed_paths: [] };
  const body = JSON.stringify(input);
  const manifest = {
    schema_version: SCHEMA_VERSION,
    reviewer_version: REVIEWER_VERSION,
    repository: REPO,
    pr_number: 42,
    base_sha: BASE,
    head_sha: HEAD,
    prepare_run_id: "777",
    input_sha256: createHash("sha256").update(body).digest("hex"),
    input_bytes: body.length,
    file_count: 0,
    changed_path_count: 0,
    context_paths: [],
    truncated: false,
    truncation: [],
    ...over,
  };
  return { manifest, body };
}

const run = { runId: "777", headSha: HEAD, prNumber: 42 };

// --- the event: is this even ours? ------------------------------------------------------------

test("a well-formed prepare run is accepted", () => {
  assert.deepEqual(verifyWorkflowRun(event(), REPO), { runId: "777", headSha: HEAD, prNumber: 42 });
});

test("a run of some other workflow cannot hand us an artifact", () => {
  assert.throws(() => verifyWorkflowRun(event({ name: "CI" }), REPO), /not "AI Review Prepare"/);
});

test("a run triggered by something other than a pull request is refused", () => {
  assert.throws(() => verifyWorkflowRun(event({ event: "push" }), REPO), /not pull_request/);
});

test("a prepare run that did not succeed produced nothing worth reading", () => {
  assert.throws(() => verifyWorkflowRun(event({ conclusion: "failure" }), REPO), /concluded "failure"/);
  assert.throws(() => verifyWorkflowRun(event({ conclusion: null }), REPO), /concluded null/);
});

test("a run belonging to another repository is refused", () => {
  assert.throws(() => verifyWorkflowRun(event({ repository: { full_name: "attacker/rune" } }), REPO), /not "ninthworld\/rune"/);
});

test("a fork PR has no run association, so the PR is resolved from the head SHA instead", () => {
  const parsed = verifyWorkflowRun(event({ pull_requests: [] }), REPO);
  assert.equal(parsed.prNumber, null);
  assert.equal(parsed.headSha, HEAD);
});

// --- the artifact: is it the one our prepare run made, unaltered? -----------------------------

test("a matching artifact is accepted and its input returned", () => {
  const { manifest, body } = artifact();
  const input = verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() });
  assert.equal(input.head_sha, HEAD);
});

test("a tampered artifact fails the hash check", () => {
  const { manifest, body } = artifact();
  const tampered = body.replace('"patches":[]', '"patches":[{"path":"x","patch":"evil"}]');
  assert.throws(
    () => verifyArtifact({ manifest, body: tampered, run, repository: REPO, pr: pr() }),
    (err) => err instanceof ArtifactRejected && /bytes, manifest claims|sha256/.test(err.message),
  );
});

test("a manifest whose hash was recomputed to match a tampered body still fails on size", () => {
  // The realistic attack: rewrite the body AND the manifest's hash. It survives the hash check
  // and dies on provenance — the manifest still has to name our run, our repo, and this head.
  const body = JSON.stringify({ schema_version: SCHEMA_VERSION, head_sha: HEAD, evil: true });
  const { manifest } = artifact({
    input_sha256: createHash("sha256").update(body).digest("hex"),
    input_bytes: body.length,
    prepare_run_id: "999",
  });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() }), /names prepare run 999/);
});

test("an oversized artifact is refused before it is parsed", () => {
  const body = "x".repeat(CAPS.artifactBytes + 1);
  const { manifest } = artifact({ input_bytes: body.length, input_sha256: createHash("sha256").update(body).digest("hex") });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() }), /over the .* cap/);
});

test("an artifact prepared by a different reviewer version is refused", () => {
  const { manifest, body } = artifact({ reviewer_version: "0.9.0" });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() }), /prepared by reviewer 0\.9\.0/);
});

test("an artifact naming a different PR than the head belongs to is refused", () => {
  const { manifest, body } = artifact({ pr_number: 43 });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() }), /names PR #43/);
});

test("a stale head is refused: the push that moved it already started a new run", () => {
  const { manifest, body } = artifact();
  const moved = pr({ head: { sha: "c".repeat(40) } });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: moved }), /stale head/);
});

test("a base branch that moved while we were queued does NOT fail the review", () => {
  // `pr.base.sha` tracks the tip of `main`, so it moves whenever anything merges. Rejecting on
  // it would publish a failing *required* check on an innocent PR every time someone else merged
  // — and a re-run could not clear it, because a re-run replays the original event payload. The
  // head SHA and the artifact hash are what pin the review; the base branch moving is a fact
  // about the repository, not evidence of tampering.
  const { manifest, body } = artifact();
  const movedBase = pr({ base: { sha: "d".repeat(40) } });
  const input = verifyArtifact({ manifest, body, run, repository: REPO, pr: movedBase });
  assert.equal(input.head_sha, HEAD, "the review proceeds");
});

test("an input that disagrees with its own manifest is refused", () => {
  const input = { schema_version: SCHEMA_VERSION, head_sha: "z".repeat(40) };
  const body = JSON.stringify(input);
  const { manifest } = artifact({ input_sha256: createHash("sha256").update(body).digest("hex"), input_bytes: body.length });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() }), /disagrees with its own manifest/);
});

test("a body that is not JSON is refused", () => {
  const body = "not json";
  const { manifest } = artifact({ input_sha256: createHash("sha256").update(body).digest("hex"), input_bytes: body.length });
  assert.throws(() => verifyArtifact({ manifest, body, run, repository: REPO, pr: pr() }), /not JSON/);
});

// --- idempotency --------------------------------------------------------------------------------

test("a head already reviewed by this reviewer is not reviewed again", () => {
  const done = [{ name: CHECK_NAME, status: "completed", conclusion: "success", output: { summary: `reviewer ${REVIEWER_VERSION} ok` } }];
  assert.equal(alreadyReviewed(done), true);
  // A *failed* prior check is not a review: rerunning after an outage must actually review.
  assert.equal(alreadyReviewed([{ ...done[0], conclusion: "failure" }]), false);
  // A different reviewer version is a different reviewer, and re-reviewing is correct.
  assert.equal(alreadyReviewed(done, { reviewerVersion: "2.0.0" }), false);
  assert.equal(alreadyReviewed([]), false);
  assert.equal(alreadyReviewed(undefined), false);
});
