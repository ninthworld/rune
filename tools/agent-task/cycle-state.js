import { randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { stateRoot } from "./config.js";
import { git } from "./git.js";
import { redact } from "./redact.js";
import { mirrorPath } from "./sandbox.js";

/**
 * Where a stewardship cycle's artifacts live: outside the repository, exactly like a run's
 * (ADR 0017 §"What stays out of tracked paths", ADR 0016's `runs/`).
 *
 * Evidence bundles quote source, list `file:line` stubs, and carry provider-adjacent
 * material; a bundle that landed in a tracked path would end up in a diff, in a PR, and in
 * `main`. Keeping the whole cycle under `$XDG_STATE_HOME` makes that impossible rather than
 * merely discouraged — the tool has no path into the working tree to begin with.
 */
export function cyclesRoot() {
  return join(stateRoot(), "cycles");
}

export function cycleDir(cycleId, root = cyclesRoot()) {
  return join(root, cycleId);
}

/**
 * The cycle's stable identity, minted once at collection and carried by every later stage.
 *
 * Stable because idempotency hangs off it: a resumed cycle, a re-collected bundle, and a
 * partially applied wave all have to be recognizable as *the same cycle* rather than
 * counted twice (#189). Same shape as `newRunId`, for the same reason.
 */
export function newCycleId(milestone, now = new Date()) {
  const stamp = now.toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
  return `${milestoneSlug(milestone)}-${stamp}-${randomBytes(3).toString("hex")}`;
}

/** `"M3 — A real card pool"` → `m3-a-real-card-pool`. */
export function milestoneSlug(milestone) {
  return String(milestone)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/, "");
}

/**
 * The checkout every read is made against: a clone pinned at `base_commit_sha`.
 *
 * A clone of the runner's own mirror rather than the maintainer's working tree, for the
 * reason `sandbox.js` gives — a clone gets its own `.git`, so no hook or config from
 * elsewhere fires — and, more mundanely, because the working tree is *dirty*: a collector
 * that read `docs/roadmap.md` from wherever it happened to be invoked would audit whatever
 * the maintainer had half-edited that morning, not the commit it claims to have audited.
 */
export function cycleWorkspace(cycleId, baseSha, { root, mirror = mirrorPath() } = {}) {
  const repo = join(cycleDir(cycleId, root), "repo");
  rmSync(repo, { recursive: true, force: true });
  mkdirSync(cycleDir(cycleId, root), { recursive: true });

  git(["clone", "--no-hardlinks", "--quiet", mirror, repo]);
  git(["checkout", "--quiet", "--detach", baseSha], { cwd: repo });
  return repo;
}

/**
 * Writes the evidence bundle for a cycle.
 *
 * Redacted on the way out, not on the way in. The bundle is assembled from real repository
 * content — grep hits, roadmap prose, gate output — and a credential that leaked into any
 * of those would otherwise be written to disk verbatim. `redact.js` is the same scrubber
 * the runner puts between a provider and its log file.
 */
export function saveBundle(bundle, root = cyclesRoot()) {
  const dir = cycleDir(bundle.cycle_id, root);
  mkdirSync(dir, { recursive: true });
  const path = join(dir, "evidence.json");
  writeFileSync(path, `${redact(JSON.stringify(bundle, null, 2))}\n`);
  return path;
}

export function loadBundle(cycleId, root = cyclesRoot()) {
  try {
    return JSON.parse(readFileSync(join(cycleDir(cycleId, root), "evidence.json"), "utf8"));
  } catch (err) {
    if (err.code === "ENOENT") return null;
    throw err;
  }
}

/** Cycles this machine knows about, newest first. */
export function listCycles(root = cyclesRoot()) {
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch (err) {
    if (err.code === "ENOENT") return [];
    throw err;
  }
  return entries
    .filter((e) => e.isDirectory())
    .map((e) => loadBundle(e.name, root))
    .filter((bundle) => bundle !== null)
    .sort((a, b) => b.collected_at.localeCompare(a.collected_at));
}
