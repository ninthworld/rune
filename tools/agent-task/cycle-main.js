import { TaskError } from "./claim.js";
import { repoSlug } from "./config.js";
import { collectEvidence, validateBundle } from "./cycle-evidence.js";
import { cycleDir, cycleWorkspace, listCycles, loadBundle, newCycleId, saveBundle } from "./cycle-state.js";
import { git } from "./git.js";
import { GitHub, mintToken } from "./github.js";
import { parseArgs } from "./main.js";
import { ensureMirror } from "./sandbox.js";
import { GATE_SETS } from "./verify.js";

const USAGE = `scripts/agent-cycle — milestone stewardship (ADR 0017)

  collect <milestone> [--gates verify|check] [--base <sha>]
                          snapshot one commit and build the Evidence Bundle for a
                          milestone: exit criteria verbatim, issues/PRs, the required
                          checks on each merged PR **and** a fresh gate run against the
                          audited commit, test counts, rules coverage, ADR/protocol
                          state, TODO/stub locations, documented gaps
  show <cycle-id>         summarize a collected bundle
  list                    cycles collected on this machine

Evidence collection **reads only**: no issue, label, comment, branch, or PR is touched,
and no model is invoked. Bundles live outside the repository, under
\`$XDG_STATE_HOME/rune/cycles/<cycle-id>/\`, so they can never land in a diff.

The audit, the closeout gate, planning, application, and the roadmap PR are the rest of
ADR 0017 and are not implemented yet (#225–#228). Until they are, a milestone is still
reconciled by a human — reading, now, the same evidence the cycle will eventually hand
to the Auditor.`;

/**
 * The commit the whole cycle is pinned to.
 *
 * `origin/main` as the mirror sees it, never the local checkout's HEAD: a bundle has to name
 * a commit that exists on GitHub for anyone else to be able to reproduce it, and a
 * maintainer's local `main` can be behind, ahead, or on an entirely different branch.
 */
function baseSha(mirror, flag) {
  if (typeof flag === "string") return git(["rev-parse", flag], { cwd: mirror });
  return git(["rev-parse", "refs/heads/main"], { cwd: mirror });
}

async function cmdCollect({ positional, flags }, { out }) {
  const milestone = positional[0];
  if (!milestone) throw new TaskError("collect needs a milestone, e.g. scripts/agent-cycle collect M3");

  const gateSet = flags.gates ?? "verify";
  if (!GATE_SETS[gateSet]) {
    throw new TaskError(`--gates must be one of ${Object.keys(GATE_SETS).join(", ")} (got ${gateSet})`);
  }

  const { owner, repo } = repoSlug();
  const gh = new GitHub({ owner, repo, token: mintToken() });

  const mirror = ensureMirror();
  const sha = baseSha(mirror, flags.base);
  const cycleId = newCycleId(milestone);

  out(`cycle ${cycleId}`);
  out(`  base ${sha}`);
  out("  checking out the audited commit…");
  const workspace = cycleWorkspace(cycleId, sha, { mirror });

  out(`  collecting evidence (running the ${gateSet} gates against it — this takes a while)…`);
  const bundle = await collectEvidence({ gh, workspace, milestone, cycleId, baseSha: sha, gateSet });
  const path = saveBundle(bundle);

  const { ok, problems } = validateBundle(bundle);
  summarize(bundle, out);
  out(`\n  ${path}`);

  if (!ok) {
    out(`\n  ⚠️  the bundle is not schema-valid and no later stage may read it:`);
    for (const problem of problems) out(`      ${problem.path}: ${problem.problem}`);
    return 1;
  }
  return 0;
}

function summarize(bundle, out) {
  const criteria = bundle.exit_criteria;
  const ticked = criteria.filter((c) => c.checked).length;
  const open = bundle.issues.filter((i) => i.state === "open");
  const merged = bundle.prs.filter((p) => p.merged);
  const fresh = bundle.ci.fresh_run;
  const red = fresh.gates.filter((g) => !g.ok).map((g) => g.gate);

  out(`\n  ${bundle.milestone}`);
  out(`  criteria      ${criteria.length} (${ticked} ticked in the roadmap — a claim, not a verdict)`);
  out(`  issues        ${bundle.issues.length} (${open.length} open)`);
  out(`  merged PRs    ${merged.length}`);
  out(`  fresh gates   ${fresh.ok ? "green" : `RED: ${red.join(", ")}`} (${fresh.base_gate_set})`);
  for (const suite of bundle.tests.suites) out(`  tests         ${suite.suite}: ${suite.passed} passed, ${suite.failed} failed`);
  for (const missing of bundle.tests.missing_counts) {
    out(`  tests         ⚠️  ${missing.suite} ran in the ${missing.gate} gate but its counts could not be read`);
  }
  out(`  coverage rows ${bundle.rules_coverage.length} in the milestone's CR scope`);
  out(`  stubs         ${bundle.todos_and_stubs.matches.length} TODO/unimplemented sites in scope`);
  out(`  known gaps    ${bundle.documented_gaps.partial_notes.length} criteria already marked partial`);
}

function cmdShow({ positional }, { out }) {
  const cycleId = positional[0];
  if (!cycleId) throw new TaskError("show needs a cycle id (scripts/agent-cycle list)");
  const bundle = loadBundle(cycleId);
  if (!bundle) throw new TaskError(`no cycle ${cycleId} on this machine`);

  out(`cycle ${bundle.cycle_id}`);
  out(`  base ${bundle.base_commit_sha}  collected ${bundle.collected_at}`);
  summarize(bundle, out);

  const { ok, problems } = validateBundle(bundle);
  out(`\n  schema        ${ok ? "valid" : `INVALID (${problems.length} problems)`}`);
  out(`  ${cycleDir(bundle.cycle_id)}`);
  return ok ? 0 : 1;
}

function cmdList(_args, { out }) {
  const cycles = listCycles();
  if (cycles.length === 0) {
    out("no cycles collected on this machine");
    return 0;
  }
  for (const bundle of cycles) {
    out(`${bundle.cycle_id}  ${bundle.milestone}  ${bundle.base_commit_sha.slice(0, 8)}  ${bundle.collected_at}`);
  }
  return 0;
}

const COMMANDS = { collect: cmdCollect, show: cmdShow, list: cmdList };

export async function main(argv, { out = console.log, err = console.error } = {}) {
  const [command, ...rest] = argv;
  if (!command || command === "help" || command === "--help") {
    out(USAGE);
    return command ? 0 : 1;
  }

  const handler = COMMANDS[command];
  if (!handler) {
    err(`unknown command "${command}"\n\n${USAGE}`);
    return 1;
  }

  try {
    return await handler(parseArgs(rest), { out, err });
  } catch (error) {
    if (error instanceof TaskError) {
      err(`✗ ${error.message}`);
      return 1;
    }
    throw error;
  }
}
