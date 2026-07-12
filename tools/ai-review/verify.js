/**
 * The gate on the TRUSTED stage's front door.
 *
 * The review workflow runs on `workflow_run`, in the base-repository context, holding the model
 * credential and a `pull-requests: write` token. Everything it is about to consume was produced
 * by a job that ran attacker-adjacent. So before any of it is used, this module answers one
 * question: *is this artifact the one our own prepare workflow produced, for a pull request that
 * actually exists, on a head SHA we have not already reviewed?*
 *
 * Each check below closes a specific way of lying to the trusted stage:
 *
 *   - **repository / event / workflow identity** — a `workflow_run` payload names its source. A
 *     run of some *other* workflow, or of a fork's copy of ours, must not be able to hand us an
 *     artifact and have it treated as prepared input.
 *   - **conclusion** — a prepare run that failed produced nothing we should reason about.
 *   - **PR association and SHA agreement** — the manifest says which PR and which head it
 *     describes; the workflow_run payload and the live PR must agree. Otherwise a prepare run on
 *     a benign PR could be replayed to publish a review onto a different PR.
 *   - **hash and size** — the manifest hashes the input; we re-hash it. This does not make the
 *     content *trustworthy* (prepare read hostile files), it makes it *unaltered between the two
 *     jobs*, which is a different and necessary property.
 *   - **already reviewed** — one review per head SHA. This is the cost ceiling and the
 *     idempotency guarantee, and it is why a rerun is free.
 *
 * None of this makes the diff safe to *execute*. Nothing ever executes it. It makes the diff
 * safe to *read*, which is all the reviewer does with it.
 */

import { createHash } from "node:crypto";

import { CAPS, CHECK_NAME, PREPARE_WORKFLOW, REVIEWER_VERSION, SCHEMA_VERSION } from "./config.js";

export class ArtifactRejected extends Error {}

const sha256 = (text) => createHash("sha256").update(text).digest("hex");

/**
 * Validates the `workflow_run` event before we even look for an artifact.
 *
 * @param {object} event  the `github.event.workflow_run` payload
 * @param {string} repository  `owner/name` of the repository we are running in
 */
export function verifyWorkflowRun(event, repository) {
  if (!event || typeof event !== "object") throw new ArtifactRejected("no workflow_run payload");

  if (event.name !== PREPARE_WORKFLOW) {
    throw new ArtifactRejected(`triggered by workflow ${JSON.stringify(event.name)}, not ${JSON.stringify(PREPARE_WORKFLOW)}`);
  }
  if (event.event !== "pull_request") {
    throw new ArtifactRejected(`prepare run was triggered by ${JSON.stringify(event.event)}, not pull_request`);
  }
  if (event.conclusion !== "success") {
    throw new ArtifactRejected(`prepare run concluded ${JSON.stringify(event.conclusion)}`);
  }
  // The run must belong to THIS repository. A fork running its own copy of the prepare workflow
  // produces a workflow_run in the fork, not here — but this is the check that says so out loud,
  // rather than relying on that being true forever.
  if (event.repository?.full_name !== repository) {
    throw new ArtifactRejected(
      `prepare run belongs to ${JSON.stringify(event.repository?.full_name)}, not ${JSON.stringify(repository)}`,
    );
  }
  if (!Array.isArray(event.pull_requests) || event.pull_requests.length === 0) {
    // A fork PR's workflow_run carries an empty `pull_requests` array — the association has to
    // be recovered from the head SHA instead, which the caller does against the live API.
    return { runId: String(event.id), headSha: event.head_sha, prNumber: null };
  }
  return { runId: String(event.id), headSha: event.head_sha, prNumber: event.pull_requests[0].number };
}

/** Re-hashes the artifact and checks it says what the run says. */
export function verifyArtifact({ manifest, body, run, repository, pr }) {
  if (manifest?.schema_version !== SCHEMA_VERSION) {
    throw new ArtifactRejected(`manifest schema_version ${manifest?.schema_version}, expected ${SCHEMA_VERSION}`);
  }
  if (manifest.reviewer_version !== REVIEWER_VERSION) {
    // Not fatal in principle, but a mismatch means the prepare job ran a different commit of
    // this tool than the review job did. Refusing is the honest response: we do not know which
    // side's caps and schema the artifact reflects.
    throw new ArtifactRejected(
      `artifact was prepared by reviewer ${manifest.reviewer_version}, this stage is ${REVIEWER_VERSION}`,
    );
  }
  if (manifest.repository !== repository) {
    throw new ArtifactRejected(`manifest names repository ${JSON.stringify(manifest.repository)}, not ${repository}`);
  }
  if (String(manifest.prepare_run_id) !== String(run.runId)) {
    throw new ArtifactRejected(`manifest names prepare run ${manifest.prepare_run_id}, not ${run.runId}`);
  }
  if (manifest.head_sha !== run.headSha) {
    throw new ArtifactRejected(`manifest head ${manifest.head_sha} != workflow_run head ${run.headSha}`);
  }
  if (pr) {
    if (manifest.pr_number !== pr.number) {
      throw new ArtifactRejected(`manifest names PR #${manifest.pr_number}, but head ${run.headSha} belongs to #${pr.number}`);
    }
    // The head moved while we were queued. Reviewing a superseded SHA would publish a check on
    // a commit nobody is looking at, so stop: the push that moved it already started a new run.
    if (pr.head.sha !== run.headSha) {
      throw new ArtifactRejected(`stale head: PR #${pr.number} is now at ${pr.head.sha}, this run prepared ${run.headSha}`);
    }
    // The **base** SHA is deliberately NOT compared against the live PR.
    //
    // `pr.base.sha` tracks the tip of the base branch, so it moves every time anything merges to
    // `main`. An earlier draft rejected the artifact when it disagreed with the manifest — which
    // meant that any merge to `main` during the window between the two stages would publish a
    // *failing* `AI Review` on a pull request with nothing wrong with it. As a required check
    // that blocks the merge, and re-running does not help: a re-run replays the original event
    // payload, so the manifest's base is stale again. Only a fresh push could clear it.
    //
    // It also bought nothing. The diff was computed `base...head` by our own prepare run against
    // the base it recorded, the artifact is hash-pinned, and the head SHA is checked strictly
    // above — which together already fix exactly what was reviewed. A moving base branch is a
    // fact about the repository, not evidence of tampering.
  }

  if (body.length > CAPS.artifactBytes) {
    throw new ArtifactRejected(`artifact is ${body.length} bytes, over the ${CAPS.artifactBytes}-byte cap`);
  }
  if (body.length !== manifest.input_bytes) {
    throw new ArtifactRejected(`artifact is ${body.length} bytes, manifest claims ${manifest.input_bytes}`);
  }
  const digest = sha256(body);
  if (digest !== manifest.input_sha256) {
    throw new ArtifactRejected(`artifact sha256 ${digest} != manifest ${manifest.input_sha256}`);
  }

  let input;
  try {
    input = JSON.parse(body);
  } catch (err) {
    throw new ArtifactRejected(`artifact input is not JSON: ${err.message}`);
  }
  if (input.head_sha !== manifest.head_sha) {
    throw new ArtifactRejected("artifact input disagrees with its own manifest about the head SHA");
  }
  return input;
}

/**
 * Artifact *selection* is `actions/download-artifact`'s job, not this module's.
 *
 * The trusted workflow names the artifact it wants (`name: ai-review-input`) and the action fails
 * the step if no such artifact exists. An earlier draft had a `selectArtifact` here, with four
 * passing tests — and nothing ever called it. A tested-but-unreachable check in the module whose
 * whole job is to be the trust boundary is not a safety net, it is a *false* one: the tests report
 * a guarantee that does not run. It is deleted rather than kept "just in case", and what actually
 * enforces the guarantee is named here instead. Everything the artifact *contains* is still
 * verified below, which is the part that matters.
 */

/**
 * Has this exact head SHA already been reviewed by this exact reviewer?
 *
 * One review per head SHA (ADR 0015), so a rerun of an unchanged SHA costs nothing and a new
 * push gets a fresh, independent review. Keyed on the reviewer version too: bumping the prompt
 * is a *different* reviewer, and re-reviewing under it is the correct behaviour.
 */
export function alreadyReviewed(checkRuns, { reviewerVersion = REVIEWER_VERSION } = {}) {
  return (checkRuns ?? []).some(
    (c) =>
      c.name === CHECK_NAME &&
      c.status === "completed" &&
      c.conclusion === "success" &&
      typeof c.output?.summary === "string" &&
      c.output.summary.includes(`reviewer ${reviewerVersion}`),
  );
}
