#!/usr/bin/env node
/**
 * `make ci-lint` — the policy half of the workflow gate (#199).
 *
 * Checks every committed workflow against the rules in `policy.js` and exits non-zero on the
 * first violation. Deterministic and offline: it reads files, and does nothing else.
 */

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { checkWorkflow } from "./policy.js";

const dir = process.argv[2] ?? ".github/workflows";

let files;
try {
  files = readdirSync(dir)
    .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
    .sort();
} catch (err) {
  console.error(`ci-policy: cannot read ${dir}: ${err.message}`);
  process.exit(2);
}

if (files.length === 0) {
  console.error(`ci-policy: no workflows found in ${dir} — refusing to pass a gate that checked nothing`);
  process.exit(2);
}

const findings = files.flatMap((f) => checkWorkflow(f, readFileSync(join(dir, f), "utf8")));

for (const f of findings) {
  console.error(`${join(dir, f.workflow)}:${f.line}: [${f.rule}] ${f.message}`);
}

if (findings.length > 0) {
  console.error(`\nci-policy: ${findings.length} violation(s) in ${files.length} workflow(s).`);
  process.exit(1);
}

console.log(`ci-policy: ${files.length} workflow(s) clean (${files.join(", ")})`);
