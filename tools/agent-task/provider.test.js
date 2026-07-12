import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { runProvider } from "./provider.js";
import { runDir } from "./runs.js";

let root;
let dir;
const RUN = { run_id: "186-p", issue: 186, provider: "local", branch: "agent/186-p" };
const SAME_UID = { mode: "same-uid" };

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "rune-prov-"));
  dir = runDir(RUN.run_id, root);
  mkdirSync(join(dir, "logs"), { recursive: true });
  mkdirSync(join(dir, "home"), { recursive: true });
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
  delete process.env.RUNE_LOCAL_CMD;
});

const log = () => readFileSync(join(dir, "logs", "provider.log"), "utf8");

const run = (opts = {}) =>
  runProvider({ run: RUN, workspace: dir, isolation: SAME_UID, root, brief: "the brief", ...opts });

test("a provider that exits 0 is implemented; a non-zero exit is provider_failed", async () => {
  process.env.RUNE_LOCAL_CMD = "exit 0";
  assert.equal((await run()).outcome, "implemented");

  process.env.RUNE_LOCAL_CMD = "exit 3";
  const failed = await run();
  assert.equal(failed.outcome, "provider_failed");
  assert.equal(failed.exit_code, 3);
});

test("the outcome is observed from the exit code, not taken from the provider's own report", async () => {
  // A provider claiming success while failing must not be believed — ADR 0016 makes outcome a
  // runner-observed field and provider_usage merely advisory.
  writeFileSync(join(dir, "result.json"), JSON.stringify({ outcome: "implemented", tokens: 42 }));
  process.env.RUNE_LOCAL_CMD = "exit 1";

  const result = await run();
  assert.equal(result.outcome, "provider_failed");
  assert.deepEqual(result.provider_usage, { outcome: "implemented", tokens: 42 });
});

test("a provider that overruns its timeout is killed, along with everything it spawned", async () => {
  // The child spawns a grandchild and exits; killing only the child would leave the
  // grandchild running. The runner signals the whole process group.
  const marker = join(root, "grandchild-still-alive");
  process.env.RUNE_LOCAL_CMD = `bash -c 'sleep 30; touch "${marker}"' & sleep 30`;

  const result = await run({ timeoutMs: 300, graceMs: 100 });
  assert.equal(result.outcome, "provider_timeout");

  // If the group kill worked the grandchild is gone and never gets to write its marker.
  await new Promise((r) => setTimeout(r, 600));
  assert.equal(existsSync(marker), false, "the grandchild outlived the run");
});

test("provider output is captured to a log", async () => {
  process.env.RUNE_LOCAL_CMD = 'echo "on stdout"; echo "on stderr" >&2';
  await run();
  assert.match(log(), /on stdout/);
  assert.match(log(), /on stderr/);
});

test("credentials the provider prints are redacted before they reach the log", async () => {
  const token = "ghs_abcdefghijklmnopqrstuvwxyz0123456789";
  process.env.RUNE_LOCAL_CMD = `echo "stole: ${token}"`;

  await run({ secrets: [token] });

  assert.doesNotMatch(log(), /ghs_/, "the token must not be in the log");
  assert.match(log(), /stole: \[redacted\]/);
});

test("the brief reaches the provider by path, outside the working copy", async () => {
  writeFileSync(join(dir, "brief.md"), "# the brief\n");
  process.env.RUNE_LOCAL_CMD = 'cat "$RUNE_BRIEF"; echo "issue=$RUNE_ISSUE run=$RUNE_RUN_ID"';

  await run();

  assert.match(log(), /# the brief/);
  assert.match(log(), /issue=186 run=186-p/);
});
