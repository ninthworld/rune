import { LABELS } from "./config.js";
import { branchName, declaredDependencies, preflight } from "./preflight.js";
import { RUN_SCHEMA_VERSION, activeRunForIssue, newRunId, saveRun, transition } from "./runs.js";

export class TaskError extends Error {}

async function openDependenciesOf(gh, issue) {
  const declared = declaredDependencies(issue.body).filter((n) => n !== issue.number);
  const states = await Promise.all(
    declared.map(async (n) => ({ n, issue: await gh.issue(n).catch(() => null) })),
  );
  return states.filter(({ issue: dep }) => dep && dep.state === "open").map(({ n }) => n);
}

/**
 * Claims an issue: preflight, then atomically create its branch.
 *
 * Every check runs before the first mutation, so a rejected task leaves no trace on
 * GitHub. The branch creation is the lock (ADR 0016) — losing it is an ordinary outcome,
 * not an error, because another runner legitimately got there first.
 */
export async function claim(gh, { issue: number, provider, allowCi = false, actor, root, now = new Date() }) {
  const existing = activeRunForIssue(number, root);
  if (existing) {
    throw new TaskError(
      `#${number} already has an active run on this machine (${existing.run_id}, ${existing.state}).\n` +
        `Resume it, or drop the claim with: scripts/agent-task release ${number}`,
    );
  }

  const issue = await gh.issue(number);
  const check = preflight(issue, await openDependenciesOf(gh, issue));
  for (const warning of check.warnings) process.stderr.write(`warning: ${warning}\n`);
  if (!check.ok) {
    throw new TaskError(`#${number} is not claimable:\n  - ${check.errors.join("\n  - ")}`);
  }

  const branch = branchName(issue);
  const baseSha = await gh.branchSha("main");

  // Recorded before the branch exists: a crash between here and the transition below
  // leaves a record pointing at a branch that may exist, which `release` can clean up.
  // The reverse order would leave an orphan branch nothing knows about.
  let run = saveRun(
    {
      schema_version: RUN_SCHEMA_VERSION,
      run_id: newRunId(number, now),
      issue: number,
      title: issue.title,
      provider,
      allow_ci: allowCi,
      branch,
      base_sha: baseSha,
      actor,
      state: "claiming",
      events: [{ state: "claiming", at: now.toISOString() }],
      created_at: now.toISOString(),
    },
    root,
  );

  if (!(await gh.createBranch(branch, baseSha))) {
    run = transition(run, "claim_lost", root);
    throw new TaskError(
      `#${number} was claimed by another runner — ${branch} already exists.\n` +
        `Local run ${run.run_id} recorded as claim_lost; nothing on GitHub was changed.`,
    );
  }

  run = transition(run, "claimed", root);

  await gh.removeLabel(number, LABELS.ready);
  await gh.addLabels(number, [LABELS.inProgress]);
  await gh.comment(
    number,
    [
      `🤖 Claimed by \`rune-agent[bot]\` — run \`${run.run_id}\`.`,
      "",
      `| | |`,
      `|---|---|`,
      `| Branch | \`${branch}\` (created at \`${baseSha.slice(0, 7)}\`) |`,
      `| Provider | \`${provider}\` |`,
      `| Started by | \`${actor}\` |`,
      `| Claimed at | ${now.toISOString()} |`,
      "",
      `Release the claim with \`scripts/agent-task release ${number}\`.`,
    ].join("\n"),
  );

  return run;
}

/**
 * Drops a claim: delete the branch, move the issue back to `status:ready`.
 *
 * Works from GitHub state alone, so it can recover a claim made on a machine whose local
 * run record is gone. It refuses to discard a branch that has commits on it, or one this
 * machine does not own, unless forced — `--force` is the documented human-approved
 * takeover of a stale claim.
 */
export async function release(gh, { issue: number, force = false, root }) {
  const issue = await gh.issue(number);
  const branch = branchName(issue);
  const run = activeRunForIssue(number, root);

  if (!run && !force) {
    throw new TaskError(
      `#${number} has no active run on this machine — it may be claimed elsewhere.\n` +
        `Take the claim over with: scripts/agent-task release ${number} --force`,
    );
  }

  const head = await gh.branchSha(branch);
  if (head) {
    const cmp = await gh.request("GET", gh.repoPath(`/compare/main...${encodeURIComponent(branch)}`));
    if (cmp.ahead_by > 0 && !force) {
      throw new TaskError(
        `${branch} has ${cmp.ahead_by} commit(s) that would be destroyed by releasing it.\n` +
          `Discard them anyway with: scripts/agent-task release ${number} --force`,
      );
    }
    await gh.deleteBranch(branch);
  }

  await gh.removeLabel(number, LABELS.inProgress);
  await gh.addLabels(number, [LABELS.ready]);
  await gh.comment(
    number,
    `🤖 Claim released${run ? ` (run \`${run.run_id}\`)` : ""}${force ? " — forced takeover" : ""}. ` +
      `Branch \`${branch}\` deleted; the issue is back to \`${LABELS.ready}\`.`,
  );

  return run ? transition(run, "released", root) : null;
}
