import assert from "node:assert/strict";
import test from "node:test";

import { CAPS } from "./config.js";
import { InvalidFindings, countBySeverity, findingId, neutralize, normalizeFindings } from "./schema.js";

const HEAD = "a".repeat(40);
const ok = (over = {}) => ({
  severity: "high",
  category: "defect",
  path: "src/a.rs",
  line: 12,
  title: "off-by-one in damage",
  risk: "lethal damage is applied one point early",
  recommendation: "use >= instead of >",
  ...over,
});
const norm = (findings, changedPaths = ["src/a.rs"]) =>
  normalizeFindings({ findings }, { headSha: HEAD, changedPaths });

test("a well-formed finding survives with a stable, content-derived id", () => {
  const { findings, dropped } = norm([ok()]);
  assert.equal(dropped.length, 0);
  assert.equal(findings.length, 1);
  assert.match(findings[0].id, /^f_[0-9a-f]{12}$/);
  // Stable across runs: the same defect on the same head gets the same id, which is what lets
  // a human disposition (#244) stay attached to it.
  assert.equal(findings[0].id, norm([ok()]).findings[0].id);
  // ...and it is not a positional counter: reordering does not renumber.
  const two = norm([ok({ title: "second" }), ok()]);
  assert.equal(two.findings.find((f) => f.title === "off-by-one in damage").id, findings[0].id);
});

test("a payload with no findings array is an infrastructure failure, not an empty review", () => {
  assert.throws(() => normalizeFindings({}, { headSha: HEAD }), InvalidFindings);
  assert.throws(() => normalizeFindings(null, { headSha: HEAD }), InvalidFindings);
  assert.throws(() => normalizeFindings({ findings: "none" }, { headSha: HEAD }), InvalidFindings);
  // An *empty* list, though, is a perfectly good review.
  assert.deepEqual(normalizeFindings({ findings: [] }, { headSha: HEAD }).findings, []);
});

test("an invented severity or category is dropped, and the drop is counted", () => {
  const { findings, dropped } = norm([ok({ severity: "catastrophic" }), ok({ category: "style" }), ok()]);
  assert.equal(findings.length, 1);
  assert.equal(dropped.length, 2);
  assert.match(dropped[0].reason, /severity/);
  assert.match(dropped[1].reason, /category/);
});

test("a finding with no risk or no recommendation is not a finding", () => {
  const { findings, dropped } = norm([ok({ risk: "" }), ok({ recommendation: null }), ok({ title: "" })]);
  assert.equal(findings.length, 0);
  assert.equal(dropped.length, 3);
});

test("a finding about a file the diff never touched keeps the claim but loses the location", () => {
  const { findings } = norm([ok({ path: "src/elsewhere.rs" })]);
  assert.equal(findings[0].path, null);
  assert.equal(findings[0].line, null);
  // The claim is preserved so a human can see the model wandered — it is not silently dropped.
  assert.equal(findings[0].off_diff_path, "src/elsewhere.rs");
});

test("findings are ordered by severity and capped", () => {
  const many = [ok({ severity: "low" }), ok({ severity: "critical", title: "x" }), ok({ severity: "medium", title: "y" })];
  assert.deepEqual(
    norm(many).findings.map((f) => f.severity),
    ["critical", "medium", "low"],
  );

  const flood = Array.from({ length: CAPS.findings + 10 }, (_, i) => ok({ title: `finding ${i}` }));
  const { findings, findings_truncated } = norm(flood);
  assert.equal(findings.length, CAPS.findings);
  assert.equal(findings_truncated, true);
});

test("countBySeverity reports every severity, including the zeroes", () => {
  const counts = countBySeverity([{ severity: "high" }, { severity: "high" }]);
  assert.equal(counts.high, 2);
  assert.equal(counts.critical, 0);
  assert.equal(counts.low, 0);
});

// --- the model read an attacker-written diff, so its output is attacker-influenced ------------

test("neutralize defuses markup that would let a finding lie to a human reader", () => {
  // An HTML comment would hide text from a reader of the rendered comment.
  assert.equal(neutralize("<!-- hidden -->"), "&lt;!-- hidden --&gt;");
  // A pipe would break out of the table cell it lands in.
  assert.ok(!neutralize("a | b").includes(" | "));
  // A fence would break out of the block.
  assert.ok(!neutralize("```js").includes("`"));
  // A newline would break the layout of the finding block.
  assert.ok(!neutralize("a\nb").includes("\n"));
});

test("an @mention in a finding renders but cannot notify anyone", () => {
  const out = neutralize("ping @ninthworld");
  assert.ok(out.includes("@"), "the text still reads as an @mention to a human");
  assert.ok(!out.includes("@ninthworld"), "but the literal handle is broken by a zero-width space");
});

test("a title stays bounded no matter how long the model makes it", () => {
  const { findings } = norm([ok({ title: "x".repeat(5_000) })]);
  assert.ok(findings[0].title.length <= 200);
});

test("findingId separates findings that differ only by location", () => {
  const a = findingId({ headSha: HEAD, category: "defect", path: "a.rs", line: 1, title: "t" });
  const b = findingId({ headSha: HEAD, category: "defect", path: "a.rs", line: 2, title: "t" });
  assert.notEqual(a, b);
});
