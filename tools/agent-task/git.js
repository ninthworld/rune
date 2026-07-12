import { execFileSync } from "node:child_process";

/**
 * Runs git with the repository's own hooks disabled.
 *
 * ADR 0016: no command the runner issues may execute code the provider controls. A run
 * clone's `.git/hooks` is writable by the provider, so a `pre-commit` or `pre-push` hook it
 * drops there would run as the runner. Every git call in this codebase goes through here —
 * never `execFileSync("git", …)` directly.
 */
export function git(args, { cwd, env } = {}) {
  return execFileSync("git", ["-c", "core.hooksPath=/dev/null", ...args], {
    cwd,
    encoding: "utf8",
    env: env ?? process.env,
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}
