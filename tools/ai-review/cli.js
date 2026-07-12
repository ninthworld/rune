#!/usr/bin/env node
/**
 * The trusted stage's entry point: `node tools/ai-review/cli.js <artifact-dir>`.
 *
 * Every input arrives through the environment, and the set below is the whole of it. Note what
 * is *not* here: no repository checkout path, no PR ref, no way to point this at head content.
 * The artifact directory holds two files that `actions/download-artifact` fetched from the
 * prepare run, and they are read as bytes and hashed before anything looks at them.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { CREDENTIAL_ENV } from "./adapters.js";
import { run } from "./review.js";

const artifactDir = process.argv[2] ?? "ai-review-input";

const provider = process.env.RUNE_REVIEW_PROVIDER ?? "claude";
const credentialEnv = CREDENTIAL_ENV[provider];

const event = JSON.parse(process.env.WORKFLOW_RUN_JSON ?? "null");

const result = await run({
  event,
  repository: process.env.GITHUB_REPOSITORY,
  token: process.env.GITHUB_TOKEN,
  provider,
  apiKey: credentialEnv ? process.env[credentialEnv] : null,
  loadArtifact: () => ({
    manifest: JSON.parse(readFileSync(join(artifactDir, "manifest.json"), "utf8")),
    body: readFileSync(join(artifactDir, "input.json"), "utf8"),
  }),
});

// The check run already carries the outcome; the job's own exit code is what makes `AI Review`
// a *required-to-complete* gate rather than an advisory one. A rejected artifact or a provider
// outage must turn this job red — a green job with a red check would be an invitation to
// ignore the check.
if (result.outcome === "failed") process.exit(1);
