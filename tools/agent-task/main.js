import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { AUDIT_BRANCH, observePr, publishSummary } from "./audit.js";
import { LABELS, PROVIDERS, actor, repoSlug, runsRoot } from "./config.js";
import { TaskError, claim, release } from "./claim.js";
import { buildBrief } from "./brief.js";
import { inspect } from "./diff.js";
import { diagnose } from "./doctor.js";
import { GitHub, mintToken } from "./github.js";
import { hasCredential, providerEnv, resolveIsolation, scratchHomeFor } from "./isolation.js";
import { buildPrBody } from "./prbody.js";
import { DEFAULT_TIMEOUT_MS, runProvider } from "./provider.js";
import { commitWork, openDraftPr, pushFromMirror, rebaseOntoMain } from "./publish.js";
import { activeRunForIssue, isActive, listRuns, loadRun, newRunId, removeRun, runDir, saveRun, transition } from "./runs.js";
import { createWorkspace, ensureMirror } from "./sandbox.js";
import { buildSummary } from "./summary.js";
import { GATE_SETS, runGates } from "./verify.js";

const USAGE = `scripts/agent-task — drive one RUNE issue to a reviewable PR (ADR 0016)

  start <issue> --provider <${PROVIDERS.join("|")}> [--allow-ci] [--timeout 45m]
                [--gates verify|check] [--unsafe-same-uid]
                          claim, run the provider, verify, and open a draft PR
  resume <issue> [--rerun-provider] [--allow-ci]
                          re-enter a failed run; the claim and the work are still there
  report <issue|run-id>   re-observe the PR and whether the ADR 0015 review actually ran,
                          and publish a superseding run summary
  status [<issue>|<run-id>]  show lifecycle state
  list                    active and recent runs (⚠️ STALE marks an abandoned claim)
  release <issue> [--force]  drop the claim; --force takes over a stale one
  cleanup [<issue>|--all]    remove finished run directories
  doctor                  check this machine can run agent tasks

The runner performs every GitHub mutation as \`rune-agent[bot]\`. It never approves and
never merges: a successful run ends at an open, human-reviewable PR.
Sanitized run summaries are published to the \`${AUDIT_BRANCH}\` branch (append-only).`;

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

/**
 * A claim whose run stopped heartbeating.
 *
 * The machine that made the claim may have been closed, killed, or rebooted; nothing on GitHub
 * expires. A stale claim is surfaced rather than reclaimed automatically — taking a claim over
 * is `release --force`, and that is a human's call (ADR 0016).
 */
const STALE_AFTER_MS = 4 * 60 * 60 * 1000;

export function isStale(run, now = Date.now()) {
  return isActive(run) && now - Date.parse(run.updated_at ?? run.created_at) > STALE_AFTER_MS;
}

function describe(run) {
  const stale = isStale(run) ? "  ⚠️ STALE" : "";
  return `${run.run_id}  #${run.issue}  ${run.state.padEnd(14)}  ${run.provider.padEnd(6)}  ${run.title}${stale}`;
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
  const gateSet = flags.gates ?? "verify";
  if (!GATE_SETS[gateSet]) {
    throw new TaskError(`--gates must be one of ${Object.keys(GATE_SETS).join(", ")} (got ${gateSet})`);
  }

  // The sandbox replaces HOME, so a provider that was only ever logged in interactively cannot
  // see its own session and will sit asking for `/login` with nobody there to answer. Checked
  // before the claim, like everything else that can be known in advance.
  if (!hasCredential(provider)) {
    throw new TaskError(
      `the ${provider} provider has no credential in the environment.\n\n` +
        `Its interactive login lives under your real HOME, which the sandbox deliberately puts out\n` +
        `of reach — the same isolation that stops a provider reading the rune-agent private key.\n` +
        `A headless run needs a token instead:\n\n` +
        (provider === "claude"
          ? "  claude setup-token          # once; mints a long-lived token for subscription users\n  export CLAUDE_CODE_OAUTH_TOKEN=…\n"
          : provider === "codex"
            ? "  export OPENAI_API_KEY=…\n"
            : "  export RUNE_LOCAL_ENV=VAR1,VAR2   # names of the env vars your harness needs\n"),
    );
  }

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
  run = { ...run, isolation: isolation.mode, gate_set: gateSet };

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
  process.stdout.write(`\n${provider} exited ${result.exit_code} — ${run.state}\n`);

  if (result.outcome !== "implemented") {
    // The claim, the branch, and the diff all survive: a failed run is resumable, not lost.
    await stop({ gh, run, issue, root }, `provider did not finish (${run.state}). Logs: ${join(dir, "logs", "provider.log")}\nResume with: scripts/agent-task resume ${number}`);
  }

  await publishRun({ gh, run, issue, workspace, isolation, root, number });
}

/**
 * Ends a run at a failure: record it, publish the summary, and say what to do next.
 *
 * The claim, the branch, and the work all survive — a failed run is resumable, not lost — but
 * it is *terminal for this attempt*, and #200 only sees runs that end. A run that dies without
 * a summary is a run that never happened as far as the report is concerned, which is exactly
 * how "our agents mostly work" becomes an unfalsifiable claim.
 */
async function stop({ gh, run, issue, root }, message) {
  await recordSummary(gh, run, { issue });
  throw new TaskError(message);
}

/** Telemetry must never be the reason a run fails: a summary that cannot be published warns. */
async function recordSummary(gh, run, { issue, pr = null, review = null }) {
  try {
    const record = await publishSummary(gh, buildSummary(run, { issue, pr, review }));
    process.stdout.write(`summary: ${AUDIT_BRANCH}:${record.path}\n`);
  } catch (err) {
    process.stderr.write(`warning: could not publish the run summary (${err.message})\n`);
  }
}

/**
 * Everything between "the provider stopped" and "a human has something to review".
 *
 * None of it trusts the provider's account of what it did: the diff is inspected, the gates
 * are run, and the commit, the push, and the PR are the runner's own.
 */
async function publishRun({ gh, run: claimed, issue, workspace, isolation, root, number }) {
  let run = claimed;
  const dir = runDir(run.run_id, root);
  const env = providerEnv({ provider: run.provider, workspace, run, root, scratchHome: join(dir, "home") });

  const found = inspect(workspace, { allowCi: run.allow_ci, baseSha: run.base_sha });
  run = { ...run, files: found.files, ci_paths: found.ciPaths };
  if (!found.ok) {
    const first = found.violations[0];
    run = transition(run, first.outcome, root);
    await stop(
      { gh, run, issue, root },
      `${first.outcome}: ${found.violations.map((v) => v.detail).join("\n")}\n\n` +
        `The work is preserved in ${workspace} and the claim is held.`,
    );
  }
  process.stdout.write(`diff ok — ${found.files.length} file(s)${found.ciPaths.length ? `, ${found.ciPaths.length} CI-governance` : ""}\n`);

  commitWork(workspace, { issue });

  const set = run.gate_set ?? "verify";
  process.stdout.write(`verifying (${set})…\n`);
  let verification = await runGates({ run, workspace, isolation, root, env, set });
  if (!verification.ok) {
    const failed = verification.gates.find((g) => !g.ok);
    run = transition({ ...run, gates: verification.gates }, "verification_failed", root);
    await stop(
      { gh, run, issue, root },
      `verification failed at gate ${failed.gate}. Logs: ${join(dir, "logs", "verify.log")}\n` +
        `Fix it in ${workspace} and \`resume ${number}\`, or release the claim.`,
    );
  }

  // `main` requires branches to be up to date, and a run takes long enough to go stale.
  let rebase;
  try {
    rebase = rebaseOntoMain(workspace);
  } catch (err) {
    run = transition(run, err.outcome ?? "rebase_conflict", root);
    await stop({ gh, run, issue, root }, `${err.message}\nResolve it in ${workspace} and \`resume ${number}\`.`);
  }
  if (rebase.moved) {
    process.stdout.write("main moved — rebased; re-verifying against the new base…\n");
    verification = await runGates({ run, workspace, isolation, root, env, set });
    if (!verification.ok) {
      run = transition({ ...run, gates: verification.gates }, "verification_failed", root);
      await stop({ gh, run, issue, root }, "verification failed after rebasing onto current main — the change conflicts semantically with new work on main.");
    }
  }
  run = { ...run, gates: verification.gates, head_sha: rebase.head };

  const remoteSha = await gh.branchSha(run.branch);
  pushFromMirror({ workspace, branch: run.branch, remoteSha });

  const body = buildPrBody({
    issue,
    run,
    gates: verification.gates,
    files: found.files,
    ciPaths: found.ciPaths,
    providerUsage: run.provider_usage,
  });
  const pr = openDraftPr({ branch: run.branch, title: issue.title, body });

  // `agent` and `ci-change` belong on the PR (the thing being reviewed); the lifecycle label
  // belongs on the issue. `status:review` is set only now, so the label always means "there is
  // something to review", never "a run intends to produce something".
  await gh.addLabels(pr.number, found.ciPaths.length > 0 ? ["agent", "ci-change"] : ["agent"]);
  await gh.removeLabel(number, LABELS.inProgress);
  await gh.addLabels(number, [LABELS.review]);

  run = transition({ ...run, pr: pr.number, pr_url: pr.url }, "review", root);
  process.stdout.write(`\ndraft PR opened: ${pr.url}\n#${number} is now ${LABELS.review}. A human reviews and merges — always.\n`);

  // The ADR 0015 review has not run yet — it was triggered by the push a moment ago. The summary
  // records that it has not been observed; `report` supersedes it once there is something to see.
  await recordSummary(gh, run, { issue, pr: { number: pr.number, author: null, draft: true } });
  process.stdout.write(`Once CI settles, record what actually ran:  scripts/agent-task report ${number}\n`);
}

/**
 * Re-enters a failed run.
 *
 * The claim, the branch, and the workspace are all still there, so resuming picks up whatever is
 * in the working copy *now* — which may be what the provider left, or what a human fixed by hand
 * after reading the gate output. `--rerun-provider` hands it back to the provider first.
 *
 * Resumption is recorded (`resume_of`), so #200 can tell "worked first time" from "worked on the
 * third attempt" instead of counting both as a success.
 */
async function cmdResume({ positional, flags }) {
  const number = issueArg(positional, "resume");
  const root = runsRoot();
  const previous = activeRunForIssue(number, root);

  if (!previous) throw new TaskError(`#${number} has no active run on this machine to resume.`);
  if (previous.state === "review") throw new TaskError(`#${number} already has a PR (${previous.pr_url}). Nothing to resume.`);
  if (!existsSync(previous.workspace ?? "")) {
    throw new TaskError(`the workspace for run ${previous.run_id} is gone (${previous.workspace}).\nRelease #${number} and start again.`);
  }

  const gh = connect();
  const issue = await gh.issue(number);
  const isolation = resolveIsolation({ unsafeSameUid: flags["unsafe-same-uid"] === true });

  let run = saveRun(
    {
      ...previous,
      run_id: newRunId(number),
      resume_of: previous.run_id,
      // A ci_change_refused run is resumed *with* the flag, or it is refused again.
      allow_ci: flags["allow-ci"] === true || previous.allow_ci,
      state: "implementing",
      events: [...(previous.events ?? []), { state: "resumed", at: new Date().toISOString() }],
    },
    root,
  );
  // The old run is closed out so nothing thinks two runs hold the same claim.
  transition(previous, "resumed", root);
  process.stdout.write(`resuming #${number} as ${run.run_id} (was ${previous.run_id}, ${previous.state})\n`);

  if (flags["rerun-provider"] === true) {
    const dir = runDir(run.run_id, root);
    mkdirSync(join(dir, "logs"), { recursive: true });
    scratchHomeFor(run, root);
    const brief = buildBrief({ issue, run });
    writeFileSync(join(dir, "brief.md"), brief);

    const result = await runProvider({ run, workspace: run.workspace, isolation, root, brief, timeoutMs: parseTimeout(flags.timeout) });
    run = transition({ ...run, ...result }, result.outcome, root);
    if (result.outcome !== "implemented") {
      await stop({ gh, run, issue, root }, `provider did not finish again (${run.state}).`);
    }
  }

  await publishRun({ gh, run, issue, workspace: run.workspace, isolation, root, number });
}

/**
 * Re-observes a finished run and publishes a superseding summary.
 *
 * At the moment the PR opens, CI has not run yet — so the first summary cannot know who authored
 * the PR or whether the ADR 0015 review actually happened. Rather than guess, or block the run
 * waiting for CI, the correction mechanism does the job it exists for.
 */
async function cmdReport({ positional }) {
  const key = positional[0];
  if (!key) throw new TaskError("report needs an issue number or a run id");

  const root = runsRoot();
  const run = loadRun(key, root) ?? activeRunForIssue(Number(key), root) ?? listRuns(root).find((r) => r.issue === Number(key) && r.pr);
  if (!run) throw new TaskError(`no run found for ${key}`);
  if (!run.pr) throw new TaskError(`run ${run.run_id} never opened a PR — there is nothing further to observe.`);

  const gh = connect();
  const observed = await observePr(gh, run.pr);
  const issue = await gh.issue(run.issue);

  const summary = buildSummary({ ...run, supersedes: run.run_id }, { issue, pr: observed.pr, review: observed.review });
  const record = await publishSummary(gh, summary);

  process.stdout.write(
    [
      `PR #${observed.pr.number} by ${observed.pr.author}${observed.pr.draft ? " (draft)" : ""}`,
      observed.review.ran
        ? `ADR 0015 review: ran, ${observed.review.conclusion}`
        : "ADR 0015 review: DID NOT RUN — it is not a required check, so nothing else would have told you",
      `summary: ${AUDIT_BRANCH}:${record.path} (supersedes ${run.run_id})`,
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
  resume: cmdResume,
  report: cmdReport,
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
