import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable } from "node:stream";
import { afterEach, beforeEach, test } from "node:test";

import { GATES, GATE_SETS, runGates } from "./verify.js";
import { runDir } from "./runs.js";

let root;
const RUN = { run_id: "186-v", issue: 186, provider: "claude" };

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "rune-verify-"));
  mkdirSync(join(runDir(RUN.run_id, root), "logs"), { recursive: true });
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

/** A spawn that records the argv it was asked to run and exits with a scripted code. */
function fakeSpawn(codes) {
  const calls = [];
  const impl = (cmd, args) => {
    calls.push([cmd, ...args]);
    const child = new EventEmitter();
    child.stdout = Readable.from([`running ${args.join(" ")}\n`]);
    child.stderr = Readable.from([]);
    const code = codes.shift() ?? 0;
    setImmediate(() => child.emit("close", code));
    return child;
  };
  return { impl, calls };
}

const run = (codes, opts = {}) => {
  const { impl, calls } = fakeSpawn(codes);
  return runGates({
    run: RUN,
    workspace: "/w",
    isolation: { mode: "same-uid" },
    root,
    env: {},
    spawnImpl: impl,
    ...opts,
  }).then((result) => ({ ...result, calls }));
};

test("the gate set mirrors the four required CI checks", () => {
  assert.deepEqual(
    GATES.map((g) => g.name),
    ["Engine", "Client", "E2E", "cargo-deny"],
  );
  assert.deepEqual(GATE_SETS.verify, ["Engine", "Client", "E2E", "cargo-deny"]);
  assert.deepEqual(GATE_SETS.check, ["Engine", "Client"]);
});

test("the gates never try to provision a browser", () => {
  // `make e2e-browser` runs `playwright install --with-deps`, which shells out to apt-get as root.
  // The sandbox is unprivileged, so it would fail to `su` and take the E2E gate down with it. The
  // browser is provided (baked into the image), not installed by the gate.
  const e2e = GATES.find((g) => g.name === "E2E");
  assert.deepEqual(e2e.targets, ["e2e"]);
  assert.equal(
    GATES.some((g) => g.targets.includes("e2e-browser")),
    false,
  );
});

test("every gate is run and timed, and all-pass is ok", async () => {
  const result = await run([0, 0, 0, 0]);

  assert.equal(result.ok, true);
  assert.deepEqual(
    result.gates.map((g) => g.gate),
    ["Engine", "Client", "E2E", "cargo-deny"],
  );
  assert.ok(result.gates.every((g) => typeof g.duration_ms === "number"));
});

test("a failing gate stops the run and is named", async () => {
  const result = await run([0, 1]);

  assert.equal(result.ok, false);
  assert.deepEqual(
    result.gates.map((g) => [g.gate, g.ok]),
    [
      ["Engine", true],
      ["Client", false],
    ],
  );
  // Fail fast: a red Client makes E2E uninformative, and the run goes back to the provider anyway.
  assert.equal(result.calls.length, 2);
});

test("gates run through the isolation wrapper, in the sandbox", async () => {
  // Verification executes provider-controlled code by construction — a doctored Makefile runs
  // right here — so it must run inside the boundary, not in the runner's own context.
  const { impl, calls } = fakeSpawn([0, 0]);
  await runGates({
    run: RUN,
    workspace: "/w",
    isolation: { mode: "uid", user: "rune-provider" },
    root,
    env: { HOME: "/h" },
    set: "check",
    spawnImpl: impl,
  });

  assert.equal(calls[0][0], "sudo");
  assert.ok(calls[0].includes("rune-provider"));
  assert.ok(calls[0].includes("make"));
});

test("--gates check runs only the fast surface", async () => {
  const result = await run([0, 0], { set: "check" });
  assert.deepEqual(
    result.gates.map((g) => g.gate),
    ["Engine", "Client"],
  );
});

test("gate output is logged, with secrets redacted", async () => {
  const { impl } = fakeSpawn([0]);
  await runGates({
    run: RUN,
    workspace: "/w",
    isolation: { mode: "same-uid" },
    root,
    env: {},
    set: "check",
    secrets: ["ghs_abcdefghijklmnopqrstuvwxyz0123456789"],
    spawnImpl: impl,
  });

  const log = readFileSync(join(runDir(RUN.run_id, root), "logs", "verify.log"), "utf8");
  assert.match(log, /=== Engine: make engine-lint engine-test/);
  assert.doesNotMatch(log, /ghs_/);
});
