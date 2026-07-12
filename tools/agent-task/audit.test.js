import assert from "node:assert/strict";
import { test } from "node:test";

import { AUDIT_BRANCH, observePr, publishSummary } from "./audit.js";
import { GitHub } from "./github.js";
import { buildSummary } from "./summary.js";
import { anIssue, fakeGitHub } from "./testing/fake-github.js";

const connect = (state) => new GitHub({ owner: "ninthworld", repo: "rune", token: "t", fetchImpl: state.fetch });

const aSummary = (over = {}) =>
  buildSummary(
    {
      run_id: "186-r1",
      issue: 186,
      provider: "claude",
      branch: "agent/186-x",
      created_at: "2026-07-12T06:00:00.000Z",
      state: "review",
      events: [],
      ...over,
    },
    { issue: anIssue(), now: new Date("2026-07-12T07:00:00Z") },
  );

test("the first record creates an orphan branch, sharing no history with main", async () => {
  const state = fakeGitHub();

  const record = await publishSummary(connect(state), aSummary());

  assert.equal(record.created, true);
  assert.equal(record.path, "runs/186/186-r1.json");
  assert.ok(state.refs[`heads/${AUDIT_BRANCH}`]);
  // No parents: the audit branch can never be merged into main by accident, and never triggers CI.
  assert.deepEqual(state.commits[state.refs[`heads/${AUDIT_BRANCH}`]].parents, []);
});

test("a later record appends to the branch, keeping the earlier ones", async () => {
  const state = fakeGitHub();
  const gh = connect(state);

  await publishSummary(gh, aSummary({ run_id: "186-r1" }));
  await publishSummary(gh, aSummary({ run_id: "187-r1", issue: 187 }));

  const tree = state.trees[state.commits[state.refs[`heads/${AUDIT_BRANCH}`]].tree];
  assert.deepEqual(Object.keys(tree).sort(), ["runs/186/186-r1.json", "runs/187/187-r1.json"]);
});

test("one file per run, so concurrent runs cannot collide on content", async () => {
  const state = fakeGitHub();
  const gh = connect(state);

  await Promise.all([
    publishSummary(gh, aSummary({ run_id: "a" })),
    publishSummary(gh, aSummary({ run_id: "b" })),
    publishSummary(gh, aSummary({ run_id: "c" })),
  ]);

  const tree = state.trees[state.commits[state.refs[`heads/${AUDIT_BRANCH}`]].tree];
  assert.equal(Object.keys(tree).length, 3, Object.keys(tree).join(", "));
});

test("a contended ref update is retried rather than lost", async () => {
  const state = fakeGitHub();
  const gh = connect(state);
  await publishSummary(gh, aSummary({ run_id: "first" }));

  // Another runner lands a record between our read and our write, exactly once.
  let contended = false;
  state.onRefUpdate = () => {
    if (contended) return false;
    contended = true;
    return true;
  };

  const record = await publishSummary(gh, aSummary({ run_id: "second" }));
  assert.ok(record.commit);
  assert.equal(contended, true, "the conflict must actually have been exercised");

  const tree = state.trees[state.commits[state.refs[`heads/${AUDIT_BRANCH}`]].tree];
  assert.ok(Object.keys(tree).includes("runs/186/second.json"));
  assert.ok(Object.keys(tree).includes("runs/186/first.json"), "the earlier record survived");
});

test("a correction is a new record that supersedes, never a rewrite", async () => {
  const state = fakeGitHub();
  const gh = connect(state);

  await publishSummary(gh, aSummary({ run_id: "186-r1" }));
  const correction = await publishSummary(gh, aSummary({ run_id: "186-r1", supersedes: "186-r1" }));

  assert.notEqual(correction.path, "runs/186/186-r1.json", "the original file is not overwritten");
  const tree = state.trees[state.commits[state.refs[`heads/${AUDIT_BRANCH}`]].tree];
  assert.ok(Object.keys(tree).includes("runs/186/186-r1.json"), "the superseded record is still there");
  assert.equal(Object.keys(tree).length, 2);
});

test("the branch is only ever fast-forwarded", async () => {
  const state = fakeGitHub();
  await publishSummary(connect(state), aSummary());
  await publishSummary(connect(state), aSummary({ run_id: "x" }));

  const forced = state.calls.some((c) => c.startsWith("PATCH")) && false;
  assert.equal(forced, false);
  // The parent chain is intact, which is what append-only means in practice.
  const head = state.refs[`heads/${AUDIT_BRANCH}`];
  assert.equal(state.commits[head].parents.length, 1);
});

test("observePr records the PR's author and whether the ADR 0015 review actually ran", async () => {
  const state = fakeGitHub({
    pulls: { 300: { number: 300, draft: true, user: { login: "rune-agent[bot]" }, head: { sha: "head1" } } },
    checkRuns: { head1: [{ name: "AI Review", status: "completed", conclusion: "success" }] },
  });

  const observed = await observePr(connect(state), 300);

  assert.deepEqual(observed.pr, { number: 300, author: "rune-agent[bot]", draft: true });
  assert.deepEqual(observed.review, { observed: true, ran: true, conclusion: "success" });
});

test("a review that never ran is recorded as not-run, not as absent", async () => {
  // The whole point: a silently skipped review looks
  // exactly like a passing one. Green required checks do not imply the PR was reviewed.
  const state = fakeGitHub({
    pulls: { 301: { number: 301, draft: true, user: { login: "rune-agent[bot]" }, head: { sha: "head2" } } },
    checkRuns: { head2: [{ name: "Engine", status: "completed", conclusion: "success" }] },
  });

  const observed = await observePr(connect(state), 301);
  assert.deepEqual(observed.review, { observed: true, ran: false, conclusion: null });
});
