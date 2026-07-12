import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { crCitations, findMilestone, issueRefs, lastReconciled, parseRoadmap, whereWeAre } from "./cycle-roadmap.js";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const ROADMAP = readFileSync(join(REPO, "docs", "roadmap.md"), "utf8");

const FIXTURE = `# Roadmap

> Last reconciled against GitHub issues + \`main\`: 2026-07-11 (second pass).

## Where we are (2026-07-11)

- **Engine**: combat works end to end (#117/#118). Still true: there are no
  keywords.
- **Docs**: ADRs 0001–0014 accepted.

## Milestones

### M9 — Fixture milestone

**Outcome:** something happens.

**Exit criteria:**

- [x] Turn-based actions are real (CR 502/504/514), with tests. (#116)
- [ ] Combat works: declare attackers, blockers, damage (CR 508–510, 704.5g).
      *Partial: engine shipped (#117); the client half is #143.*
- [ ] ADR 0013 implemented: \`data/oracle.json\` plus a \`GameView.result\` field.

**Features (PR-sized, dependency order):**

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| Combat | engine | #117 ✅ | — |
| Client stack panel | client | #142 | #140 |

### M10 — Prose milestone *(coarse)*

**Outcome:** a newcomer can follow a game.

**Exit:** the comprehension items of \`design/ui-requirements.md\` §9 pass a
scripted usability run; timers enforce.

### M11 — Nothing to audit *(coarse)*

Commander night: command zone, commander-damage matrix, the full automation suite.

## How this drives work

- Issues are the queue.
`;

test("every criterion's text is carried verbatim out of the real roadmap", () => {
  const roadmap = parseRoadmap(ROADMAP);
  assert.ok(roadmap.milestones.length >= 7, "expected M1–M7");

  for (const milestone of roadmap.milestones) {
    for (const criterion of milestone.exit_criteria) {
      // The whole point of `raw`: the Auditor argues about the sentence a human wrote, not a
      // paraphrase this parser invented.
      assert.ok(
        ROADMAP.includes(criterion.raw),
        `${criterion.criterion_id} is not byte-identical to docs/roadmap.md:\n${criterion.raw}`,
      );
    }
  }
});

test("the real roadmap's milestones each have something auditable, except the coarse tail", () => {
  const roadmap = parseRoadmap(ROADMAP);
  const empty = roadmap.milestones.filter((m) => m.exit_criteria.length === 0).map((m) => m.id);

  // M6/M7 are prose-only by design. A milestone with no exit condition is not "trivially
  // done" — it is not auditable at all, and `validateBundle` refuses to build a bundle for
  // one rather than letting an Auditor infer an exit condition of its own.
  assert.deepEqual(empty, ["M6", "M7"]);
});

test("checkbox state, partial notes, and issue refs come off a criterion", () => {
  const [milestone] = parseRoadmap(FIXTURE).milestones;
  const [first, second, third] = milestone.exit_criteria;

  assert.equal(first.checked, true);
  assert.equal(second.checked, false);
  assert.deepEqual(first.issue_refs, [116]);

  assert.deepEqual(second.notes, ["Partial: engine shipped (#117); the client half is #143."]);
  assert.ok(!second.text.includes("Partial:"), "the note is carried separately, not inlined into the claim");
  assert.ok(second.raw.includes("*Partial:"), "the verbatim text keeps it");

  assert.deepEqual(third.adr_refs, ["0013"]);
  assert.deepEqual(third.terms, ["data/oracle.json", "GameView.result"]);
});

test("a milestone's issue refs span its criteria and its feature table", () => {
  const [milestone] = parseRoadmap(FIXTURE).milestones;
  // #142/#140 appear only in the table, #116/#117/#143 only in the criteria. GitHub's
  // milestone field is set on neither, which is exactly why the roadmap's own links matter.
  assert.deepEqual(milestone.issue_refs, [116, 117, 140, 142, 143]);
});

test("criterion ids are content-addressed: stable across re-collection, new when the claim changes", () => {
  const once = parseRoadmap(FIXTURE).milestones[0].exit_criteria;
  const twice = parseRoadmap(FIXTURE).milestones[0].exit_criteria;
  assert.deepEqual(
    once.map((c) => c.criterion_id),
    twice.map((c) => c.criterion_id),
  );

  const reworded = parseRoadmap(FIXTURE.replace("Combat works:", "Combat mostly works:")).milestones[0].exit_criteria;
  assert.notEqual(reworded[1].criterion_id, once[1].criterion_id);
  assert.equal(reworded[0].criterion_id, once[0].criterion_id, "an untouched criterion keeps its id");
});

test("CR citations expand ranges, split lists, and reduce to sections", () => {
  assert.deepEqual(crCitations("(CR 502/504/514)").sections, [502, 504, 514]);
  assert.deepEqual(crCitations("(CR 508–510, 704.5g)").sections, [508, 509, 510, 704]);
  assert.deepEqual(crCitations("CR 117.1a, 304.1, 307.1").sections, [117, 304, 307]);
  assert.deepEqual(crCitations("CR 704.5g").tokens, ["704.5g"]);
  assert.deepEqual(crCitations("no rules here").sections, []);
});

test("a prose milestone yields one prose criterion, not an empty checklist", () => {
  const milestone = findMilestone(parseRoadmap(FIXTURE), "M10");
  assert.equal(milestone.exit_criteria.length, 1);

  const [criterion] = milestone.exit_criteria;
  assert.equal(criterion.kind, "prose");
  assert.equal(criterion.checked, false);
  assert.match(criterion.text, /^the comprehension items of/);
  assert.deepEqual(criterion.terms, ["design/ui-requirements.md"]);
});

test("a milestone with no exit condition at all yields no criteria", () => {
  assert.deepEqual(findMilestone(parseRoadmap(FIXTURE), "M11").exit_criteria, []);
});

test("milestones are found by id or by name, and a typo finds nothing", () => {
  const roadmap = parseRoadmap(FIXTURE);
  assert.equal(findMilestone(roadmap, "M9").id, "M9");
  assert.equal(findMilestone(roadmap, "m9").id, "M9");
  assert.equal(findMilestone(roadmap, "M9 — Fixture milestone").id, "M9");
  assert.equal(findMilestone(roadmap, "M99"), null);
});

test("the known-gap prose and the reconciliation date are read straight out of the document", () => {
  assert.equal(lastReconciled(FIXTURE), "2026-07-11");

  const bullets = whereWeAre(FIXTURE);
  assert.equal(bullets.length, 2);
  assert.match(bullets[0], /^\*\*Engine\*\*: combat works end to end \(#117\/#118\)\./);
  assert.match(bullets[0], /Still true: there are no keywords\.$/);

  assert.deepEqual(issueRefs("closes #12 and #3, again #12"), [3, 12]);
});
