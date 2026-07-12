import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { cycleDir, cycleWorkspace, listCycles, loadBundle, milestoneSlug, newCycleId, saveBundle } from "./cycle-state.js";
import { git } from "./git.js";

let root;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "rune-cycles-"));
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

const aBundle = (overrides = {}) => ({
  schema_version: 1,
  cycle_id: newCycleId("M3 — A real card pool"),
  milestone: "M3 — A real card pool",
  base_commit_sha: "abc1234",
  collected_at: new Date().toISOString(),
  ...overrides,
});

test("a cycle id embeds the milestone and a UTC stamp, and does not collide", () => {
  const at = new Date("2026-07-12T10:20:30.400Z");
  const id = newCycleId("M3 — A real card pool", at);

  assert.match(id, /^m3-a-real-card-pool-20260712T102030Z-[0-9a-f]{6}$/);
  assert.notEqual(newCycleId("M3", at), newCycleId("M3", at));
  assert.equal(milestoneSlug("M3 — A real card pool"), "m3-a-real-card-pool");
});

test("bundles are written under the state root, never into the repository", () => {
  const bundle = aBundle();
  const path = saveBundle(bundle, root);

  assert.equal(path, join(cycleDir(bundle.cycle_id, root), "evidence.json"));
  assert.deepEqual(loadBundle(bundle.cycle_id, root), bundle);
  assert.equal(loadBundle("no-such-cycle", root), null);
});

test("a credential that rode in on collected evidence never reaches the disk", () => {
  // Evidence is assembled out of real repository content — grep output, roadmap prose, gate
  // logs. A token pasted into any of those would otherwise be written out verbatim.
  const bundle = aBundle({ documented_gaps: { where_we_are: ["the deploy key is ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"] } });
  saveBundle(bundle, root);

  const written = readFileSync(join(cycleDir(bundle.cycle_id, root), "evidence.json"), "utf8");
  assert.ok(!written.includes("ghp_AAAA"), "the token survived into the bundle");
  assert.ok(written.includes("[redacted]"));
});

test("cycles list newest first", () => {
  saveBundle(aBundle({ cycle_id: "old", collected_at: "2026-01-01T00:00:00.000Z" }), root);
  saveBundle(aBundle({ cycle_id: "new", collected_at: "2026-07-01T00:00:00.000Z" }), root);

  assert.deepEqual(
    listCycles(root).map((b) => b.cycle_id),
    ["new", "old"],
  );
  assert.deepEqual(listCycles(join(root, "never-collected")), []);
});

test("the workspace is pinned at the audited commit, not at the source repo's HEAD", () => {
  const source = mkdtempSync(join(tmpdir(), "rune-src-"));
  mkdirSync(join(source, "docs"), { recursive: true });
  const roadmap = join(source, "docs", "roadmap.md");

  git(["init", "--quiet", "-b", "main"], { cwd: source });
  git(["config", "user.email", "t@example.com"], { cwd: source });
  git(["config", "user.name", "t"], { cwd: source });
  writeFileSync(roadmap, "# audited\n");
  git(["add", "-A"], { cwd: source });
  git(["commit", "--quiet", "-m", "audited"], { cwd: source });
  const audited = git(["rev-parse", "HEAD"], { cwd: source });

  // The maintainer keeps working while the cycle runs. Collection must not see this.
  writeFileSync(roadmap, "# a later edit\n");
  git(["add", "-A"], { cwd: source });
  git(["commit", "--quiet", "-m", "later"], { cwd: source });

  const workspace = cycleWorkspace("c1", audited, { root, mirror: source });
  assert.equal(readFileSync(join(workspace, "docs", "roadmap.md"), "utf8"), "# audited\n");
  assert.equal(git(["rev-parse", "HEAD"], { cwd: workspace }), audited);

  rmSync(source, { recursive: true, force: true });
});
