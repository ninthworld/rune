import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { git } from "./git.js";
import { mirrorPath } from "./sandbox.js";

const scripts = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "scripts");

/**
 * Commits the provider's work.
 *
 * The runner owns the commit, so its message follows Conventional Commits and its author is
 * the human whose machine ran it — only the *push* and the *PR* carry the bot identity
 * (#205/#206). A provider-authored commit would carry whatever identity the provider had.
 */
export function commitWork(workspace, { issue, author = resolveAuthor() }) {
  const subject = /^[a-z]+(\(.+\))?!?:/.test(issue.title) ? issue.title : `feat: ${issue.title}`;
  git(["add", "--all"], { cwd: workspace });
  git(
    [
      "-c",
      `user.name=${author.name}`,
      "-c",
      `user.email=${author.email}`,
      "commit",
      "--quiet",
      "-m",
      `${subject}\n\nRefs #${issue.number}`,
    ],
    { cwd: workspace },
  );
  return git(["rev-parse", "HEAD"], { cwd: workspace });
}

/**
 * The identity the commit is authored with.
 *
 * A run clone is fresh, so it inherits no `user.name`/`user.email` — and an identity set only
 * in the maintainer's own checkout (not globally) does not travel with it. The author is
 * resolved from the invoking checkout and passed explicitly, because the alternative is a run
 * that does all its work and then dies at `git commit` with "Author identity unknown".
 */
function resolveAuthor() {
  const read = (key) => {
    try {
      return git(["config", "--get", key]);
    } catch {
      return "";
    }
  };
  const name = read("user.name");
  const email = read("user.email");
  if (!name || !email) {
    throw new Error(
      "no git identity to author the commit with. Set one:\n" +
        '  git config --global user.name "Your Name"\n' +
        '  git config --global user.email "you@example.com"',
    );
  }
  return { name, email };
}

/**
 * Brings the branch onto current `main`.
 *
 * `main` requires branches to be up to date before merging, so a run that took an hour is
 * stale by the time it finishes. Linear history rules out a merge commit, so this rebases —
 * legitimate here because the branch is exclusively the runner's own.
 */
export function rebaseOntoMain(workspace, { mirror = mirrorPath() } = {}) {
  git(["fetch", "--quiet", "origin", "main"], { cwd: mirror });
  const before = git(["rev-parse", "HEAD"], { cwd: workspace });

  git(["fetch", "--quiet", "origin", "main"], { cwd: workspace });
  try {
    git(["rebase", "origin/main"], { cwd: workspace });
  } catch (err) {
    git(["rebase", "--abort"], { cwd: workspace });
    const conflict = new Error(`rebase onto current main conflicts:\n${err.stderr || err.message}`);
    conflict.outcome = "rebase_conflict";
    throw conflict;
  }

  const after = git(["rev-parse", "HEAD"], { cwd: workspace });
  return { moved: before !== after, head: after };
}

/**
 * Publishes the branch — from the mirror, never from the workspace.
 *
 * The workspace's `.git` is a directory the provider could have rewritten (config, hooks,
 * `url.insteadOf`). Fetching the finished branch into the trusted mirror moves only objects —
 * a commit cannot carry configuration — so the credentialed push runs in a repository whose
 * setup is entirely the runner's own (ADR 0016).
 *
 * The lease is explicit (`--force-with-lease=<ref>:<sha>`) rather than implicit: a bare mirror
 * has no remote-tracking ref to lease against, and naming the SHA we expect to overwrite is
 * stronger than trusting one anyway.
 */
export function pushFromMirror({ workspace, branch, remoteSha, mirror = mirrorPath(), execImpl = execFileSync }) {
  git(["fetch", "--quiet", workspace, `+refs/heads/${branch}:refs/heads/${branch}`], { cwd: mirror });

  const lease = remoteSha ? [`--force-with-lease=refs/heads/${branch}:${remoteSha}`] : [];
  execImpl(join(scripts, "bot-push.sh"), ["--repo", mirror, "--branch", branch, ...lease], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

/** Opens the draft PR as `rune-agent[bot]`. The branch is already pushed, so this never pushes. */
export function openDraftPr({ branch, title, body, execImpl = execFileSync }) {
  const out = execImpl(join(scripts, "bot-pr.sh"), ["--head", branch, "--draft", "--no-push", title, body], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  const url = String(out).trim().split("\n").pop();
  return { url, number: Number(url.split("/").pop()) };
}
