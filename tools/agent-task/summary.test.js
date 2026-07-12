import assert from "node:assert/strict";
import { test } from "node:test";

import { SUMMARY_SCHEMA_VERSION, area, buildSummary, failureStage, sizeBucket } from "./summary.js";
import { anIssue } from "./testing/fake-github.js";

const RUN = {
  schema_version: 1,
  run_id: "186-r1",
  issue: 186,
  provider: "claude",
  isolation: "container",
  gate_set: "verify",
  branch: "agent/186-x",
  base_sha: "base000",
  head_sha: "head111",
  created_at: "2026-07-12T06:00:00.000Z",
  state: "review",
  events: [{ state: "claimed", at: "2026-07-12T06:00:01.000Z" }],
  gates: [{ gate: "Engine", ok: true, duration_ms: 1000 }],
  files: ["a.rs", "b.rs", "c.rs"],
  ci_paths: [],
};

const build = (run = {}, opts = {}) => buildSummary({ ...RUN, ...run }, { issue: anIssue(), ...opts });

test("every terminal outcome maps to a normalized failure stage", () => {
  assert.equal(failureStage("review"), null);
  assert.equal(failureStage("released"), null);
  assert.equal(failureStage("provider_timeout"), "provider");
  assert.equal(failureStage("ci_change_refused"), "diff_inspection");
  assert.equal(failureStage("verification_failed"), "verify");
  assert.equal(failureStage("rebase_conflict"), "rebase");
  assert.equal(failureStage("claim_lost"), "claim");
});

test("an unknown outcome is 'unknown', never a silent success", () => {
  // A new outcome added without updating the map must not report as a clean run.
  assert.equal(failureStage("something_new"), "unknown");
});

test("diff size is bucketed, and the contents never appear", () => {
  assert.equal(sizeBucket(1), "xs");
  assert.equal(sizeBucket(5), "s");
  assert.equal(sizeBucket(15), "m");
  assert.equal(sizeBucket(100), "l");
  assert.equal(build().issue_size, "s");
});

test("issue area comes from labels", () => {
  assert.deepEqual(area([{ name: "area:ci" }, { name: "agent-task" }, { name: "area:engine" }]), ["ci", "engine"]);
});

test("the summary is versioned and records the lifecycle", () => {
  const summary = build();
  assert.equal(summary.schema_version, SUMMARY_SCHEMA_VERSION);
  assert.equal(summary.terminal_outcome, "review");
  assert.equal(summary.failure_stage, null);
  assert.deepEqual(summary.lifecycle, [{ state: "claimed", at: "2026-07-12T06:00:01.000Z" }]);
});

test("runner-observed and provider-reported are kept apart", () => {
  const summary = build({ provider_usage: { tokens: 900, model: "some-model" } });

  assert.deepEqual(summary.gates, [{ gate: "Engine", ok: true, duration_ms: 1000 }]);
  assert.deepEqual(summary.provider_usage, { model: "some-model", tokens: 900 });
});

test("provider-reported usage is narrowed to an allowlist — a provider can write anything", () => {
  const summary = build({
    provider_usage: {
      tokens: 10,
      // A provider that dumped the diff, its prompt, or a token into result.json must not get
      // any of it into telemetry.
      diff: "--- a/src/lib.rs\n+++ b/src/lib.rs",
      prompt: "the entire brief",
      token: "ghs_abcdefghijklmnopqrstuvwxyz0123456789",
    },
  });

  assert.deepEqual(Object.keys(summary.provider_usage), ["tokens"]);
  assert.doesNotMatch(JSON.stringify(summary), /ghs_|--- a\/src|entire brief/);
});

test("a credential inside an allowlisted string field is still redacted", () => {
  const summary = build({ provider_usage: { model: "model ghs_abcdefghijklmnopqrstuvwxyz0123456789" } });
  assert.doesNotMatch(summary.provider_usage.model, /ghs_/);
});

test("the summary carries no payload — no diff, brief, logs, or environment", () => {
  const summary = build({
    workspace: "/home/me/.local/state/rune/runs/186/repo",
    violations: [{ detail: "secret leaked here" }],
  });
  const serialized = JSON.stringify(summary);

  for (const forbidden of ["brief", "diff", "logs", "env", "workspace", "violations"]) {
    assert.equal(forbidden in summary, false, `${forbidden} must not be in a run summary`);
  }
  assert.doesNotMatch(serialized, /secret leaked here/);
});

test("the review is recorded as unobserved until someone actually looks", () => {
  // claude-review is not a required check, so a skipped review is indistinguishable from a
  // passing one. "Not observed" and "did not run" must never collapse into each other.
  assert.deepEqual(build().review, { observed: false, ran: null, conclusion: null });

  const observed = build({}, { review: { observed: true, ran: false, conclusion: null } });
  assert.equal(observed.review.ran, false);
});

test("a resumed run records what it resumed, so a third-attempt success is not a first-attempt one", () => {
  const summary = build({ run_id: "186-r2", resume_of: "186-r1" });
  assert.equal(summary.resume_of, "186-r1");
});
