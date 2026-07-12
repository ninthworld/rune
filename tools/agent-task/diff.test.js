import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { inspect, isCiPath } from "./diff.js";
import { git } from "./git.js";

let repo;
let base;

beforeEach(() => {
  repo = mkdtempSync(join(tmpdir(), "rune-diff-"));
  git(["init", "--quiet", "--initial-branch=main"], { cwd: repo });
  git(["config", "user.email", "t@example.com"], { cwd: repo });
  git(["config", "user.name", "t"], { cwd: repo });
  write("README.md", "hello\n");
  git(["add", "--all"], { cwd: repo });
  git(["commit", "--quiet", "-m", "base"], { cwd: repo });
  base = git(["rev-parse", "HEAD"], { cwd: repo });
});
afterEach(() => {
  rmSync(repo, { recursive: true, force: true });
});

function write(file, contents) {
  mkdirSync(dirname(join(repo, file)), { recursive: true });
  writeFileSync(join(repo, file), contents);
}

const look = (opts = {}) => inspect(repo, { baseSha: base, ...opts });

test("CI-governance paths are recognised", () => {
  for (const path of [".github/workflows/ci.yml", ".github/CODEOWNERS", "Makefile", "scripts/bot-pr.sh", ".github/rulesets/main.json"]) {
    assert.equal(isCiPath(path), true, path);
  }
  assert.equal(isCiPath("crates/rune-engine/src/lib.rs"), false);
});

test("a real change passes inspection", () => {
  write("crates/rune-engine/src/lib.rs", "// work\n");
  const found = look();
  assert.equal(found.ok, true);
  assert.deepEqual(found.files, ["crates/rune-engine/src/lib.rs"]);
});

test("a provider that changed nothing is a no_op", () => {
  assert.equal(look().violations[0].outcome, "no_op");
});

test("a provider that commits is refused — the runner owns commits", () => {
  write("f.rs", "x\n");
  git(["add", "--all"], { cwd: repo });
  git(["commit", "--quiet", "-m", "provider commit"], { cwd: repo });

  const found = look();
  assert.equal(found.ok, false);
  assert.equal(found.violations[0].outcome, "scope_rejected");
  assert.match(found.violations[0].detail, /the runner owns commits/);
});

test("generated directories are refused", () => {
  write("target/debug/junk", "binary\n");
  write("node_modules/pkg/index.js", "x\n");
  const found = look();
  assert.equal(found.ok, false);
  assert.match(found.violations.map((v) => v.detail).join(), /generated paths/);
});

test("a credential in the diff is refused", () => {
  write("config.rs", 'let token = "ghs_abcdefghijklmnopqrstuvwxyz0123456789";\n');
  git(["add", "--all"], { cwd: repo });

  const found = look();
  assert.equal(found.ok, false);
  assert.match(found.violations.map((v) => v.detail).join(), /shaped like a credential/);
});

test("a CI-governance change is refused without --allow-ci, and permitted with it", () => {
  write(".github/workflows/ci.yml", "name: CI\n");

  const refused = look();
  assert.equal(refused.ok, false);
  assert.equal(refused.violations[0].outcome, "ci_change_refused");
  assert.deepEqual(refused.ciPaths, [".github/workflows/ci.yml"]);

  const allowed = look({ allowCi: true });
  assert.equal(allowed.ok, true);
  assert.deepEqual(allowed.ciPaths, [".github/workflows/ci.yml"], "still reported, so the PR can flag it");
});

test("untracked files count as work, not as an empty diff", () => {
  // `git diff` alone would miss a brand-new file, and "the provider created a whole new module"
  // is the most ordinary thing an implementation run can do.
  write("crates/rune-engine/src/new_module.rs", "// new\n");
  const found = look();
  assert.equal(found.ok, true);
  assert.ok(found.files.includes("crates/rune-engine/src/new_module.rs"));
});
