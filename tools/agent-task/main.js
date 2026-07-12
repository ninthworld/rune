import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { PROVIDERS, actor, repoSlug, runsRoot } from "./config.js";
import { TaskError, claim, release } from "./claim.js";
import { buildBrief } from "./brief.js";
import { diagnose } from "./doctor.js";
import { GitHub, mintToken } from "./github.js";
import { resolveIsolation, scratchHomeFor } from "./isolation.js";
import { DEFAULT_TIMEOUT_MS, runProvider } from "./provider.js";
import { activeRunForIssue, isActive, listRuns, loadRun, removeRun, runDir, transition } from "./runs.js";
import { createWorkspace, ensureMirror } from "./sandbox.js";

const USAGE = `scripts/agent-task — drive one RUNE issue to a reviewable PR (ADR 0016)

  start <issue> --provider <${PROVIDERS.join("|")}> [--allow-ci] [--timeout 45m]
                [--unsafe-same-uid]
                          claim the issue and run the provider in a sandbox
  status [<issue>|<run-id>]  show lifecycle state
  list                    active and recent runs
  release <issue> [--force]  drop the claim; --force takes over a stale one
  cleanup [<issue>|--all]    remove finished run directories
  doctor                  check this machine can run agent tasks

The runner performs every GitHub mutation as \`rune-agent[bot]\`. It never approves and
never merges: a successful run ends at an open, human-reviewable PR.`;

function parseArgs(argv) {
  const positional = [];
  const flags = {};
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      positional.push(arg);
      continue;
    }
    const [name, inline] = arg.slice(2).split("=");
    if (inline !== undefined) flags[name] = inline;
    else if (argv[i + 1] && !argv[i + 1].startsWith("--")) flags[name] = argv[++i];
    else flags[name] = true;
  }
  return { positional, flags };
}

function connect() {
  const { owner, repo } = repoSlug();
  return new GitHub({ owner, repo, token: mintToken() });
}

function issueArg(positional, command) {
  const number = Number(positional[0]);
  if (!Number.isInteger(number) || number <= 0) {
    throw new TaskError(`${command} needs an issue number, e.g. scripts/agent-task ${command} 186`);
  }
  return number;
}

function describe(run) {
  return `${run.run_id}  #${run.issue}  ${run.state.padEnd(10)}  ${run.provider.padEnd(6)}  ${run.title}`;
}

function parseTimeout(value) {
  if (value === undefined) return DEFAULT_TIMEOUT_MS;
  const match = /^(\d+)(s|m|h)?$/.exec(String(value));
  if (!match) throw new TaskError(`--timeout must look like 45m, 2h, or 900s (got ${value})`);
  const scale = { s: 1000, m: 60_000, h: 3_600_000 }[match[2] ?? "m"];
  return Number(match[1]) * scale;
}

async function cmdStart({ positional, flags }) {
  const number = issueArg(positional, "start");
  const provider = flags.provider;
  if (!PROVIDERS.includes(provider)) {
    throw new TaskError(`--provider must be one of ${PROVIDERS.join(", ")} (got ${provider ?? "nothing"})`);
  }
  const timeoutMs = parseTimeout(flags.timeout);

  // Resolved before the claim: a host that cannot contain a provider should fail while the
  // issue is still untouched, not after it has been claimed and labelled.
  let isolation;
  try {
    isolation = resolveIsolation({ unsafeSameUid: flags["unsafe-same-uid"] === true });
  } catch (err) {
    throw new TaskError(err.message);
  }
  if (isolation.mode === "same-uid") {
    process.stderr.write(
      "warning: --unsafe-same-uid — the provider runs as you and can read the rune-agent\n" +
        "         private key. Recorded as isolation=same-uid in the run summary.\n\n",
    );
  }

  const gh = connect();
  const root = runsRoot();
  const issue = await gh.issue(number);

  let run = await claim(gh, {
    issue: number,
    provider,
    allowCi: flags["allow-ci"] === true,
    actor: actor(),
    root,
  });
  run = { ...run, isolation: isolation.mode };

  const dir = runDir(run.run_id, root);
  process.stdout.write(`claimed #${number} — run ${run.run_id}\n  branch    ${run.branch}\n  state     ${dir}\n\n`);

  ensureMirror();
  const workspace = createWorkspace(run, { root });
  scratchHomeFor(run, root);

  // Written to a file *outside* the working copy (so it can never land in the diff) and also
  // handed to the adapter as text, because the CLIs take the prompt as an argument.
  const brief = buildBrief({ issue, run });
  writeFileSync(join(dir, "brief.md"), brief);

  run = transition({ ...run, workspace }, "implementing", root);
  process.stdout.write(`running ${provider} in ${workspace} (isolation: ${isolation.mode}, timeout: ${timeoutMs / 60000}m)\n`);

  const result = await runProvider({ run, workspace, isolation, root, brief, timeoutMs });
  run = transition({ ...run, ...result }, result.outcome, root);

  process.stdout.write(
    [
      "",
      `${provider} exited ${result.exit_code} — run is ${run.state}`,
      `  logs      ${join(dir, "logs", "provider.log")}`,
      `  diff      git -C ${workspace} status`,
      "",
      // Slices 3-4 add diff inspection, the verification gates, publication, and run
      // summaries. Until they land, `start` stops here rather than pretending to have done
      // work it cannot yet do. The claim is held either way, so the run stays resumable.
      "Verifying, committing, and opening the PR are not implemented yet (ADR 0016 slices 3-4).",
      `Release the claim when you are done:  scripts/agent-task release ${number}`,
      "",
    ].join("\n"),
  );
}

async function cmdRelease({ positional, flags }) {
  const number = issueArg(positional, "release");
  await release(connect(), { issue: number, force: flags.force === true, root: runsRoot() });
  process.stdout.write(`released #${number}\n`);
}

function cmdStatus({ positional }) {
  const key = positional[0];
  const runs = key
    ? [loadRun(key) || activeRunForIssue(Number(key))].filter(Boolean)
    : listRuns().filter(isActive);

  if (runs.length === 0) {
    process.stdout.write(key ? `no run found for ${key}\n` : "no active runs\n");
    return;
  }
  for (const run of runs) {
    process.stdout.write(`${describe(run)}\n`);
    for (const event of run.events || []) process.stdout.write(`    ${event.at}  ${event.state}\n`);
  }
}

function cmdList() {
  const runs = listRuns();
  if (runs.length === 0) {
    process.stdout.write("no runs\n");
    return;
  }
  for (const run of runs) process.stdout.write(`${describe(run)}\n`);
}

function cmdCleanup({ positional, flags }) {
  const all = flags.all === true;
  if (!all && positional.length === 0) {
    throw new TaskError("cleanup needs an issue number, a run id, or --all");
  }

  const key = positional[0];
  const candidates = listRuns().filter((run) => all || run.run_id === key || run.issue === Number(key));
  const finished = candidates.filter((run) => !isActive(run));
  const held = candidates.filter(isActive);

  for (const run of finished) {
    removeRun(run.run_id);
    process.stdout.write(`removed ${run.run_id}\n`);
  }
  for (const run of held) {
    // Deleting the directory of a live claim would strand the branch and the labels with
    // nothing on this machine that knows how to release them.
    process.stderr.write(`skipped ${run.run_id}: still active (release #${run.issue} first)\n`);
  }
  if (finished.length === 0 && held.length === 0) process.stdout.write("nothing to clean up\n");
}

function cmdDoctor() {
  const checks = diagnose();
  for (const check of checks) {
    const mark = check.ok ? "ok  " : check.required ? "FAIL" : "warn";
    process.stdout.write(`${mark}  ${check.name.padEnd(26)} ${check.detail}\n`);
  }
  const failed = checks.filter((c) => c.required && !c.ok);
  if (failed.length > 0) {
    throw new TaskError(`${failed.length} required check(s) failed — see docs/agents/workflow.md`);
  }
}

const COMMANDS = {
  start: cmdStart,
  status: cmdStatus,
  list: cmdList,
  release: cmdRelease,
  cleanup: cmdCleanup,
  doctor: cmdDoctor,
};

export async function main(argv) {
  const [command, ...rest] = argv;
  if (!command || command === "help" || command === "--help") {
    process.stdout.write(`${USAGE}\n`);
    return 0;
  }
  const handler = COMMANDS[command];
  if (!handler) {
    process.stderr.write(`unknown command "${command}"\n\n${USAGE}\n`);
    return 2;
  }

  try {
    await handler(parseArgs(rest));
    return 0;
  } catch (err) {
    if (err instanceof TaskError) {
      process.stderr.write(`${err.message}\n`);
      return 1;
    }
    throw err;
  }
}
