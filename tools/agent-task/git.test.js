import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { git } from "./git.js";

let repo;
beforeEach(() => {
  repo = mkdtempSync(join(tmpdir(), "rune-git-"));
  git(["init", "--quiet", "--initial-branch=main"], { cwd: repo });
  git(["config", "user.email", "t@example.com"], { cwd: repo });
  git(["config", "user.name", "t"], { cwd: repo });
});
afterEach(() => {
  rmSync(repo, { recursive: true, force: true });
});

/** A hook that proves it ran by creating a file the test can look for. */
function plantHook(name) {
  const hooks = join(repo, ".git", "hooks");
  mkdirSync(hooks, { recursive: true });
  const hook = join(hooks, name);
  writeFileSync(hook, `#!/usr/bin/env bash\ntouch "${join(repo, `${name}.fired`)}"\n`);
  chmodSync(hook, 0o755);
}

test("git runs with the repository's hooks disabled", () => {
  // ADR 0016: a run clone's .git/hooks is writable by the provider, so a hook it drops there
  // would execute as the runner — with the runner's access, in the runner's process. Every
  // git call the runner makes must therefore ignore them.
  plantHook("pre-commit");
  writeFileSync(join(repo, "file.txt"), "hello\n");

  git(["add", "file.txt"], { cwd: repo });
  git(["commit", "--quiet", "-m", "test"], { cwd: repo });

  assert.equal(existsSync(join(repo, "pre-commit.fired")), false, "a provider-planted hook must not run");
  assert.match(git(["log", "--oneline", "-1"], { cwd: repo }), /test/, "the commit itself still happened");
});

test("a failing hook cannot block the runner either", () => {
  const hooks = join(repo, ".git", "hooks");
  mkdirSync(hooks, { recursive: true });
  writeFileSync(join(hooks, "pre-commit"), "#!/usr/bin/env bash\nexit 1\n");
  chmodSync(join(hooks, "pre-commit"), 0o755);

  writeFileSync(join(repo, "f.txt"), "x\n");
  git(["add", "f.txt"], { cwd: repo });
  git(["commit", "--quiet", "-m", "still commits"], { cwd: repo });

  assert.match(git(["log", "--oneline", "-1"], { cwd: repo }), /still commits/);
});
