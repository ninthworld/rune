/**
 * The TRUSTED stage (ADR 0015 stage 2), end to end.
 *
 * Ordering is the security property, so it is worth reading as a sequence: verify the event,
 * verify the artifact, *then* build a prompt out of it, call one bounded model request, validate
 * what comes back, publish. Nothing from the pull request is ever executed, imported, installed,
 * or checked out here — this file and everything it imports came out of the base branch, and the
 * only thing that came out of the pull request is a JSON blob that is treated as text from the
 * moment it arrives until the moment a human reads a finding about it.
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { GitHub } from "../agent-task/github.js";
import { collectContext } from "./prepare.js";
import { PROVIDERS, ProviderError, parseModelJson, review as callProvider } from "./adapters.js";
import { CAPS, CHECK_NAME, LIMITS, REVIEWER_VERSION } from "./config.js";
import { checkSummary, failureSummary, publish, reviewBody } from "./publish.js";
import { InvalidFindings, normalizeFindings } from "./schema.js";
import { ArtifactRejected, alreadyReviewed, verifyArtifact, verifyWorkflowRun } from "./verify.js";

const HERE = dirname(fileURLToPath(import.meta.url));

export function loadPrompt(readImpl = readFileSync) {
  return readImpl(join(HERE, "prompt.md"), "utf8");
}

/**
 * Renders the prompt.
 *
 * The untrusted material goes in *last*, after every instruction, and is announced as untrusted
 * both in the template and again at the fence. This is not a guarantee — nothing about prompt
 * construction is a guarantee against injection — which is exactly why the adapter gives the
 * model no tools at all. Defence in depth: the prompt makes injection unlikely, the absent tool
 * loop makes a successful injection harmless beyond a wrong review, and the human approval gate
 * makes a wrong review non-fatal.
 */
export function renderPrompt(template, input) {
  // Exactly one of each placeholder, or refuse to build a prompt at all.
  //
  // `String.replace` substitutes the FIRST occurrence. The prompt file's own header comment used
  // to *mention* `{{CONTEXT}}` and `{{DIFF}}` while explaining them — so the substitution filled
  // the comment, the real sections kept their literal placeholders, and the untrusted diff was
  // spliced in **above the instructions**: the single place it must never be, since
  // instructions-last is the ordering the whole injection defence rests on. It still produced
  // plausible reviews, which is precisely why it went unnoticed and why this is a hard failure
  // now rather than a comment asking the next person to be careful.
  for (const marker of ["{{CONTEXT}}", "{{DIFF}}"]) {
    const count = template.split(marker).length - 1;
    if (count !== 1) {
      throw new Error(
        `the prompt template contains ${count} copies of ${marker}, expected exactly 1. ` +
          "Substitution replaces the first occurrence, so a stray copy silently captures the content " +
          "and leaves the real section empty.",
      );
    }
  }

  const context = input.context
    .map((d) => `## ${d.path}\n\n${d.text}`)
    .join("\n\n---\n\n");

  const diff = input.patches.map((p) => `### ${p.path}\n\n\`\`\`diff\n${p.patch}\n\`\`\``).join("\n\n");

  // The replacements are passed as **functions**, not strings. In `String.replace`, a string
  // replacement still honours `$$`, `$&`, `` $` ``, and `$'` as substitution patterns — and the
  // diff is attacker-controlled text. A patch containing `` $` `` would splice everything
  // *before* the placeholder (the entire instruction block, and the base branch's constraint
  // documents) into the region the prompt labels untrusted, letting a pull request relocate or
  // duplicate the reviewer's own instructions inside its own diff. A replacer function is
  // substituted verbatim and honours no patterns at all.
  return template
    .replace("{{CONTEXT}}", () => context || "_(no constraint documents applied to this change)_")
    .replace("{{DIFF}}", () => diff || "_(empty diff)_");
}

/**
 * The GitHub API's own list of the files this pull request touches — a source the pull request
 * cannot forge, unlike the artifact. If the untrusted stage hid a file from the reviewer, the diff
 * it handed us is not the change under review, and refusing is the only honest response.
 *
 * Only *omissions* are fatal. A file the artifact lists but the API does not is possible in benign
 * races (the head moved, and the stale-head check above is what catches that properly), and a
 * reviewer seeing one file too many is not a security failure.
 */
export async function crossCheckFileList(gh, prNumber, changedPaths, manifest) {
  const files = [];
  for (let page = 1; page <= 10; page++) {
    const batch = await gh.request("GET", `/repos/${gh.owner}/${gh.repo}/pulls/${prNumber}/files?per_page=100&page=${page}`);
    if (!Array.isArray(batch) || batch.length === 0) break;
    files.push(...batch.map((f) => f.filename));
    if (batch.length < 100) break;
  }
  if (files.length === 0) return;

  // A truncated artifact legitimately omits files, and says so. Only compare what it claimed to
  // have seen in full.
  const truncatedFiles = manifest.truncation?.some((t) => t.kind === "files" || t.kind === "diff_total");
  if (truncatedFiles) return;

  const claimed = new Set(changedPaths);
  const hidden = files.filter((f) => !claimed.has(f));
  if (hidden.length > 0) {
    throw new ArtifactRejected(
      `the prepared input omits ${hidden.length} file(s) the GitHub API says this PR changes ` +
        `(e.g. ${hidden.slice(0, 3).join(", ")}). The untrusted stage runs from the pull request's own ` +
        "workflow file, so a hidden file means the artifact is not the change under review.",
    );
  }
}

/** Resolves the PR for a head SHA — needed because a fork PR's workflow_run has no association. */
async function resolvePr(gh, { prNumber, headSha }) {
  if (prNumber !== null) {
    return gh.request("GET", `/repos/${gh.owner}/${gh.repo}/pulls/${prNumber}`);
  }
  const prs = await gh.request("GET", `/repos/${gh.owner}/${gh.repo}/commits/${headSha}/pulls`);
  const open = (prs ?? []).filter((p) => p.state === "open");
  if (open.length !== 1) {
    throw new ArtifactRejected(`head ${headSha} maps to ${open.length} open PRs; refusing to guess which one to review`);
  }
  return open[0];
}

/**
 * @param {object} deps  everything that touches the outside world, injected so the tests can
 *                       run the whole stage with a faked GitHub, a faked provider, and fixture
 *                       artifacts — no network, no paid model, no repository mutated.
 */
export async function run({
  event,
  repository,
  token,
  provider,
  apiKey,
  loadArtifact,
  fetchImpl,
  execImpl,
  sleepImpl,
  promptImpl = loadPrompt,
  // The constraint documents, re-read from THIS job's trusted base checkout — never taken from
  // the artifact, which the pull request's own workflow file produced.
  readContextImpl = (changedPaths) => collectContext(changedPaths).docs,
  now = () => new Date(),
}) {
  const [owner, repo] = repository.split("/");
  const gh = new GitHub({ owner, repo, token, fetchImpl });

  let manifest = null;

  try {
    if (!PROVIDERS.includes(provider)) {
      throw new ProviderError(`unknown provider ${JSON.stringify(provider)} (have: ${PROVIDERS.join(", ")})`);
    }

    // 1. Is this event even ours?
    const run_ = verifyWorkflowRun(event, repository);

    // 2. Which PR is it, and is its head still what we prepared?
    const pr = await resolvePr(gh, run_);

    // 3. Have we already reviewed this exact head with this exact reviewer?
    const checks = await gh.request("GET", `/repos/${owner}/${repo}/commits/${run_.headSha}/check-runs`);
    if (alreadyReviewed(checks?.check_runs)) {
      console.log(`ai-review: ${run_.headSha} already reviewed by reviewer ${REVIEWER_VERSION} — nothing to do`);
      return { outcome: "already_reviewed", headSha: run_.headSha };
    }

    // 4. Is the artifact the one our prepare run made, unaltered?
    const { manifest: m, body } = await loadArtifact(run_);
    manifest = m;
    const input = verifyArtifact({ manifest, body, run: run_, repository, pr });

    // 4a. Do not trust the untrusted stage about *what the change even is*.
    //
    // A `pull_request` workflow runs from the PULL REQUEST's own head — so a pull request can
    // edit `ai-review-prepare.yml` itself. `prepare.js` is read from the base checkout, but the
    // YAML that invokes it is not, and a rewritten prepare job could produce a perfectly
    // well-formed, correctly-hashed artifact that simply lies: a benign subset of the diff, or
    // constraint documents saying the rules permit whatever this PR does. Provenance checks
    // cannot catch that — the artifact really did come from our workflow name, in our repository,
    // for this head SHA. It was the *contents* that were forged.
    //
    // So the trusted stage independently re-establishes both halves from sources the pull request
    // does not control:
    //
    //   - the **constraint documents** are re-read from this job's own trusted base checkout, and
    //     the artifact's copies are discarded. The rules a change is judged against now come from
    //     `main`, in the job holding the credential, full stop.
    //   - the **file list** is cross-checked against the GitHub API's own view of the PR. If the
    //     artifact hid a file, the diff it presents is not the change under review, and there is
    //     nothing honest to do with it but refuse.
    //
    // What remains, and is worth stating plainly: a pull request can still make its own review
    // *fail* (a red check on itself), and it can still corrupt a *file's* patch text within a file
    // it legitimately touches. It cannot make its review falsely clean by rewriting the rules, and
    // it cannot hide a file. And it never had a path to a secret — prepare holds none.
    const trustedContext = readContextImpl(input.changed_paths);
    await crossCheckFileList(gh, pr.number, input.changed_paths, manifest);

    // 5. One bounded model request. No tools; nothing here can execute the diff.
    const prompt = renderPrompt(promptImpl(), { ...input, context: trustedContext });
    const result = await callProvider(prompt, {
      provider,
      apiKey,
      maxOutputTokens: CAPS.maxOutputTokens,
      timeoutMs: LIMITS.requestTimeoutMs,
      fetchImpl,
      execImpl,
      sleepImpl,
    });

    // 6. The model's output is untrusted too.
    const { findings, dropped, findings_truncated } = normalizeFindings(parseModelJson(result.text), {
      headSha: manifest.head_sha,
      changedPaths: input.changed_paths,
    });

    await publish(gh, {
      prNumber: pr.number,
      headSha: manifest.head_sha,
      body: reviewBody({ findings, manifest, provider, model: result.model, dropped, findings_truncated }),
      summary: checkSummary({ findings, manifest, provider, model: result.model, dropped, findings_truncated }),
      // Findings do not fail this check. Only a failure to review does.
      conclusion: "success",
      title: findings.length === 0 ? "Review completed — no findings" : `Review completed — ${findings.length} advisory finding(s)`,
    });

    console.log(`ai-review: ${findings.length} finding(s) on ${manifest.head_sha} (${provider}/${result.model})`);
    return { outcome: "completed", findings, dropped, findings_truncated, headSha: manifest.head_sha, provider, model: result.model, now: now() };
  } catch (err) {
    // Everything below this line is the honest-outage path. An ArtifactRejected, a provider
    // outage, a malformed model reply — none of them is a review, so none of them may look
    // like one. Publishing a *failing* check is how the merge gate learns that.
    const fatal = err instanceof ArtifactRejected || err instanceof ProviderError || err instanceof InvalidFindings;
    const headSha = manifest?.head_sha ?? event?.head_sha;

    if (headSha) {
      try {
        await publish(gh, {
          prNumber: null,
          headSha,
          body: null, // no PR comment for an outage: there is nothing to say about the code
          summary: failureSummary(err, { manifest, provider }),
          conclusion: "failure",
          title: `${CHECK_NAME} did not complete`,
        });
      } catch (publishErr) {
        console.error(`ai-review: could not publish the failure check: ${publishErr.message}`);
      }
    }

    console.error(`ai-review: ${fatal ? "review failed" : "unexpected error"}: ${err.message}`);
    return { outcome: "failed", error: err.message, headSha };
  }
}
