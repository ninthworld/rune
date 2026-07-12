import { PROVIDERS, actor, repoSlug, runsRoot } from "./config.js";
import { TaskError, claim, release } from "./claim.js";
import { diagnose } from "./doctor.js";
import { GitHub, mintToken } from "./github.js";
import { activeRunForIssue, isActive, listRuns, loadRun, removeRun, runDir } from "./runs.js";

const USAGE = `scripts/agent-task — drive one RUNE issue to a reviewable PR (ADR 0016)

  start <issue> --provider <${PROVIDERS.join("|")}> [--allow-ci]
                          claim the issue and prepare a run
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

async function cmdStart({ positional, flags }) {
  const number = issueArg(positional, "start");
  const provider = flags.provider;
  if (!PROVIDERS.includes(provider)) {
    throw new TaskError(`--provider must be one of ${PROVIDERS.join(", ")} (got ${provider ?? "nothing"})`);
  }

  const run = await claim(connect(), {
    issue: number,
    provider,
    allowCi: flags["allow-ci"] === true,
    actor: actor(),
    root: runsRoot(),
  });

  process.stdout.write(
    [
      `claimed #${number} — run ${run.run_id}`,
      `  branch    ${run.branch} (at ${run.base_sha.slice(0, 7)})`,
      `  provider  ${run.provider}`,
      `  state     ${runDir(run.run_id)}`,
      "",
      // Slices 2-4 of ADR 0016 add the sandbox, the provider adapters, the verification
      // gates, and publication. Until they land, `start` stops at the claim rather than
      // pretending to have done work it cannot yet do.
      "The claim is held. Running the provider, verifying, and opening the PR are not",
      "implemented yet (ADR 0016 slices 2-4). Release the claim when you are done:",
      `  scripts/agent-task release ${number}`,
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
