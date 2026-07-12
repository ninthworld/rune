import assert from "node:assert/strict";
import { test } from "node:test";

import { MAX_BRIEF_BYTES, buildBrief } from "./brief.js";
import { anIssue } from "./testing/fake-github.js";

const RUN = { branch: "agent/186-x", issue: 186, allow_ci: false };

test("the brief points at the repository's docs instead of inlining them", () => {
  const brief = buildBrief({ issue: anIssue(), run: RUN });

  assert.match(brief, /AGENTS\.md/);
  assert.match(brief, /docs\/coding-standards\.md/);
  // Inlining them would flatten the nested-AGENTS.md discovery the harnesses implement, and
  // would crowd the issue out of the context window.
  assert.doesNotMatch(brief, /Zero game logic in the client/);
});

test("the brief forbids the provider from touching GitHub or publishing", () => {
  const brief = buildBrief({ issue: anIssue(), run: RUN });
  for (const forbidden of [/do not.*commit/i, /push/i, /pull request/i, /merge/i, /labels/i]) {
    assert.match(brief, forbidden);
  }
});

test("CI paths are off-limits unless the run was started with --allow-ci", () => {
  assert.match(buildBrief({ issue: anIssue(), run: RUN }), /refused before it can be pushed/);
  assert.match(buildBrief({ issue: anIssue(), run: { ...RUN, allow_ci: true } }), /--allow-ci.*permitted/s);
});

test("the brief is capped, and says so when it truncates", () => {
  const brief = buildBrief({ issue: anIssue({ body: "x".repeat(50_000) }), run: RUN });

  assert.ok(Buffer.byteLength(brief) <= MAX_BRIEF_BYTES, `brief was ${Buffer.byteLength(brief)} bytes`);
  assert.match(brief, /truncated/);
});

test("dependencies are listed by title and state, not by pasting their bodies", () => {
  const brief = buildBrief({
    issue: anIssue(),
    run: RUN,
    dependencies: [{ number: 185, state: "closed", title: "the ADR" }],
  });
  assert.match(brief, /#185 \(closed\): the ADR/);
});
