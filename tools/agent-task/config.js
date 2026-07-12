import { homedir, hostname, userInfo } from "node:os";
import { join } from "node:path";

export const PROVIDERS = ["claude", "codex", "local"];

export const LABELS = {
  ready: "status:ready",
  inProgress: "status:in-progress",
  review: "status:review",
  blocked: "status:blocked",
};

/** Where run state lives: outside the repository, so it can never land in a diff. */
export function stateRoot() {
  const xdg = process.env.XDG_STATE_HOME || join(homedir(), ".local", "state");
  return join(xdg, "rune");
}

export function runsRoot() {
  return join(stateRoot(), "runs");
}

export function repoSlug() {
  const slug = process.env.RUNE_BOT_REPO || "ninthworld/rune";
  const [owner, repo] = slug.split("/");
  if (!owner || !repo) throw new Error(`RUNE_BOT_REPO must be "owner/repo", got "${slug}"`);
  return { owner, repo, slug };
}

/** Identifies the human whose machine started the run. Recorded, never used to authorize. */
export function actor() {
  return `${userInfo().username}@${hostname()}`;
}
