import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { TaskError, claim, release } from "./claim.js";
import { GitHub } from "./github.js";
import { branchName } from "./preflight.js";
import { activeRunForIssue, listRuns } from "./runs.js";
import { anIssue, fakeGitHub } from "./testing/fake-github.js";

const BRANCH = branchName(anIssue());
const REF = `heads/${BRANCH}`;

let root;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "rune-runs-"));
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

function connect(state) {
  return new GitHub({ owner: "ninthworld", repo: "rune", token: "t", fetchImpl: state.fetch });
}

const start = (state, overrides = {}) =>
  claim(connect(state), { issue: 186, provider: "claude", actor: "dev@box", root, ...overrides });

test("claim creates the branch, moves the labels, and records the run", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue() } });

  const run = await start(state);

  assert.equal(state.refs[REF], "base000");
  assert.deepEqual(
    state.issues[186].labels.map((l) => l.name),
    ["agent-task", "area:ci", "status:in-progress"],
  );
  assert.equal(state.comments.length, 1);
  assert.match(state.comments[0].body, /Claimed by `rune-agent\[bot\]`/);
  assert.match(state.comments[0].body, new RegExp(run.run_id));

  assert.equal(run.state, "claimed");
  assert.equal(run.branch, BRANCH);
  assert.equal(run.base_sha, "base000");
  assert.equal(activeRunForIssue(186, root).run_id, run.run_id);
});

test("the branch is created before any issue mutation", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue() } });
  await start(state);

  const branchCreate = state.calls.indexOf("POST /git/refs");
  const firstMutation = state.calls.findIndex((c) => /^(POST|DELETE) \/issues/.test(c));
  assert.ok(branchCreate >= 0 && branchCreate < firstMutation, state.calls.join("\n"));
});

test("losing the claim race mutates nothing and records claim_lost", async () => {
  const state = fakeGitHub({
    issues: { 186: anIssue() },
    refs: { "heads/main": "base000", [REF]: "other" },
  });

  await assert.rejects(start(state), (err) => err instanceof TaskError && /claimed by another runner/.test(err.message));

  assert.equal(state.comments.length, 0);
  assert.deepEqual(
    state.issues[186].labels.map((l) => l.name),
    ["agent-task", "status:ready", "area:ci"],
  );
  assert.equal(state.refs[REF], "other");
  assert.equal(listRuns(root)[0].state, "claim_lost");
});

test("a preflight rejection makes no GitHub mutation at all", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue({ labels: [{ name: "status:blocked" }] }) } });

  await assert.rejects(start(state), TaskError);

  assert.deepEqual(state.calls.filter((c) => !c.startsWith("GET ")), []);
  assert.equal(listRuns(root).length, 0);
});

test("an issue blocked by an open dependency is not claimable", async () => {
  const issue = anIssue({ body: "Blocked by:\n\n- #185 — the ADR.\n" });
  const state = fakeGitHub({ issues: { 186: issue, 185: anIssue({ number: 185, state: "open" }) } });

  await assert.rejects(start(state), /blocked by open #185/);
  assert.equal(state.refs[REF], undefined);
});

test("a closed dependency does not block the claim", async () => {
  const issue = anIssue({ body: "Blocked by:\n\n- #185 — the ADR.\n\n- [ ] build it\n" });
  const state = fakeGitHub({ issues: { 186: issue, 185: anIssue({ number: 185, state: "closed" }) } });

  const run = await start(state);
  assert.equal(run.state, "claimed");
});

test("a second claim on the same machine is refused", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue() } });
  await start(state);

  await assert.rejects(start(state), /already has an active run/);
});

test("release deletes the branch and returns the issue to status:ready", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue() } });
  const run = await start(state);

  await release(connect(state), { issue: 186, root });

  assert.equal(state.refs[REF], undefined);
  assert.deepEqual(
    state.issues[186].labels.map((l) => l.name),
    ["agent-task", "area:ci", "status:ready"],
  );
  assert.equal(activeRunForIssue(186, root), null);
  assert.equal(listRuns(root).find((r) => r.run_id === run.run_id).state, "released");
});

test("release refuses to discard a branch that has commits, unless forced", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue() }, aheadBy: { [BRANCH]: 3 } });
  await start(state);

  await assert.rejects(release(connect(state), { issue: 186, root }), /3 commit\(s\) that would be destroyed/);
  assert.ok(state.refs[REF]);

  await release(connect(state), { issue: 186, root, force: true });
  assert.equal(state.refs[REF], undefined);
});

test("release refuses a claim this machine does not own, unless forced", async () => {
  const state = fakeGitHub({
    issues: { 186: anIssue({ labels: [{ name: "status:in-progress" }] }) },
    refs: { "heads/main": "base000", [REF]: "elsewhere" },
  });

  await assert.rejects(release(connect(state), { issue: 186, root }), /no active run on this machine/);

  await release(connect(state), { issue: 186, root, force: true });
  assert.equal(state.refs[REF], undefined);
  assert.deepEqual(
    state.issues[186].labels.map((l) => l.name),
    ["status:ready"],
  );
  assert.match(state.comments.at(-1).body, /forced takeover/);
});

test("release recovers a partial claim whose branch is already gone", async () => {
  const state = fakeGitHub({ issues: { 186: anIssue({ labels: [{ name: "status:in-progress" }] }) } });

  await release(connect(state), { issue: 186, root, force: true });

  assert.deepEqual(
    state.issues[186].labels.map((l) => l.name),
    ["status:ready"],
  );
});
