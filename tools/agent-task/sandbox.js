import { existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";

import { repoSlug, stateRoot } from "./config.js";
import { git } from "./git.js";
import { runDir } from "./runs.js";

/**
 * The runner-owned mirror: a bare clone that no provider ever touches.
 *
 * It exists so the runner has a repository whose config and hooks are its own. Run clones
 * are made *from* it (cheap — objects are hardlinked), and in slice 3 pushes go out *from*
 * it, so no credentialed git command ever runs in a directory a provider could rewrite.
 */
export function mirrorPath() {
  return join(stateRoot(), "mirror");
}

export function ensureMirror({ path = mirrorPath(), url } = {}) {
  const remote = url ?? `https://github.com/${repoSlug().slug}.git`;
  if (!existsSync(path)) {
    mkdirSync(stateRoot(), { recursive: true });
    git(["clone", "--bare", remote, path]);
  }
  git(["fetch", "--prune", "origin", "+refs/heads/*:refs/heads/*"], { cwd: path });
  return path;
}

/**
 * Creates the working copy a provider runs in.
 *
 * Deliberately a clone and **not** `git worktree add`: a worktree shares `.git/config` and
 * `.git/hooks` with its parent, so a provider could write a hook that fires in the
 * maintainer's own checkout. A clone gets its own `.git` entirely.
 *
 * `origin` therefore points at the local mirror, not GitHub — the provider has no network
 * remote to push to even if it tries.
 */
export function createWorkspace(run, { root, mirror = mirrorPath() } = {}) {
  const repo = join(runDir(run.run_id, root), "repo");
  rmSync(repo, { recursive: true, force: true });

  // `--no-hardlinks` costs disk and a little time, and buys the mirror's integrity. A local
  // clone hardlinks object files by default, which means clone and mirror share inodes: a
  // provider running under `--unsafe-same-uid` could chmod and rewrite one in place and
  // corrupt the trusted mirror through the link. ADR 0016 assumed hardlinks were free; they
  // are not free under the escape hatch it also allows.
  git(["clone", "--no-hardlinks", "--quiet", mirror, repo]);
  git(["checkout", "--quiet", "-b", run.branch, run.base_sha], { cwd: repo });

  return repo;
}

export function workspacePath(run, { root } = {}) {
  return join(runDir(run.run_id, root), "repo");
}
