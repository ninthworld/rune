export const AUDIT_BRANCH = "agent-runs";

/**
 * Publishes a run summary to the audit branch.
 *
 * The surface is an **orphan branch** in this repository: durable and auditable (#200 must not
 * depend on undocumented files in one maintainer's checkout), off `main` (telemetry never
 * touches the reviewed history and never triggers CI), and one-file-per-run, so concurrent runs
 * cannot collide on content — only on the ref, which is retried.
 *
 * Records are **append-only**. A correction is a *new* record naming what it `supersedes`,
 * never a rewrite, so the audit trail cannot be quietly edited after the fact.
 */
export async function publishSummary(gh, summary, { branch = AUDIT_BRANCH, attempts = 3 } = {}) {
  const path = `runs/${summary.issue}/${summary.run_id}${summary.supersedes ? `.${stamp(summary)}` : ""}.json`;
  const content = `${JSON.stringify(summary, null, 2)}\n`;

  for (let attempt = 1; ; attempt++) {
    try {
      return await commitRecord(gh, { branch, path, content, summary });
    } catch (err) {
      // Another run updated the ref between our read and our write. Re-read and re-apply: the
      // record is a new file, so there is nothing to merge.
      const contended = err.status === 409 || err.status === 422;
      if (!contended || attempt >= attempts) throw err;
    }
  }
}

function stamp(summary) {
  return summary.terminal_at.replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
}

async function commitRecord(gh, { branch, path, content, summary }) {
  const blob = await gh.request("POST", gh.repoPath("/git/blobs"), { content, encoding: "utf-8" });
  const head = await gh.branchSha(branch);

  const message = `run: #${summary.issue} ${summary.run_id} ${summary.terminal_outcome}`;

  if (!head) {
    // First record ever: an orphan commit with no parents, so the audit branch shares no
    // history with `main` and can never be merged into it by accident.
    const tree = await gh.request("POST", gh.repoPath("/git/trees"), {
      tree: [{ path, mode: "100644", type: "blob", sha: blob.sha }],
    });
    const commit = await gh.request("POST", gh.repoPath("/git/commits"), { message, tree: tree.sha, parents: [] });
    await gh.request("POST", gh.repoPath("/git/refs"), { ref: `refs/heads/${branch}`, sha: commit.sha });
    return { path, commit: commit.sha, created: true };
  }

  const parent = await gh.request("GET", gh.repoPath(`/git/commits/${head}`));
  const tree = await gh.request("POST", gh.repoPath("/git/trees"), {
    base_tree: parent.tree.sha,
    tree: [{ path, mode: "100644", type: "blob", sha: blob.sha }],
  });
  const commit = await gh.request("POST", gh.repoPath("/git/commits"), { message, tree: tree.sha, parents: [head] });

  // Never forced: a fast-forward-only update is what makes the branch append-only in fact and
  // not merely by convention.
  await gh.request("PATCH", gh.repoPath(`/git/refs/heads/${branch}`), { sha: commit.sha, force: false });
  return { path, commit: commit.sha, created: false };
}

/**
 * Observes the PR and whether the ADR 0015 review actually ran.
 *
 * Both are runner-observed for a reason. The PR's author decides whether a human other than the
 * author can approve it at all (#205/#206), and a *skipped* review looks exactly like a passing
 * one unless somebody goes and reads the check runs.
 *
 * The check is now `AI Review` (#243), which replaced the interim `claude-review`. It is
 * required-to-complete, so a skipped review can no longer merge — but this observation is still
 * worth making rather than assuming: "the check is required" is a repository setting, and the
 * whole reason this field exists is that settings and beliefs about settings drift apart.
 */
export async function observePr(gh, prNumber, { reviewCheck = "AI Review" } = {}) {
  const pr = await gh.request("GET", gh.repoPath(`/pulls/${prNumber}`));
  const checks = await gh.request("GET", gh.repoPath(`/commits/${pr.head.sha}/check-runs`));

  const review = (checks.check_runs ?? []).find((c) => c.name === reviewCheck);
  return {
    pr: { number: pr.number, author: pr.user?.login ?? null, draft: pr.draft },
    review: {
      observed: true,
      ran: Boolean(review) && review.status === "completed",
      conclusion: review?.conclusion ?? null,
    },
  };
}
