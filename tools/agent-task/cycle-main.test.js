import assert from "node:assert/strict";
import { test } from "node:test";

import { main } from "./cycle-main.js";

/** Collects what the command printed, so nothing is asserted against a real terminal. */
function run(argv) {
  const lines = [];
  const sink = (line) => lines.push(String(line));
  return main(argv, { out: sink, err: sink }).then((code) => ({ code, output: lines.join("\n") }));
}

test("bare invocation prints the usage and fails", async () => {
  const { code, output } = await run([]);

  assert.equal(code, 1);
  assert.match(output, /scripts\/agent-cycle — milestone stewardship \(ADR 0017\)/);
  // The usage is where a maintainer learns what this command does *not* do yet, so the honest
  // statement of that lives in the tool rather than only in the ADR.
  assert.match(output, /reads only/);
  assert.match(output, /a milestone is still\nreconciled by a human/);
});

test("an unknown command fails without doing anything", async () => {
  const { code, output } = await run(["approve", "M3"]);

  assert.equal(code, 1);
  assert.match(output, /unknown command "approve"/);
});

test("bad arguments are rejected before anything is connected to, cloned, or run", async () => {
  // These paths would otherwise mint a bot token and clone the mirror. Argument validation
  // happens first, so a typo costs nothing — the same ordering the runner's preflight has.
  assert.deepEqual(await run(["collect"]), {
    code: 1,
    output: "✗ collect needs a milestone, e.g. scripts/agent-cycle collect M3",
  });
  assert.deepEqual(await run(["collect", "M3", "--gates", "bogus"]), {
    code: 1,
    output: "✗ --gates must be one of check, verify (got bogus)",
  });
  assert.deepEqual(await run(["show"]), {
    code: 1,
    output: "✗ show needs a cycle id (scripts/agent-cycle list)",
  });
});
