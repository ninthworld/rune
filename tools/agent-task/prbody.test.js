import assert from "node:assert/strict";
import { test } from "node:test";

import { acceptanceCriteria, buildPrBody } from "./prbody.js";
import { anIssue } from "./testing/fake-github.js";

const RUN = { run_id: "186-x", provider: "claude", isolation: "container", issue: 186 };
const GATES = [
  { gate: "Engine", ok: true, duration_ms: 42_000 },
  { gate: "Client", ok: true, duration_ms: 60_000 },
];

const ISSUE = anIssue({
  body: "### Acceptance criteria\n\n- [ ] Claim the issue atomically.\n- [ ] Never merge a PR.\n",
});

const build = (opts = {}) =>
  buildPrBody({ issue: ISSUE, run: RUN, gates: GATES, files: ["a.rs"], ciPaths: [], ...opts });

test("acceptance criteria are read from the issue's checkboxes", () => {
  assert.deepEqual(acceptanceCriteria(ISSUE.body), ["Claim the issue atomically.", "Never merge a PR."]);
  assert.deepEqual(acceptanceCriteria("no criteria here"), []);
});

test("the body links the issue and names the run", () => {
  const body = build();
  assert.match(body, /Closes #186\./);
  assert.match(body, /run `186-x`.*provider `claude`.*isolation `container`/);
  assert.match(body, /cannot approve or merge/);
});

test("criteria the provider did not map are called out, not quietly dropped", () => {
  const body = build({
    providerUsage: { criteria: [{ criterion: "Claim the issue atomically.", evidence: "claim.js + tests" }] },
  });

  assert.match(body, /1 of 2 criteria have no reported evidence/);
  assert.match(body, /claim\.js \+ tests/);
  assert.match(body, /Never merge a PR.*unmapped — no evidence reported/);
});

// The bug behind #221: the join was byte equality, and a provider copying a criterion into JSON
// drops the markdown. On #191 every criterion carrying an inline code span — 6 of 27 — reported
// as unmapped while its evidence sat in the result file, matching nothing.
test("a criterion is matched even when the provider reformats it copying it back", () => {
  const issue = anIssue({
    body: "- [ ] `GameView` round-trips through serde_json\n- [ ] `make check` green\n",
  });
  const body = build({
    issue,
    providerUsage: {
      criteria: [
        { criterion: "GameView round-trips through serde_json", evidence: "view.rs + a round-trip test" },
        { criterion: "**make check**   GREEN.", evidence: "ran it; Engine and Client pass" },
      ],
    },
  });

  assert.match(body, /All 2 criteria have reported evidence/);
  assert.match(body, /`GameView` round-trips through serde_json \| view\.rs \+ a round-trip test/);
  assert.match(body, /`make check` green \| ran it; Engine and Client pass/);
  assert.doesNotMatch(body, /unmapped/);
});

test("the criterion column keeps the issue's own wording, not the provider's copy of it", () => {
  const body = build({
    issue: anIssue({ body: "- [ ] `make check` green\n" }),
    providerUsage: { criteria: [{ criterion: "make check green", evidence: "ran it" }] },
  });
  assert.match(body, /\| `make check` green \|/);
});

test("a reported claim matching no criterion is surfaced, not dropped", () => {
  const body = build({
    providerUsage: {
      criteria: [
        { criterion: "Claim the issue atomically.", evidence: "claim.js" },
        { criterion: "Never merge a PR.", evidence: "the runner cannot approve" },
        { criterion: "Rewrite the scheduler.", evidence: "scheduler.js, rewritten" },
      ],
    },
  });

  assert.match(body, /1 reported claim matches no criterion in this issue/);
  assert.match(body, /> - _Rewrite the scheduler\._ — scheduler\.js, rewritten/);
  assert.match(body, /All 2 criteria have reported evidence/);
});

test("one claim satisfies at most one criterion", () => {
  const body = build({
    providerUsage: {
      criteria: [
        { criterion: "Claim the issue atomically.", evidence: "first claim" },
        { criterion: "claim the issue atomically", evidence: "same criterion, said twice" },
      ],
    },
  });

  // The duplicate must not be spent on the *other* criterion, which nobody reported evidence for.
  assert.match(body, /1 of 2 criteria have no reported evidence/);
  assert.match(body, /Never merge a PR.*unmapped — no evidence reported/);
  assert.match(body, /1 reported claim matches no criterion/);
  assert.match(body, /same criterion, said twice/);
});

test("an entry reported with no evidence leaves its criterion unmapped", () => {
  const body = build({
    providerUsage: { criteria: [{ criterion: "Claim the issue atomically.", evidence: "" }] },
  });
  assert.match(body, /2 of 2 criteria have no reported evidence/);
  assert.doesNotMatch(body, /matches no criterion/);
});

test("the criteria mapping is labelled as the provider's claim, not the runner's finding", () => {
  const body = build({
    providerUsage: {
      criteria: [
        { criterion: "Claim the issue atomically.", evidence: "a" },
        { criterion: "Never merge a PR.", evidence: "b" },
      ],
    },
  });
  assert.match(body, /Evidence \(provider-reported\)/);
  assert.match(body, /the provider's claim, not the runner's finding/);
});

test("gate results are reported as runner-observed", () => {
  const body = build();
  assert.match(body, /## Verification \(runner-observed\)/);
  assert.match(body, /\| `Engine` \| ✅ pass \| 42s \|/);
});

test("a failed gate shows as failed", () => {
  const body = build({ gates: [{ gate: "E2E", ok: false, duration_ms: 1000 }] });
  assert.match(body, /`E2E` \| ❌ fail/);
});

test("CI-governance changes are shouted at the top of the body", () => {
  const body = build({ ciPaths: [".github/workflows/ci.yml"] });

  const banner = body.indexOf("CI-governance changes");
  assert.ok(banner > 0 && banner < body.indexOf("## Acceptance criteria"), "the warning must come first");
  assert.match(body, /weaken the checks/);
  assert.match(body, /> - `\.github\/workflows\/ci\.yml`/);
});

test("a PR with no CI changes carries no CI warning", () => {
  assert.doesNotMatch(build(), /CI-governance/);
});

test("provider usage is included but marked non-comparable", () => {
  const body = build({ providerUsage: { tokens: 1234, model: "some-model" } });
  assert.match(body, /advisory, not comparable across providers/);
  assert.match(body, /"tokens": 1234/);
});
