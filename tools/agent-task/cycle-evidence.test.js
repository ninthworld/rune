import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable } from "node:stream";
import { afterEach, beforeEach, test } from "node:test";

import {
  EVIDENCE_SCHEMA_VERSION,
  closesLinkage,
  collectAdrProtocolState,
  collectEvidence,
  collectRulesCoverage,
  collectStubs,
  parseTestCounts,
  validateBundle,
} from "./cycle-evidence.js";
import { GitHub } from "./github.js";
import { fakeGitHub } from "./testing/fake-github.js";
import { git } from "./git.js";

const ROADMAP = `# Roadmap

> Last reconciled against GitHub issues + \`main\`: 2026-07-11.

## Where we are (2026-07-11)

- **Engine**: combat works; there are no keywords yet.

## Milestones

### M9 — Fixture milestone

**Outcome:** something happens.

**Exit criteria:**

- [x] Combat works: attackers, blockers, damage (CR 508–510). (#117)
- [ ] ADR 0013 implemented: \`data/oracle.json\` and a \`GameView.result\` field.
      *Partial: the loader shipped (#146); the set files are #160.*

**Features (PR-sized, dependency order):**

| Feature | Area | Issue | Depends on |
|---|---|---|---|
| Combat | engine | #117 ✅ | — |
| Starter set | engine | #160 | #146 |

## How this drives work

- Issues are the queue.
`;

const COVERAGE = `# Rules coverage

| CR rule | Summary | Status | Code anchor | Test anchor |
| --- | --- | --- | --- | --- |
| CR 508.1 / 508.1a | Declare attackers. | partial — single attack target. | \`combat.rs :: attacker_candidates\` | \`apply.rs :: issue_117_declare\` |
| CR 704.5g | Lethal damage destroys. | implemented | \`sba.rs :: lethal\` | \`sba.rs :: cr_704_5g\` |
| CR 103.5 | The London mulligan. | implemented | \`mulligan.rs :: keep\` | \`mulligan.rs :: cr_103_5\` |
`;

const PROTOCOL = `# RUNE protocol

## Server → client: GameView

### Game over: result

\`GameView.result\` is null while the game is live.
`;

const ADR_0013 = `# ADR 0013: Card identity vs printing

- Status: accepted
- Date: 2026-07-01
`;

/** A workspace pinned at the audited commit — a real git repo, because `git grep` needs one. */
function fixtureWorkspace() {
  const dir = mkdtempSync(join(tmpdir(), "rune-ws-"));
  mkdirSync(join(dir, "docs", "decisions"), { recursive: true });
  mkdirSync(join(dir, "crates", "rune-engine", "src"), { recursive: true });

  writeFileSync(join(dir, "docs", "roadmap.md"), ROADMAP);
  writeFileSync(join(dir, "docs", "rules-coverage.md"), COVERAGE);
  writeFileSync(join(dir, "docs", "protocol.md"), PROTOCOL);
  writeFileSync(join(dir, "docs", "decisions", "0013-card-identity.md"), ADR_0013);
  writeFileSync(
    join(dir, "crates", "rune-engine", "src", "combat.rs"),
    "fn blockers() {\n    // TODO: multi-block ordering\n    unimplemented!()\n}\n",
  );

  git(["init", "--quiet", "-b", "main"], { cwd: dir });
  git(["add", "-A"], { cwd: dir });
  return dir;
}

/**
 * Gate output in each harness's own dialect, so the count parser is exercised for real.
 *
 * The vitest line carries the SGR colour codes vitest actually emits into a pipe — copied
 * from a real `make check` run, because a hand-written plain-text version of it is what let
 * the parser silently read zero vitest suites out of a perfectly good run.
 */
const GATE_OUTPUT = {
  Engine: "running tests\ntest result: ok. 214 passed; 0 failed; 1 ignored; 0 measured\ntest result: ok. 31 passed; 2 failed; 0 ignored\n",
  Client:
    "\u001B[2m Test Files \u001B[22m \u001B[1m\u001B[32m14 passed\u001B[39m\u001B[22m\u001B[90m (14)\u001B[39m\n" +
    "\u001B[2m      Tests \u001B[22m \u001B[31m1 failed\u001B[39m | \u001B[32m40 passed\u001B[39m (41)\n" +
    "# pass 122\n# fail 0\n",
  E2E: "  6 passed (12.4s)\n",
  "cargo-deny": "advisories ok\n",
};

function fakeSpawn(codes = {}, outputs = GATE_OUTPUT) {
  const calls = [];
  return {
    calls,
    impl: (cmd, args) => {
      calls.push([cmd, ...args]);
      const gate = { "engine-lint": "Engine", "client-audit": "Client", e2e: "E2E", deny: "cargo-deny" }[args[0]];
      const child = new EventEmitter();
      child.stdout = Readable.from([outputs[gate] ?? ""]);
      child.stderr = Readable.from([]);
      setImmediate(() => child.emit("close", codes[gate] ?? 0));
      return child;
    },
  };
}

const REPO = {
  milestones: [{ number: 4, title: "M9 — Fixture milestone" }],
  issues: {
    117: {
      number: 117,
      title: "Combat II",
      state: "closed",
      closed_at: "2026-06-01T00:00:00Z",
      labels: [{ name: "area:engine" }],
      milestone: { number: 4, title: "M9 — Fixture milestone" },
    },
    // Named only by the roadmap's feature table — GitHub does not tag it.
    160: { number: 160, title: "Starter set", state: "open", labels: [{ name: "status:blocked" }], milestone: null },
    // Tagged on GitHub but absent from the roadmap: the drift runs both ways.
    161: {
      number: 161,
      title: "Late addition",
      state: "open",
      labels: [],
      milestone: { number: 4, title: "M9 — Fixture milestone" },
    },
    146: { number: 146, title: "Oracle split", state: "closed", labels: [], milestone: null },
  },
  timelines: {
    117: [
      { event: "cross-referenced", source: { issue: { number: 300, pull_request: {} } } },
      // Merged, and it mentions the issue — but it did not close it.
      { event: "cross-referenced", source: { issue: { number: 301, pull_request: {} } } },
      // Cross-referenced by another *issue*, not a PR.
      { event: "cross-referenced", source: { issue: { number: 999 } } },
    ],
  },
  pulls: {
    300: {
      number: 300,
      title: "feat(engine): combat damage",
      state: "closed",
      body: "Closes #117.",
      merged_at: "2026-06-01T00:00:00Z",
      merge_commit_sha: "merge300",
    },
    301: {
      number: 301,
      title: "docs: combat notes",
      state: "closed",
      body: "Refs #117.",
      merged_at: "2026-06-02T00:00:00Z",
      merge_commit_sha: "merge301",
    },
  },
  checkRuns: {
    merge300: [
      { name: "Engine", status: "completed", conclusion: "success" },
      { name: "E2E", status: "completed", conclusion: "success" },
      { name: "claude-review", status: "completed", conclusion: "neutral" },
    ],
  },
};

let workspace;
let root;
let api;

beforeEach(() => {
  workspace = fixtureWorkspace();
  root = mkdtempSync(join(tmpdir(), "rune-cycles-"));
  api = fakeGitHub(REPO);
});
afterEach(() => {
  rmSync(workspace, { recursive: true, force: true });
  rmSync(root, { recursive: true, force: true });
});

const collect = (opts = {}) => {
  const { impl, calls } = fakeSpawn(opts.codes, opts.outputs);
  return collectEvidence({
    gh: new GitHub({ owner: "ninthworld", repo: "rune", token: "t", fetchImpl: api.fetch }),
    workspace,
    milestone: "M9",
    cycleId: "c1",
    baseSha: "base9999",
    root,
    spawnImpl: impl,
    now: new Date("2026-07-12T00:00:00.000Z"),
    ...opts,
  }).then((bundle) => ({ bundle, calls }));
};

test("collection produces a schema-valid bundle pinned to the audited commit", async () => {
  const { bundle } = await collect();

  assert.equal(bundle.schema_version, EVIDENCE_SCHEMA_VERSION);
  assert.equal(bundle.base_commit_sha, "base9999");
  assert.equal(bundle.milestone, "M9 — Fixture milestone");
  assert.deepEqual(validateBundle(bundle), { ok: true, problems: [] });
});

test("collection mutates nothing: every GitHub call is a read", async () => {
  await collect();

  const writes = api.calls.filter((call) => !call.startsWith("GET "));
  assert.deepEqual(writes, [], "the Evidence Collector must not touch a label, comment, branch, or PR");
  assert.deepEqual(api.comments, []);
});

test("the milestone's issues are the union of GitHub's tag and the roadmap's own links", async () => {
  const { bundle } = await collect();
  const byNumber = Object.fromEntries(bundle.issues.map((i) => [i.number, i]));

  assert.deepEqual(Object.keys(byNumber).map(Number), [117, 146, 160, 161]);
  // Recording *where* each issue came from is what makes the drift between the two sources
  // legible instead of silently resolved in favor of one of them.
  assert.deepEqual(byNumber[117].sources, ["github-milestone", "roadmap"]);
  assert.deepEqual(byNumber[160].sources, ["roadmap"], "the roadmap tracks it; GitHub does not tag it");
  assert.deepEqual(byNumber[161].sources, ["github-milestone"], "GitHub tags it; the roadmap has not caught up");
  assert.equal(byNumber[117].state, "closed");
});

test("a closed issue is recorded as a closed issue — never as a satisfied criterion", async () => {
  const { bundle } = await collect();

  const combat = bundle.exit_criteria.find((c) => c.text.startsWith("Combat works"));
  const oracle = bundle.exit_criteria.find((c) => c.text.startsWith("ADR 0013"));

  // #117 is closed and #146 is closed, but nothing in the bundle promotes either into
  // evidence that the criterion is met: `checked` is the roadmap's own claim, and no other
  // field on a criterion carries a status at all. Concluding is the Auditor's job (#225),
  // and it has to do it from the citations below, not from a closed issue.
  assert.equal(combat.checked, true, "the roadmap ticked it — a claim, not a verdict");
  assert.equal(oracle.checked, false);
  for (const criterion of bundle.exit_criteria) {
    assert.ok(!("status" in criterion) && !("met" in criterion) && !("verdict" in criterion));
  }
});

test("merged PRs carry their merge SHA and Closes linkage, and their required checks", async () => {
  const { bundle } = await collect();

  assert.deepEqual(bundle.prs, [
    {
      number: 300,
      title: "feat(engine): combat damage",
      state: "closed",
      merged: true,
      merged_at: "2026-06-01T00:00:00Z",
      merge_commit_sha: "merge300",
      // GitHub's link and the PR's own claim, kept apart: "mentioned the issue" and "closed
      // the issue" are different facts, and #999 — cross-referenced but not a PR — is neither.
      closes: [117],
      referenced_by: [117],
    },
    {
      number: 301,
      title: "docs: combat notes",
      state: "closed",
      merged: true,
      merged_at: "2026-06-02T00:00:00Z",
      merge_commit_sha: "merge301",
      // Kept, because over-inclusion is safe — but with an empty `closes`, so nothing
      // downstream can credit the criterion to a PR that only mentioned the issue.
      closes: [],
      referenced_by: [117],
    },
  ]);

  // Narrowed to the checks `main` actually requires: `claude-review` is not one of them, and
  // an advisory check reported alongside the required four must not read as one. A PR with no
  // required-check runs at all reports an empty list rather than vanishing from the evidence.
  assert.deepEqual(bundle.ci.merged_pr_checks, [
    {
      pr: 300,
      sha: "merge300",
      checks: [
        { name: "Engine", status: "completed", conclusion: "success" },
        { name: "E2E", status: "completed", conclusion: "success" },
      ],
    },
    { pr: 301, sha: "merge301", checks: [] },
  ]);
});

test("the fresh gate run is recorded separately from the merged PRs' green checks", async () => {
  // Every merged PR was green (above) and `main` is red today. Conflating the two is exactly
  // how a milestone gets closed on history rather than on fact.
  const { bundle, calls } = await collect({ codes: { E2E: 1 } });

  assert.deepEqual(calls, [
    ["make", "engine-lint", "engine-test"],
    ["make", "client-audit", "client-check", "runner-test"],
    ["make", "e2e"],
    ["make", "deny"],
  ]);

  const fresh = bundle.ci.fresh_run;
  assert.equal(fresh.ok, false);
  assert.deepEqual(
    fresh.gates.map((g) => [g.gate, g.ok]),
    [
      ["Engine", true],
      ["Client", true],
      ["E2E", false],
      ["cargo-deny", true],
    ],
  );
  assert.equal(bundle.ci.merged_pr_checks[0].checks[1].conclusion, "success");
});

test("a red gate does not stop collection", async () => {
  // The runner fails fast on a red gate because the work goes back to the provider either
  // way. An audit cannot: "we stopped looking after the first failure" is how a missing
  // piece of evidence turns into a wrong verdict.
  const { bundle } = await collect({ codes: { Engine: 1 } });
  assert.equal(bundle.ci.fresh_run.gates.length, 4);
});

test("test counts are read from every suite a gate runs, colour codes and all", async () => {
  const { bundle } = await collect();

  assert.deepEqual(bundle.tests.suites, [
    { suite: "cargo", passed: 245, failed: 2, gate: "Engine" },
    { suite: "vitest", passed: 40, failed: 1, gate: "Client" },
    { suite: "node:test", passed: 122, failed: 0, gate: "Client" },
    { suite: "playwright", passed: 6, failed: 0, gate: "E2E" },
  ]);
  assert.deepEqual(bundle.tests.missing_counts, []);
});

test("a suite whose counts could not be read is named, not silently read as zero failures", async () => {
  // The `Client` gate runs two suites. When the vitest counts went missing, the gate still
  // reported `node:test` and so *looked* fully parsed — a whole suite vanished from the
  // evidence with nothing to show for it. Expecting a named set per gate is what catches that.
  const { bundle } = await collect({ outputs: { ...GATE_OUTPUT, Client: "# pass 122\n# fail 0\n" } });

  assert.deepEqual(
    bundle.tests.suites.map((s) => s.suite),
    ["cargo", "node:test", "playwright"],
  );
  assert.deepEqual(bundle.tests.missing_counts, [{ gate: "Client", suite: "vitest" }]);
});

test("rules-coverage rows are scoped to the CR sections the criteria name", () => {
  // CR 508–510 is cited; CR 103.5 is not. The `partial —` status and its named gap survive,
  // because that gap is evidence against a `met`.
  const rows = collectRulesCoverage(COVERAGE, [508, 509, 510]);
  assert.deepEqual(
    rows.map((r) => r.cr),
    ["CR 508.1 / 508.1a"],
  );
  assert.match(rows[0].status, /^partial — single attack target/);
  assert.deepEqual(collectRulesCoverage(COVERAGE, []), []);
});

test("ADR status and protocol coverage are read, not inferred", () => {
  const criteria = [{ adr_refs: ["0013", "0099"], terms: [] }];
  const state = collectAdrProtocolState(workspace, criteria, ["GameView.result", "valid_actions", "data/oracle.json"]);

  assert.deepEqual(state.adrs, [
    { adr: "0013", found: true, status: "accepted", path: "docs/decisions/0013-card-identity.md" },
    { adr: "0099", found: false, status: null, path: null },
  ]);
  assert.deepEqual(state.protocol.terms, [
    { term: "GameView.result", documented: true },
    { term: "valid_actions", documented: false },
  ]);
  assert.ok(!state.protocol.terms.some((t) => t.term === "data/oracle.json"), "a file path is not a protocol shape");
  assert.deepEqual(state.protocol.sections, ["Server → client: GameView", "Game over: result"]);
});

test("stubs are located, never quoted", () => {
  const stubs = collectStubs(workspace, ["crates/rune-engine/src"]);

  assert.deepEqual(stubs.roots, ["crates/rune-engine/src"]);
  assert.deepEqual(stubs.matches, [
    { file: "crates/rune-engine/src/combat.rs", line: 2, marker: "TODO" },
    { file: "crates/rune-engine/src/combat.rs", line: 3, marker: "unimplemented!(" },
  ]);
  // The matched line itself is never recorded: source text (and whatever a source file
  // happens to contain) must not ride into a bundle that later gets summarized.
  for (const match of stubs.matches) assert.deepEqual(Object.keys(match), ["file", "line", "marker"]);
});

test("known gaps are carried forward verbatim rather than re-derived", async () => {
  const { bundle } = await collect();

  assert.equal(bundle.documented_gaps.partial_notes.length, 1);
  assert.deepEqual(bundle.documented_gaps.partial_notes[0].notes, [
    "Partial: the loader shipped (#146); the set files are #160.",
  ]);
  assert.match(bundle.documented_gaps.where_we_are[0], /combat works; there are no keywords yet/);
});

test("a milestone that is not in the roadmap fails loudly", async () => {
  await assert.rejects(() => collect({ milestone: "M99" }), /no milestone "M99" in docs\/roadmap\.md \(known: M9\)/);
});

test("the parser reads each harness's own summary dialect", () => {
  assert.deepEqual(parseTestCounts("test result: ok. 3 passed; 1 failed; 0 ignored"), [
    { suite: "cargo", passed: 3, failed: 1 },
  ]);
  assert.deepEqual(parseTestCounts("  Tests  12 passed (12)"), [{ suite: "vitest", passed: 12, failed: 0 }]);
  assert.deepEqual(parseTestCounts("# pass 7\n# fail 2\n"), [{ suite: "node:test", passed: 7, failed: 2 }]);
  assert.deepEqual(parseTestCounts("  2 failed\n  4 passed (3.1s)"), [{ suite: "playwright", passed: 4, failed: 2 }]);
  assert.deepEqual(parseTestCounts("nothing recognizable"), []);
});

test("Closes linkage is read off a PR body in every spelling GitHub honors", () => {
  assert.deepEqual(closesLinkage("Closes #1, fixes #2\nResolved #3. Mentions #4 in passing."), [1, 2, 3]);
  assert.deepEqual(closesLinkage(null), []);
});

test("validation reports every structural problem at once, and never a verdict", () => {
  const { ok, problems } = validateBundle({
    schema_version: 99,
    cycle_id: "c1",
    exit_criteria: [
      { criterion_id: "M9-aaa", raw: "- [ ] one", checked: false },
      { criterion_id: "M9-aaa", checked: false },
    ],
    ci: { fresh_run: { gates: [] } },
    prompt: "you are an auditor…",
  });

  assert.equal(ok, false);
  const at = (path) => problems.find((p) => p.path === path)?.problem;

  assert.match(at("schema_version"), /expected 1, got 99/);
  assert.equal(at("milestone"), "missing");
  assert.equal(at("base_commit_sha"), "missing");
  assert.equal(at("issues"), "missing");
  assert.match(at("exit_criteria[1].criterion_id"), /duplicate M9-aaa/);
  assert.match(at("exit_criteria[1].raw"), /missing verbatim text/);
  // Without a fresh run, the bundle can only prove that old PRs were green.
  assert.match(at("ci.fresh_run.gates"), /CI history proves those PRs passed, not that main passes now/);
  // ADR 0016's "never a payload" rule, enforced rather than trusted.
  assert.match(at("prompt"), /forbidden: a bundle carries evidence, never a payload/);
});

test("a bundle with no criteria cannot be audited, and validation says so", async () => {
  const { bundle } = await collect();
  assert.equal(validateBundle({ ...bundle, exit_criteria: [] }).ok, false);
  assert.match(
    validateBundle({ ...bundle, exit_criteria: [] }).problems.find((p) => p.path === "exit_criteria").problem,
    /a milestone with no criteria cannot be audited/,
  );
});
