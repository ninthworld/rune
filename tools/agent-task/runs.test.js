import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { activeRunForIssue, isActive, listRuns, loadRun, newRunId, removeRun, runDir, saveRun, transition } from "./runs.js";

let root;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "rune-runs-"));
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

const aRun = (overrides = {}) => ({
  schema_version: 1,
  run_id: newRunId(186),
  issue: 186,
  title: "t",
  provider: "claude",
  branch: "agent/186-t",
  state: "claimed",
  events: [],
  created_at: new Date().toISOString(),
  ...overrides,
});

test("run ids embed the issue and a UTC timestamp, and do not collide", () => {
  const at = new Date("2026-07-12T05:47:37Z");
  assert.match(newRunId(186, at), /^186-20260712T054737Z-[0-9a-f]{6}$/);
  assert.notEqual(newRunId(186, at), newRunId(186, at));
});

test("a saved run round-trips and stamps updated_at", () => {
  const saved = saveRun(aRun(), root);
  assert.ok(saved.updated_at);
  assert.deepEqual(loadRun(saved.run_id, root), saved);
});

test("loadRun returns null for an unknown run rather than throwing", () => {
  assert.equal(loadRun("nope", root), null);
  assert.deepEqual(listRuns(join(root, "missing")), []);
});

test("transition appends an event and preserves history", () => {
  const run = transition(transition(saveRun(aRun({ state: "claiming" }), root), "claimed", root), "released", root);
  assert.equal(run.state, "released");
  assert.deepEqual(
    run.events.map((e) => e.state),
    ["claimed", "released"],
  );
});

test("only claiming and claimed runs are active", () => {
  assert.equal(isActive(aRun({ state: "claimed" })), true);
  assert.equal(isActive(aRun({ state: "claiming" })), true);
  for (const state of ["released", "claim_lost"]) assert.equal(isActive(aRun({ state })), false);
});

test("activeRunForIssue ignores finished runs on the same issue", () => {
  saveRun(aRun({ state: "released" }), root);
  const live = saveRun(aRun({ state: "claimed" }), root);
  saveRun(aRun({ issue: 999, state: "claimed" }), root);

  assert.equal(activeRunForIssue(186, root).run_id, live.run_id);
  assert.equal(activeRunForIssue(1234, root), null);
});

test("listRuns skips directories that are not runs", () => {
  const run = saveRun(aRun(), root);
  writeFileSync(join(root, "stray-file"), "junk");
  assert.deepEqual(
    listRuns(root).map((r) => r.run_id),
    [run.run_id],
  );
});

test("removeRun deletes the run directory", () => {
  const run = saveRun(aRun(), root);
  assert.ok(existsSync(runDir(run.run_id, root)));
  removeRun(run.run_id, root);
  assert.equal(existsSync(runDir(run.run_id, root)), false);
});
