/**
 * The UNTRUSTED stage (ADR 0015 stage 1).
 *
 * Runs on `pull_request`, with `contents: read` and **no secrets at all**. It sees
 * attacker-controlled file content, and that is fine precisely because it holds nothing worth
 * stealing. Its one job is to turn a pull request into a bounded, hashed, inert artifact that
 * the trusted stage can consume without ever touching the pull request itself.
 *
 * Two rules govern everything here:
 *
 *  1. **Nothing from the head is executed.** Not `make`, not Cargo, not npm, not a script, not a
 *     hook, not a binary. Head content is read as bytes and diffed. Even `git` is run with
 *     hooks and attribute drivers disabled, because a `.gitattributes` in the head can name a
 *     textconv/filter driver and `git diff` would gladly run it.
 *
 *  2. **Everything the reviewer will be *judged against* comes from the base**, never the head.
 *     This module is itself checked out from the base ref by the workflow. If the constraint
 *     documents came from the head, a pull request could rewrite the rules it is about to be
 *     reviewed against — editing `AGENTS.md` to say "the client may contain game logic" is a
 *     perfectly legal diff. It judges the change against the rules as they are on `main`, and a
 *     change *to* those rules then appears where it should: in the diff, for a human to read.
 */

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { CAPS, CONTEXT_DOCS, NESTED_CONTEXT, REVIEWER_VERSION, SCHEMA_VERSION } from "./config.js";

export const sha256 = (text) => createHash("sha256").update(text).digest("hex");

/**
 * `git`, with every mechanism by which a repository can make git execute something turned off.
 *
 * This is the same rule ADR 0016 applies in the runner ("no credentialed command ever runs in a
 * provider-controlled repository"): here there is no credential to lose, but the habit is what
 * keeps the next person from adding one.
 */
export function git(args, { cwd, execImpl = execFileSync } = {}) {
  return execImpl(
    "git",
    ["-c", "core.hooksPath=/dev/null", "-c", "core.attributesFile=/dev/null", "-c", "core.fsmonitor=false", ...args],
    { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
}

/** The files the diff touches, and each file's patch, already per-file capped. */
export function collectDiff(baseSha, headSha, { cwd = ".", execImpl = execFileSync } = {}) {
  const names = git(["diff", "--name-only", `${baseSha}...${headSha}`], { cwd, execImpl })
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);

  const truncation = [];
  const included = names.slice(0, CAPS.files);
  if (names.length > included.length) {
    // `omitted` is itself capped. A pull request touching 5,000 files would otherwise put 5,000
    // path strings into the manifest, blow the artifact cap, and hard-fail prepare — turning
    // "this PR is large" into a red required check. Truncation must degrade, never explode.
    truncation.push({
      kind: "files",
      limit: CAPS.files,
      actual: names.length,
      omitted: names.slice(CAPS.files, CAPS.files + CAPS.omittedPathsListed),
      omitted_count: names.length - included.length,
    });
  }

  const patches = [];
  let total = 0;

  for (const path of included) {
    let patch = git(["diff", "--no-textconv", `${baseSha}...${headSha}`, "--", path], { cwd, execImpl });

    if (patch.length > CAPS.fileDiffBytes) {
      truncation.push({ kind: "file_diff", path, limit: CAPS.fileDiffBytes, actual: patch.length });
      patch = `${patch.slice(0, CAPS.fileDiffBytes)}\n… [truncated: this file's diff exceeded ${CAPS.fileDiffBytes} bytes]\n`;
    }
    if (total + patch.length > CAPS.diffBytes) {
      truncation.push({
        kind: "diff_total",
        limit: CAPS.diffBytes,
        omitted: included.slice(included.indexOf(path)),
      });
      break;
    }
    total += patch.length;
    patches.push({ path, patch });
  }

  return { changed_paths: names, patches, truncation };
}

/** The constraint documents, from the base checkout. Nested ones only when the diff goes there. */
export function collectContext(changedPaths, { cwd = ".", readImpl = readFileSync, existsImpl = existsSync } = {}) {
  const docs = [];
  const truncation = [];
  let total = 0;

  for (const path of CONTEXT_DOCS) {
    const scope = NESTED_CONTEXT[path];
    if (scope && !changedPaths.some((p) => p.startsWith(scope))) continue;

    const full = join(cwd, path);
    if (!existsImpl(full)) continue;

    let text = readImpl(full, "utf8");
    if (total + text.length > CAPS.contextBytes) {
      const room = Math.max(0, CAPS.contextBytes - total);
      truncation.push({ kind: "context", path, limit: CAPS.contextBytes, actual: text.length });
      text = text.slice(0, room);
      if (room === 0) continue;
    }
    total += text.length;
    docs.push({ path, text, sha256: sha256(text) });
  }

  return { docs, truncation };
}

/**
 * Builds the artifact: a manifest (what this is, and the hash of every byte in it) and the
 * review input itself. The trusted stage re-hashes and refuses anything that does not match,
 * so a tampered artifact is a failed check rather than a poisoned prompt.
 */
export function buildArtifact({ repository, prNumber, baseSha, headSha, runId, title, diff, context }) {
  // Same reasoning: the full changed-path list is unbounded, and the reviewer only needs it to
  // tell "is this finding about a file in the diff?" — which is answered by the paths it was
  // actually shown. The true count is preserved in the manifest.
  const changedPaths = diff.changed_paths.slice(0, CAPS.files);

  const input = {
    schema_version: SCHEMA_VERSION,
    repository,
    pr_number: prNumber,
    base_sha: baseSha,
    head_sha: headSha,
    // Bounded metadata only. The PR *body* is deliberately absent: it is free-form
    // attacker-controlled prose whose only purpose here would be to talk to the reviewer.
    title: String(title ?? "").slice(0, 300),
    context: context.docs,
    patches: diff.patches,
    changed_paths: changedPaths,
  };

  const body = JSON.stringify(input);
  const truncation = [...diff.truncation, ...context.truncation];

  if (body.length > CAPS.artifactBytes) {
    throw new Error(
      `prepared input is ${body.length} bytes, over the ${CAPS.artifactBytes}-byte artifact cap — ` +
        "tighten CAPS rather than raising this, or the trusted stage will refuse it",
    );
  }

  const manifest = {
    schema_version: SCHEMA_VERSION,
    reviewer_version: REVIEWER_VERSION,
    repository,
    pr_number: prNumber,
    base_sha: baseSha,
    head_sha: headSha,
    prepare_run_id: String(runId),
    input_sha256: sha256(body),
    input_bytes: body.length,
    file_count: diff.patches.length,
    changed_path_count: diff.changed_paths.length,
    context_paths: context.docs.map((d) => d.path),
    truncated: truncation.length > 0,
    truncation,
  };

  return { manifest, body };
}

/** Writes `manifest.json` + `input.json` into `outDir` for `actions/upload-artifact`. */
export function writeArtifact(outDir, { manifest, body }, { writeImpl = writeFileSync, mkdirImpl = mkdirSync } = {}) {
  mkdirImpl(outDir, { recursive: true });
  writeImpl(join(outDir, "input.json"), body);
  writeImpl(join(outDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

/** CLI: `node tools/ai-review/prepare.js <out-dir>` — every input arrives via the environment. */
export function main(env = process.env, argv = process.argv) {
  const outDir = argv[2] ?? "ai-review-input";
  const baseSha = env.PR_BASE_SHA;
  const headSha = env.PR_HEAD_SHA;

  if (!baseSha || !headSha) throw new Error("PR_BASE_SHA and PR_HEAD_SHA must be set");

  const diff = collectDiff(baseSha, headSha);
  const context = collectContext(diff.changed_paths);
  const artifact = buildArtifact({
    repository: env.GITHUB_REPOSITORY,
    prNumber: Number(env.PR_NUMBER),
    baseSha,
    headSha,
    runId: env.GITHUB_RUN_ID,
    title: env.PR_TITLE,
    diff,
    context,
  });

  const manifest = writeArtifact(outDir, artifact);
  console.log(
    `ai-review/prepare: ${manifest.file_count} file(s), ${manifest.input_bytes} bytes` +
      `${manifest.truncated ? `, TRUNCATED (${manifest.truncation.map((t) => t.kind).join(", ")})` : ""}`,
  );
  return manifest;
}

if (import.meta.url === `file://${process.argv[1]}`) main();
