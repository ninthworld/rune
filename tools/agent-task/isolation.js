import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join } from "node:path";

import { runDir } from "./runs.js";

/**
 * Environment variables that must never reach a provider.
 *
 * The allowlist below already excludes them by construction — the provider's environment is
 * *built*, not inherited — but they are named explicitly so the intent survives a future
 * refactor that reaches for `...process.env`, and so a test can assert on the list.
 */
export const FORBIDDEN_ENV = ["BOT_TOKEN", "GH_TOKEN", "GITHUB_TOKEN", "RUNE_BOT_KEY", "RUNE_BOT_APP_ID"];

/** Model credentials a provider legitimately needs. Nothing else from the host is passed. */
const PROVIDER_CREDENTIALS = {
  claude: ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"],
  codex: ["OPENAI_API_KEY"],
  local: (process.env.RUNE_LOCAL_ENV || "").split(",").filter(Boolean),
};

function have(cmd) {
  try {
    execFileSync("bash", ["-c", 'command -v -- "$1"', "bash", cmd], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/**
 * Chooses how the provider will be contained.
 *
 * ADR 0016 makes this a boundary rather than a rule: "the provider may not push" is a
 * sentence, and the app's private key is readable by whatever UID runs the provider. So the
 * provider runs as *someone else* — another UID, or a container without the key mounted.
 *
 * When neither is available the run is refused. `--unsafe-same-uid` is the only way past
 * that, it is never chosen implicitly, and it is recorded in the run summary so a run made
 * without the boundary can never be mistaken for one made with it.
 */
export function resolveIsolation({ unsafeSameUid = false, providerUser = process.env.RUNE_PROVIDER_USER, image = process.env.RUNE_PROVIDER_IMAGE } = {}) {
  if (providerUser && have("sudo")) return { mode: "uid", user: providerUser };

  const engine = ["podman", "docker"].find(have);
  if (image && engine) return { mode: "container", engine, image };

  if (unsafeSameUid) return { mode: "same-uid" };

  throw new Error(
    [
      "refusing to run a provider with no isolation.",
      "",
      "The rune-agent private key is readable by the UID that runs the provider, so without a",
      "boundary the provider can mint its own token and open its own PRs (ADR 0016).",
      "",
      "Choose one:",
      "  RUNE_PROVIDER_USER=<user>    run the provider as another UID (needs passwordless sudo)",
      "  RUNE_PROVIDER_IMAGE=<image>  run the provider in a container (podman or docker)",
      "  --unsafe-same-uid            accept the risk; recorded in the run summary",
    ].join("\n"),
  );
}

/**
 * Builds the provider's environment from nothing.
 *
 * An allowlist, not a denylist: the provider gets what it needs to run and nothing else, so
 * a credential added to the maintainer's shell tomorrow is not silently inherited. `HOME` is
 * a scratch directory inside the run, which is what keeps `~/.config/rune` (the key) and
 * `~/.config/gh` (the maintainer's login) out of reach.
 */
export function providerEnv({ provider, workspace, run, root, scratchHome }) {
  const dir = runDir(run.run_id, root);
  const env = {
    PATH: process.env.PATH,
    LANG: process.env.LANG ?? "C.UTF-8",
    TERM: "dumb",
    HOME: scratchHome,
    RUNE_RUN_ID: run.run_id,
    RUNE_ISSUE: String(run.issue),
    RUNE_BRIEF: join(dir, "brief.md"),
    RUNE_RESULT: join(dir, "result.json"),
    RUNE_LOG_DIR: join(dir, "logs"),
    RUNE_WORKSPACE: workspace,
    // The provider's git operations are local-only anyway (origin is the mirror path), but
    // a credential helper inherited from the host would be one more way to reach GitHub.
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_SYSTEM: "/dev/null",
    GIT_TERMINAL_PROMPT: "0",
  };

  for (const name of PROVIDER_CREDENTIALS[provider] ?? []) {
    if (process.env[name]) env[name] = process.env[name];
  }
  return env;
}

export function scratchHomeFor(run, root) {
  const home = join(runDir(run.run_id, root), "home");
  mkdirSync(home, { recursive: true });
  return home;
}

/**
 * Wraps the provider's argv in whatever the chosen isolation needs.
 *
 * The environment is passed explicitly in every mode: `sudo -i` would reset it and `docker`
 * would inherit nothing, so each mode has to re-state the allowlist rather than assume it.
 */
export function wrap(argv, { isolation, env, workspace, dir }) {
  if (isolation.mode === "same-uid") return { argv, env };

  if (isolation.mode === "uid") {
    const assignments = Object.entries(env).map(([k, v]) => `${k}=${v}`);
    return {
      argv: ["sudo", "-n", "-u", isolation.user, "env", "-i", ...assignments, ...argv],
      env: {},
    };
  }

  const flags = Object.entries(env).flatMap(([k, v]) => ["--env", `${k}=${v}`]);
  return {
    argv: [
      isolation.engine,
      "run",
      "--rm",
      // The run directory holds the workspace, the scratch HOME, the brief, and the logs —
      // everything the provider is entitled to. It is the *only* host path mounted: the key
      // lives in the host's ~/.config/rune, and nothing in the container can reach it.
      "--volume",
      `${dir}:${dir}`,
      "--workdir",
      workspace,
      ...flags,
      isolation.image,
      ...argv,
    ],
    env: {},
  };
}
