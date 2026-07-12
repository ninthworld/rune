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
