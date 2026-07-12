import { randomBytes } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { runsRoot } from "./config.js";

export const RUN_SCHEMA_VERSION = 1;

/**
 * States in which the run still holds its claim — the branch exists and the issue is
 * `status:in-progress`, so something must eventually `release` it.
 *
 * Provider failures are in here on purpose (ADR 0016): a failed run keeps its claim, its
 * worktree, and its diff, because that is what makes it resumable. Only `release` and a lost
 * claim end a run.
 */
const ACTIVE = new Set([
  "claiming",
  "claimed",
  "implementing",
  "implemented",
  "provider_failed",
  "provider_timeout",
  "cancelled",
]);

export function isActive(run) {
  return ACTIVE.has(run.state);
}

export function newRunId(issue, now = new Date()) {
  const stamp = now.toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
  return `${issue}-${stamp}-${randomBytes(3).toString("hex")}`;
}

export function runDir(runId, root = runsRoot()) {
  return join(root, runId);
}

/**
 * Writes the run record.
 *
 * Written before the claim is attempted and updated after, so a crash mid-claim leaves a
 * record pointing at the branch that may exist on GitHub rather than an orphan.
 */
export function saveRun(run, root = runsRoot()) {
  const dir = runDir(run.run_id, root);
  mkdirSync(join(dir, "logs"), { recursive: true });
  const next = { ...run, updated_at: new Date().toISOString() };
  writeFileSync(join(dir, "run.json"), `${JSON.stringify(next, null, 2)}\n`);
  return next;
}

export function transition(run, state, root = runsRoot()) {
  const at = new Date().toISOString();
  return saveRun({ ...run, state, events: [...(run.events || []), { state, at }] }, root);
}

/**
 * Keeps `updated_at` fresh while something long is running.
 *
 * Staleness is what tells a human that a claim was abandoned — a machine that was closed, killed,
 * or rebooted leaves the branch and the labels behind, and nothing on GitHub expires. Without a
 * heartbeat, a legitimately long provider run would look abandoned; with one, only an actually
 * dead run does.
 */
export function heartbeat(run, root = runsRoot(), intervalMs = 60_000) {
  const timer = setInterval(() => {
    try {
      saveRun(run, root);
    } catch {
      // A run directory that vanished mid-run is not worth crashing the run over.
    }
  }, intervalMs);
  timer.unref();
  return () => clearInterval(timer);
}

export function loadRun(runId, root = runsRoot()) {
  try {
    return JSON.parse(readFileSync(join(runDir(runId, root), "run.json"), "utf8"));
  } catch (err) {
    if (err.code === "ENOENT") return null;
    throw err;
  }
}

export function listRuns(root = runsRoot()) {
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch (err) {
    if (err.code === "ENOENT") return [];
    throw err;
  }
  return entries
    .filter((e) => e.isDirectory())
    .map((e) => loadRun(e.name, root))
    .filter((run) => run !== null)
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
}

/**
 * The local view of who owns an issue. Authoritative ownership is the remote branch — a
 * run started on another machine has no record here — so callers must treat a null result
 * as "not claimed *by this machine*", never as "not claimed".
 */
export function activeRunForIssue(issue, root = runsRoot()) {
  return listRuns(root).find((run) => run.issue === issue && isActive(run)) || null;
}

export function removeRun(runId, root = runsRoot()) {
  rmSync(runDir(runId, root), { recursive: true, force: true });
}
