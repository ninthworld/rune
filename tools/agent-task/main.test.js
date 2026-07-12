import assert from "node:assert/strict";
import { test } from "node:test";

import { main } from "./main.js";

/** Runs a command with stdout/stderr captured, so a CLI test never mints a token or prints. */
async function run(argv) {
  const out = [];
  const err = [];
  const [stdout, stderr] = [process.stdout.write, process.stderr.write];
  process.stdout.write = (chunk) => out.push(chunk) && true;
  process.stderr.write = (chunk) => err.push(chunk) && true;
  try {
    return { code: await main(argv), out: out.join(""), err: err.join("") };
  } finally {
    process.stdout.write = stdout;
    process.stderr.write = stderr;
  }
}

test("no command prints usage and succeeds", async () => {
  const { code, out } = await run([]);
  assert.equal(code, 0);
  assert.match(out, /scripts\/agent-task/);
});

test("an unknown command exits 2", async () => {
  const { code, err } = await run(["merge"]);
  assert.equal(code, 2);
  assert.match(err, /unknown command "merge"/);
});

test("start requires a valid provider before it touches GitHub", async () => {
  for (const argv of [["start", "186"], ["start", "186", "--provider", "gpt"]]) {
    const { code, err } = await run(argv);
    assert.equal(code, 1);
    assert.match(err, /--provider must be one of claude, codex, local/);
  }
});

test("start requires an issue number", async () => {
  const { code, err } = await run(["start", "--provider", "claude"]);
  assert.equal(code, 1);
  assert.match(err, /needs an issue number/);
});

test("release requires an issue number", async () => {
  const { code, err } = await run(["release"]);
  assert.equal(code, 1);
  assert.match(err, /needs an issue number/);
});

test("cleanup without a target or --all is refused", async () => {
  const { code, err } = await run(["cleanup"]);
  assert.equal(code, 1);
  assert.match(err, /needs an issue number, a run id, or --all/);
});
