import { spawn } from "node:child_process";
import { createWriteStream, readFileSync } from "node:fs";
import { join } from "node:path";

import { adapterFor } from "./adapters.js";
import { providerEnv, wrap } from "./isolation.js";
import { redactor } from "./redact.js";
import { runDir } from "./runs.js";

export const DEFAULT_TIMEOUT_MS = 45 * 60 * 1000;
const GRACE_MS = 30 * 1000;

/** Kills the provider's whole process group — a provider that spawned children must not outlive its run. */
function terminate(child, signal) {
  try {
    process.kill(-child.pid, signal);
  } catch {
    // Already dead, or never got a group. Nothing to do.
  }
}

/**
 * Runs one provider to completion inside the sandbox.
 *
 * Outcome is observed, never reported: the exit code is the only contract signal (0 means
 * "I am done, inspect the tree"). `result.json` may carry provider-reported usage, which is
 * advisory and is labelled as such — it is never allowed to decide whether a run succeeded.
 */
export async function runProvider({
  run,
  workspace,
  isolation,
  root,
  brief,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  graceMs = GRACE_MS,
  secrets = [],
  spawnImpl = spawn,
}) {
  const dir = runDir(run.run_id, root);
  const env = providerEnv({ provider: run.provider, workspace, run, root, scratchHome: join(dir, "home") });
  const { argv, env: spawnEnv } = wrap(adapterFor(run.provider).argv(brief), { isolation, env, workspace, dir });

  const log = createWriteStream(join(dir, "logs", "provider.log"), { flags: "a" });
  const logClosed = new Promise((resolve) => log.on("close", resolve));
  const sink = redactor((line) => log.write(line), secrets);

  const child = spawnImpl(argv[0], argv.slice(1), {
    cwd: workspace,
    env: spawnEnv,
    // Its own process group, so a timeout can kill the provider *and* everything it spawned.
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });

  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => sink.push(chunk));
  child.stderr.on("data", (chunk) => sink.push(chunk));

  let outcome = null;
  const deadline = setTimeout(() => {
    outcome = "provider_timeout";
    terminate(child, "SIGTERM");
    setTimeout(() => terminate(child, "SIGKILL"), graceMs).unref();
  }, timeoutMs);

  const cancel = () => {
    outcome = "cancelled";
    terminate(child, "SIGTERM");
  };
  process.on("SIGINT", cancel);
  process.on("SIGTERM", cancel);

  try {
    const code = await new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("close", resolve);
    });
    sink.flush();
    log.end();
    // The log must be on disk before the run is reported: a caller that transitions the run
    // (or a test that tears the directory down) the instant this resolves would otherwise
    // race the stream's final flush.
    await logClosed;

    return {
      outcome: outcome ?? (code === 0 ? "implemented" : "provider_failed"),
      exit_code: code,
      // Provider-reported, therefore advisory and non-comparable across providers (ADR 0016).
      provider_usage: readResult(dir),
    };
  } finally {
    clearTimeout(deadline);
    process.off("SIGINT", cancel);
    process.off("SIGTERM", cancel);
  }
}

function readResult(dir) {
  try {
    return JSON.parse(readFileSync(join(dir, "result.json"), "utf8"));
  } catch {
    return null;
  }
}
