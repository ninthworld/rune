import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { git } from "./git.js";
import { commitWork, openDraftPr, pushFromMirror, rebaseOntoMain } from "./publish.js";

let dir;
let mirror;
let workspace;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "rune-pub-"));

  // An "upstream" playing the part of GitHub, a mirror cloned from it, and a workspace cloned
  // from the mirror — the same topology the runner builds.
  const upstream = join(dir, "upstream");
  git(["init", "--quiet", "--bare", "--initial-branch=main", upstream]);

  const seed = join(dir, "seed");
  git(["clone", "--quiet", upstream, seed]);
  git(["config", "user.email", "t@example.com"], { cwd: seed });
  git(["config", "user.name", "t"], { cwd: seed });
  writeFileSync(join(seed, "README.md"), "base\n");
  git(["add", "--all"], { cwd: seed });
  git(["commit", "--quiet", "-m", "base"], { cwd: seed });
  git(["push", "--quiet", "origin", "main"], { cwd: seed });

  mirror = join(dir, "mirror");
  git(["clone", "--bare", "--quiet", upstream, mirror]);

  workspace = join(dir, "ws");
  git(["clone", "--quiet", "--no-hardlinks", mirror, workspace]);
  git(["config", "user.email", "dev@example.com"], { cwd: workspace });
  git(["config", "user.name", "Dev"], { cwd: workspace });
  git(["checkout", "--quiet", "-b", "agent/1-x"], { cwd: workspace });
});
afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

test("the commit keeps a Conventional Commits subject and links the issue", () => {
  writeFileSync(join(workspace, "f.rs"), "work\n");
  commitWork(workspace, { issue: { number: 42, title: "feat(engine): do the thing" } });

  const message = git(["log", "-1", "--format=%B"], { cwd: workspace });
  assert.match(message, /^feat\(engine\): do the thing/);
  assert.match(message, /Refs #42/);
});

test("an issue title that is not conventional gets a type prefix", () => {
  writeFileSync(join(workspace, "f.rs"), "work\n");
  commitWork(workspace, { issue: { number: 7, title: "Make the thing faster" } });
  assert.match(git(["log", "-1", "--format=%s"], { cwd: workspace }), /^feat: Make the thing faster/);
});

test("the commit is authored by the human, not the bot — only the push carries the bot identity", () => {
  writeFileSync(join(workspace, "f.rs"), "work\n");
  commitWork(workspace, { issue: { number: 1, title: "fix: x" }, author: { name: "Dev", email: "dev@example.com" } });
  assert.match(git(["log", "-1", "--format=%an <%ae>"], { cwd: workspace }), /Dev <dev@example\.com>/);
});

test("the author is passed explicitly, because a fresh clone inherits no identity", () => {
  // Regression: an identity set only in the maintainer's checkout (not globally) does not
  // travel to a fresh clone, so the runner died at `git commit` with "Author identity unknown"
  // after doing all of the work.
  const bare = join(dir, "no-identity");
  git(["clone", "--quiet", "--no-hardlinks", mirror, bare]);
  git(["checkout", "--quiet", "-b", "agent/2-y"], { cwd: bare });
  writeFileSync(join(bare, "f.rs"), "work\n");

  // `git config --get` exits non-zero when the key is unset — which is the whole condition here.
  assert.throws(() => git(["config", "--local", "--get", "user.email"], { cwd: bare }));

  commitWork(bare, { issue: { number: 2, title: "feat: y" }, author: { name: "A", email: "a@b.c" } });
  assert.match(git(["log", "-1", "--format=%ae"], { cwd: bare }), /a@b\.c/);
});

test("untracked files are included in the commit", () => {
  writeFileSync(join(workspace, "brand-new.rs"), "new\n");
  commitWork(workspace, { issue: { number: 1, title: "feat: x" } });
  assert.match(git(["show", "--stat", "--format=", "HEAD"], { cwd: workspace }), /brand-new\.rs/);
});

test("push goes out from the mirror, never from the provider's workspace", () => {
  writeFileSync(join(workspace, "f.rs"), "work\n");
  commitWork(workspace, { issue: { number: 1, title: "feat: x" } });

  const calls = [];
  pushFromMirror({
    workspace,
    branch: "agent/1-x",
    remoteSha: "abc123",
    mirror,
    execImpl: (cmd, args) => calls.push([cmd, args]) && "",
  });

  // The commit reached the mirror by object transfer — a commit cannot carry config or hooks.
  assert.equal(git(["rev-parse", "agent/1-x"], { cwd: mirror }), git(["rev-parse", "HEAD"], { cwd: workspace }));

  const [cmd, args] = calls[0];
  assert.match(cmd, /bot-push\.sh$/);
  assert.deepEqual(args.slice(0, 4), ["--repo", mirror, "--branch", "agent/1-x"]);
  assert.equal(args.some((a) => a.includes(workspace)), false, "the push must not run in the workspace");
});

test("the lease names the SHA it expects to overwrite", () => {
  // A bare mirror has no remote-tracking ref, so a bare `--force-with-lease` would have nothing
  // to compare against and would silently degrade to a force push (the #208 failure, again).
  writeFileSync(join(workspace, "f.rs"), "w\n");
  commitWork(workspace, { issue: { number: 1, title: "feat: x" } });

  const calls = [];
  const push = (remoteSha) =>
    pushFromMirror({ workspace, branch: "agent/1-x", remoteSha, mirror, execImpl: (c, a) => calls.push(a) && "" });

  push("deadbeef");
  assert.ok(calls[0].includes("--force-with-lease=refs/heads/agent/1-x:deadbeef"));

  push(null);
  assert.equal(calls[1].some((a) => a.startsWith("--force-with-lease")), false, "no lease when there is no remote branch yet");
});

test("the draft PR is opened for an explicit head, and never pushes again", () => {
  const calls = [];
  const pr = openDraftPr({
    branch: "agent/1-x",
    title: "feat: x",
    body: "body",
    execImpl: (cmd, args) => {
      calls.push([cmd, args]);
      return "https://github.com/ninthworld/rune/pull/321\n";
    },
  });

  const [cmd, args] = calls[0];
  assert.match(cmd, /bot-pr\.sh$/);
  assert.deepEqual(args, ["--head", "agent/1-x", "--draft", "--no-push", "feat: x", "body"]);
  assert.equal(pr.number, 321);
});

test("rebase brings the branch onto current main and reports whether it moved", () => {
  writeFileSync(join(workspace, "mine.rs"), "mine\n");
  commitWork(workspace, { issue: { number: 1, title: "feat: mine" } });

  assert.equal(rebaseOntoMain(workspace, { mirror }).moved, false, "nothing moved on main yet");

  // Someone else lands on main while the run was working.
  const other = join(dir, "other");
  git(["clone", "--quiet", mirror, other]);
  git(["config", "user.email", "o@example.com"], { cwd: other });
  git(["config", "user.name", "O"], { cwd: other });
  writeFileSync(join(other, "theirs.rs"), "theirs\n");
  git(["add", "--all"], { cwd: other });
  git(["commit", "--quiet", "-m", "feat: theirs"], { cwd: other });
  git(["push", "--quiet", "origin", "main"], { cwd: other });

  const rebased = rebaseOntoMain(workspace, { mirror });
  assert.equal(rebased.moved, true);
  assert.match(git(["log", "--oneline", "-2"], { cwd: workspace }), /feat: theirs/);
  assert.match(git(["log", "-1", "--format=%s"], { cwd: workspace }), /feat: mine/, "our work sits on top");
});

test("a conflicting rebase aborts cleanly and is reported as rebase_conflict", () => {
  writeFileSync(join(workspace, "same.rs"), "ours\n");
  commitWork(workspace, { issue: { number: 1, title: "feat: ours" } });

  const other = join(dir, "other2");
  git(["clone", "--quiet", mirror, other]);
  git(["config", "user.email", "o@example.com"], { cwd: other });
  git(["config", "user.name", "O"], { cwd: other });
  writeFileSync(join(other, "same.rs"), "theirs\n");
  git(["add", "--all"], { cwd: other });
  git(["commit", "--quiet", "-m", "feat: theirs"], { cwd: other });
  git(["push", "--quiet", "origin", "main"], { cwd: other });

  assert.throws(
    () => rebaseOntoMain(workspace, { mirror }),
    (err) => err.outcome === "rebase_conflict",
  );
  // The abort matters: a workspace left mid-rebase would be unusable on resume.
  assert.equal(git(["status", "--porcelain"], { cwd: workspace }), "");
});
