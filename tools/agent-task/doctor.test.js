import assert from "node:assert/strict";
import { test } from "node:test";

import { diagnose } from "./doctor.js";

const find = (name) => diagnose().find((c) => c.name === name);

test("doctor resolves a command that is certainly on PATH", () => {
  // Regression: the first cut shelled out to `command -v`, which is a shell builtin and
  // not an executable, so every CLI check reported "not found" on a machine that had them.
  // `git` is the safe probe — no checkout, and no CI job, exists without it.
  const git = find("git");
  assert.equal(git.ok, true, git.detail);
  assert.match(git.detail, /\//, "git should resolve to a path");
});

test("doctor reports the node version as a required check", () => {
  const node = find("node >= 20");
  assert.equal(node.required, true);
  assert.equal(node.ok, true);
});

test("provider CLIs and isolation are advisory, not required", () => {
  for (const name of ["provider isolation backend", "provider: claude", "provider: codex"]) {
    assert.equal(find(name).required, false);
  }
});
