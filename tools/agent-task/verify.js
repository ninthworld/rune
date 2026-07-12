import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import { join } from "node:path";

import { wrap } from "./isolation.js";
import { redactor } from "./redact.js";
import { runDir } from "./runs.js";

/**
 * The verification contract from #184, split so each result maps to the CI check it mirrors.
 *
 * `make verify` as one command would say only pass/fail; the run summary needs to say *which*
 * gate failed, so the targets are run individually and timed.
 */
export const GATES = [
  { name: "Engine", targets: ["engine-lint", "engine-test"] },
  { name: "Client", targets: ["client-audit", "client-check", "runner-test"] },
  { name: "E2E", targets: ["e2e-browser", "e2e"] },
  { name: "cargo-deny", targets: ["deny"] },
];

/** `check` is the fast inner loop; `verify` is the full required surface. */
export const GATE_SETS = {
  check: ["Engine", "Client"],
  verify: ["Engine", "Client", "E2E", "cargo-deny"],
};

/**
 * Runs the gates in the sandbox, under the provider's isolation.
 *
 * Verification executes provider-controlled code by construction — a doctored `Makefile` or a
 * malicious `build.rs` runs here. That is exactly why it runs inside the boundary, with no
 * credential in reach, rather than in the runner's own context (ADR 0016).
 */
export async function runGates({ run, workspace, isolation, root, env, set = "verify", secrets = [], spawnImpl = spawn }) {
  const wanted = GATE_SETS[set] ?? GATE_SETS.verify;
  const log = createWriteStream(join(runDir(run.run_id, root), "logs", "verify.log"), { flags: "a" });
  const logClosed = new Promise((resolve) => log.on("close", resolve));
  const sink = redactor((line) => log.write(line), secrets);

  const results = [];
  try {
    for (const gate of GATES.filter((g) => wanted.includes(g.name))) {
      const started = Date.now();
      sink.push(`\n=== ${gate.name}: make ${gate.targets.join(" ")}\n`);

      const { argv, env: spawnEnv } = wrap(["make", ...gate.targets], {
        isolation,
        env,
        workspace,
        dir: runDir(run.run_id, root),
      });

      const code = await new Promise((resolve, reject) => {
        const child = spawnImpl(argv[0], argv.slice(1), {
          cwd: workspace,
          env: spawnEnv,
          detached: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
        child.stdout.setEncoding("utf8");
        child.stderr.setEncoding("utf8");
        child.stdout.on("data", (c) => sink.push(c));
        child.stderr.on("data", (c) => sink.push(c));
        child.on("error", reject);
        child.on("close", resolve);
      });

      results.push({ gate: gate.name, ok: code === 0, exit_code: code, duration_ms: Date.now() - started });
      // Fail fast: a red Engine makes the rest of the surface uninformative, and the run has to
      // go back to the provider either way.
      if (code !== 0) break;
    }
    return { gates: results, ok: results.length > 0 && results.every((r) => r.ok) };
  } finally {
    sink.flush();
    log.end();
    await logClosed;
  }
}
