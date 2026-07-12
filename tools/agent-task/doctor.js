import { execFileSync } from "node:child_process";
import { accessSync, constants } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { hasCredential } from "./isolation.js";

/**
 * Resolves a command on PATH, or null.
 *
 * `command -v` is a shell builtin, not an executable, so it needs a shell — and the name
 * is passed as an argument rather than interpolated into the script.
 */
function which(cmd) {
  try {
    const out = execFileSync("bash", ["-c", 'command -v -- "$1"', "bash", cmd], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return out.trim() || null;
  } catch {
    return null;
  }
}

function readable(path) {
  try {
    accessSync(path, constants.R_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Preflight for the machine, not the issue.
 *
 * A missing CLI, an unreadable key, or an isolation backend the host cannot provide should
 * surface here — before an issue is claimed — rather than after a provider has already run.
 */
export function diagnose() {
  const keyPath = process.env.RUNE_BOT_KEY || join(homedir(), ".config", "rune", "rune-agent.pem");
  const appId = process.env.RUNE_BOT_APP_ID || (readable(join(homedir(), ".config", "rune", "app-id")) ? "set" : null);
  const major = Number(process.versions.node.split(".")[0]);

  // A run clone inherits no identity, so the runner passes the author explicitly — but it has
  // to have one to pass. Better to say so here than to die at `git commit` after a 40-minute run.
  let identity = "";
  try {
    identity = execFileSync("git", ["config", "--get", "user.email"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    identity = "";
  }

  const checks = [
    { name: "node >= 20", ok: major >= 20, detail: process.versions.node, required: true },
    { name: "git", ok: Boolean(which("git")), detail: which("git") || "not found", required: true },
    { name: "git commit identity", ok: Boolean(identity), detail: identity || "unset — run `git config --global user.email ...`", required: true },
    { name: "gh", ok: Boolean(which("gh")), detail: which("gh") || "not found", required: true },
    { name: "app private key", ok: readable(keyPath), detail: keyPath, required: true },
    { name: "app id", ok: Boolean(appId), detail: appId ? "configured" : "set RUNE_BOT_APP_ID or ~/.config/rune/app-id", required: true },
  ];

  // Slice 2 runs providers under a separate UID or a container (ADR 0016). Reported now so
  // a host that can offer neither is known before it matters, not on the first real run.
  const isolation = ["podman", "docker", "systemd-run"].filter((c) => which(c));
  checks.push({
    name: "provider isolation backend",
    ok: isolation.length > 0,
    detail: isolation.length > 0 ? isolation.join(", ") : "none — runs will need --unsafe-same-uid",
    required: false,
  });

  // A provider's interactive `/login` lives under the real HOME, which the sandbox replaces — so
  // an installed CLI that has only ever been logged in interactively will sit there asking for
  // `/login` with nobody to answer. Checked here, because the alternative is discovering it after
  // an issue has been claimed.
  const advice = {
    claude: "run `claude setup-token` once, then export CLAUDE_CODE_OAUTH_TOKEN",
    codex: "export OPENAI_API_KEY",
  };
  for (const cli of ["claude", "codex"]) {
    const installed = which(cli);
    const authed = hasCredential(cli);
    checks.push({
      name: `provider: ${cli}`,
      ok: Boolean(installed) && authed,
      detail: !installed ? "not installed" : authed ? `${installed} (token in env)` : `installed, but NO TOKEN — ${advice[cli]}`,
      required: false,
    });
  }

  return checks;
}
