import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

import { RULES, checkWorkflow } from "./policy.js";
import { scan } from "./scan.js";

const SHA = "34e114876b0b11c390a56381ad16ebd13914f8d5";

/** A workflow that satisfies every rule — the baseline each fixture below breaks once. */
const clean = `name: Clean
on:
  pull_request:
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@${SHA} # v4.3.1
      - name: Build
        run: make check
`;

const rules = (text) => checkWorkflow("w.yml", text).map((f) => f.rule);

test("a compliant workflow produces no findings", () => {
  assert.deepEqual(checkWorkflow("w.yml", clean), []);
});

// --- the three rejections the issue's acceptance criteria name --------------------------

test("a tag-pinned Action is rejected", () => {
  const text = clean.replace(`actions/checkout@${SHA} # v4.3.1`, "actions/checkout@v4");
  const found = checkWorkflow("w.yml", text);
  assert.deepEqual(
    found.map((f) => f.rule),
    [RULES.PINNED_ACTIONS],
  );
  assert.equal(found[0].line, 10);
  assert.match(found[0].message, /mutable/);
});

test("a branch-pinned Action is rejected, and so is a SHA that is merely short", () => {
  assert.deepEqual(rules(clean.replace(`@${SHA}`, "@main")), [RULES.PINNED_ACTIONS]);
  assert.deepEqual(rules(clean.replace(SHA, SHA.slice(0, 7))), [RULES.PINNED_ACTIONS]);
});

test("a malformed workflow is rejected rather than half-read", () => {
  const tabbed = clean.replace("  contents: read", "\tcontents: read");
  const found = checkWorkflow("w.yml", tabbed);
  assert.deepEqual(
    found.map((f) => f.rule),
    [RULES.MALFORMED],
  );
  assert.match(found[0].message, /tab/);

  const garbage = clean.replace("    runs-on: ubuntu-latest", "    this is not yaml");
  assert.deepEqual(rules(garbage), [RULES.MALFORMED]);

  // A file we cannot read yields exactly one finding: we do not report policy verdicts
  // about a structure we failed to parse.
  assert.equal(checkWorkflow("w.yml", "\tnope: true").length, 1);
});

test("an unjustified write permission is rejected", () => {
  const text = clean.replace("  contents: read", "  contents: read\n  id-token: write");
  const found = checkWorkflow("w.yml", text);
  assert.deepEqual(
    found.map((f) => f.rule),
    [RULES.WRITE_JUSTIFIED],
  );
  assert.match(found[0].message, /no stated reason/);
});

test("a write permission is accepted when it says why, on the line or above it", () => {
  const trailing = clean.replace("  contents: read", "  contents: read\n  id-token: write # OIDC for the action");
  assert.deepEqual(checkWorkflow("w.yml", trailing), []);

  const above = clean.replace("  contents: read", "  contents: read\n  # OIDC for the action\n  id-token: write");
  assert.deepEqual(checkWorkflow("w.yml", above), []);

  // An empty comment is not a reason.
  const empty = clean.replace("  contents: read", "  contents: read\n  id-token: write #");
  assert.deepEqual(rules(empty), [RULES.WRITE_JUSTIFIED]);
});

// --- the rest of the policy -------------------------------------------------------------

test("a pinned Action with no version comment is rejected as unreviewable", () => {
  assert.deepEqual(rules(clean.replace(" # v4.3.1", "")), [RULES.VERSION_COMMENT]);
  // A comment that is not a version does not count as one.
  assert.deepEqual(rules(clean.replace("# v4.3.1", "# checkout")), [RULES.VERSION_COMMENT]);
});

test("a local action is exempt: a repository SHA is the wrong reference for it", () => {
  assert.deepEqual(checkWorkflow("w.yml", clean.replace(`actions/checkout@${SHA} # v4.3.1`, "./.github/actions/setup")), []);
});

test("a job with no permissions is rejected when the workflow declares none", () => {
  const text = clean.replace("permissions:\n  contents: read\n", "");
  const found = checkWorkflow("w.yml", text);
  assert.deepEqual(
    found.map((f) => f.rule),
    [RULES.PERMISSIONS_DECLARED],
  );
  assert.match(found[0].message, /repository default/);
});

test("job-level permissions satisfy the rule without a top-level block", () => {
  const text = clean
    .replace("permissions:\n  contents: read\n", "")
    .replace("    runs-on: ubuntu-latest", "    runs-on: ubuntu-latest\n    permissions:\n      contents: read");
  assert.deepEqual(checkWorkflow("w.yml", text), []);
});

test("pull_request_target is rejected", () => {
  const text = clean.replace("  pull_request:", "  pull_request_target:");
  const found = checkWorkflow("w.yml", text);
  assert.deepEqual(
    found.map((f) => f.rule),
    [RULES.NO_PULL_REQUEST_TARGET],
  );
  assert.match(found[0].message, /exfiltration/);
});

test("untrusted event data interpolated into a shell is rejected", () => {
  const inline = clean.replace("run: make check", "run: echo ${{ github.event.pull_request.title }}");
  assert.deepEqual(rules(inline), [RULES.NO_UNTRUSTED_INTERPOLATION]);

  const block = clean.replace("        run: make check", "        run: |\n          echo ${{ github.event.issue.body }}");
  assert.deepEqual(rules(block), [RULES.NO_UNTRUSTED_INTERPOLATION]);

  // The same expression in an `if:` is an expression, not a shell string — not injection.
  const guarded = clean.replace("      - name: Build", "    if: contains(github.event.issue.body, '@claude')\n      - name: Build");
  assert.equal(rules(guarded).includes(RULES.NO_UNTRUSTED_INTERPOLATION), false);
});

// --- the scanner's own contract ----------------------------------------------------------

test("two steps may both say `uses:` — they are separate mappings, not a duplicate key", () => {
  const two = clean.replace(
    `      - uses: actions/checkout@${SHA} # v4.3.1`,
    `      - uses: actions/checkout@${SHA} # v4.3.1\n      - uses: actions/setup-node@${SHA} # v4.4.0`,
  );
  assert.deepEqual(checkWorkflow("w.yml", two), []);
  assert.equal(scan(two).filter((n) => n.key === "uses").length, 2);
});

test("a genuinely duplicated key in one mapping is malformed", () => {
  const dupe = clean.replace("    runs-on: ubuntu-latest", "    runs-on: ubuntu-latest\n    runs-on: macos-latest");
  assert.deepEqual(rules(dupe), [RULES.MALFORMED]);
});

test("a block scalar's body is skipped, not parsed as keys", () => {
  const withBlock = clean.replace(
    "      - name: Build",
    "      - name: Config\n        with:\n          extra: |\n            not: a\n            real: key\n      - name: Build",
  );
  assert.deepEqual(checkWorkflow("w.yml", withBlock), []);
});

// --- the committed workflows must actually satisfy their own gate -------------------------

test("every committed workflow passes the policy", () => {
  const dir = new URL("../../.github/workflows/", import.meta.url).pathname;
  const files = readdirSync(dir).filter((f) => f.endsWith(".yml"));
  assert.ok(files.length >= 4, `expected the repository's workflows, found ${files.length}`);

  const findings = files.flatMap((f) => checkWorkflow(f, readFileSync(join(dir, f), "utf8")));
  assert.deepEqual(
    findings.map((f) => `${f.workflow}:${f.line} ${f.rule}`),
    [],
  );
});
