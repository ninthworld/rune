import assert from "node:assert/strict";
import { test } from "node:test";

import { branchName, declaredDependencies, hasAcceptanceCriteria, preflight } from "./preflight.js";
import { anIssue } from "./testing/fake-github.js";

test("branchName produces the canonical agent/<issue>-<slug>, capped in length", () => {
  assert.equal(
    branchName(anIssue({ number: 186, title: "tooling: implement provider-neutral issue runner" })),
    "agent/186-tooling-implement-provider-neutral-issue",
  );
});

test("branchName never emits doubled or trailing dashes", () => {
  const branch = branchName(anIssue({ number: 7, title: "docs: ADR 0016 — the runner!! " }));
  assert.equal(branch, "agent/7-docs-adr-0016-the-runner");
  assert.doesNotMatch(branch, /--|-$/);
});

test("declaredDependencies reads the Blocked by list", () => {
  const body = "### Dependencies\n\nBlocked by:\n\n- #185 — the ADR.\n- #183 — the ruleset.\n\n### Goal\n\nCloses #999.";
  assert.deepEqual(declaredDependencies(body), [185, 183]);
});

test("declaredDependencies ignores issue references outside the Blocked by list", () => {
  assert.deepEqual(declaredDependencies("Follows #12 and relates to #13."), []);
  assert.deepEqual(declaredDependencies(""), []);
});

test("hasAcceptanceCriteria detects checkbox lists", () => {
  assert.equal(hasAcceptanceCriteria("- [ ] do the thing"), true);
  assert.equal(hasAcceptanceCriteria("* [x] done"), true);
  assert.equal(hasAcceptanceCriteria("just some prose"), false);
});

test("a ready, unblocked issue is claimable", () => {
  const check = preflight(anIssue());
  assert.equal(check.ok, true);
  assert.deepEqual(check.errors, []);
});

test("preflight rejects closed, blocked, in-progress, and unready issues", () => {
  const cases = [
    [anIssue({ state: "closed" }), /closed/],
    [anIssue({ labels: [{ name: "status:ready" }, { name: "status:blocked" }] }), /status:blocked/],
    [anIssue({ labels: [{ name: "status:in-progress" }] }), /already claimed/],
    [anIssue({ labels: [{ name: "status:review" }] }), /already in review/],
    [anIssue({ labels: [{ name: "agent-task" }] }), /not labelled status:ready/],
    [anIssue({ pull_request: { url: "…" } }), /pull request/],
  ];
  for (const [issue, expected] of cases) {
    const check = preflight(issue);
    assert.equal(check.ok, false);
    assert.match(check.errors.join("\n"), expected);
  }
});

test("preflight rejects an issue with open dependencies", () => {
  const check = preflight(anIssue(), [185, 183]);
  assert.equal(check.ok, false);
  assert.match(check.errors.join("\n"), /blocked by open #185, #183/);
});

test("preflight rejects an empty body but only warns about missing criteria", () => {
  assert.match(preflight(anIssue({ body: "   " })).errors.join(), /malformed/);

  const noCriteria = preflight(anIssue({ body: "Please make the thing work." }));
  assert.equal(noCriteria.ok, true);
  assert.match(noCriteria.warnings.join(), /acceptance-criteria/);
});
