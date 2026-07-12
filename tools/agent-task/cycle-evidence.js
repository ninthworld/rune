import { spawn } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { cycleDir } from "./cycle-state.js";
import { crCitations, findMilestone, parseRoadmap, whereWeAre } from "./cycle-roadmap.js";
import { git } from "./git.js";
import { redactor } from "./redact.js";
import { GATES, GATE_SETS } from "./verify.js";

/**
 * The Evidence Bundle: everything true about a milestone at one pinned commit, and nothing
 * that is an opinion about it (ADR 0017 §"Deterministic evidence collection").
 *
 * The bundle is the whole reason the cycle is not a single unverified model pass. Every
 * later stage — the Auditor, its independent reviewer, the human at the closeout gate —
 * argues from *this*, so a disagreement can always be traced back to what was actually true
 * at `base_commit_sha` rather than to what a model remembered. Which is also why nothing
 * here is a verdict: collection observes, it never concludes. In particular a closed issue
 * is recorded as a closed issue and never as a satisfied criterion — an issue can close
 * without its acceptance criteria being met, and a criterion can span several issues or
 * predate issue tracking entirely.
 *
 * A version this collector does not recognize is a hard failure downstream, never a silent
 * partial read: a bundle whose shape drifted from the schema the Auditor was written
 * against is worse than no bundle at all.
 */
export const EVIDENCE_SCHEMA_VERSION = 1;

/** The checks `main`'s ruleset actually requires — the same list the gates mirror. */
const REQUIRED_CHECKS = GATES.map((gate) => gate.name);

const STUB_PATTERN = "TODO|FIXME|todo!\\(|unimplemented!\\(|dbg!\\(";
const STUB_ROOTS = ["crates", "clients/web/src", "clients/web/e2e", "tools", "scripts"];

/**
 * The test suites each gate is known to run.
 *
 * Declared rather than inferred, because "this gate reported some counts" is not the same as
 * "we read every suite it ran". The `Client` gate runs both vitest and `node --test`; when a
 * parser silently missed vitest, the gate still yielded `node:test` counts and so looked
 * fully parsed — a whole suite vanished from the evidence and nothing said so. Naming what to
 * expect is what turns that into a reported `missing_counts` entry instead of a quiet zero.
 */
const EXPECTED_SUITES = {
  Engine: ["cargo"],
  Client: ["vitest", "node:test"],
  E2E: ["playwright"],
  "cargo-deny": [],
};

/** SGR colour sequences. Vitest emits them into a pipe, so the counts hide behind them. */
const ANSI = /\u001B\[[0-9;]*m/g;

/**
 * Collects the bundle. `workspace` is a checkout pinned at `base_commit_sha`, so every read
 * below — roadmap, coverage, ADRs, protocol, grep — sees one consistent point in time.
 */
export async function collectEvidence({
  gh,
  workspace,
  milestone,
  cycleId,
  baseSha,
  gateSet = "verify",
  root,
  spawnImpl = spawn,
  now = new Date(),
}) {
  const roadmapText = read(workspace, "docs/roadmap.md");
  const roadmap = parseRoadmap(roadmapText);
  const found = findMilestone(roadmap, milestone);
  if (!found) {
    const known = roadmap.milestones.map((m) => m.id).join(", ");
    throw new Error(`no milestone "${milestone}" in docs/roadmap.md (known: ${known})`);
  }

  const sections = new Set(found.exit_criteria.flatMap((c) => c.cr_citations.sections));
  const terms = [...new Set(found.exit_criteria.flatMap((c) => c.terms))];

  const issues = await collectIssues(gh, found);
  const prs = await collectPrs(gh, issues);
  const checks = await collectChecks(gh, prs);
  const fresh = await runFreshGates({ cycleId, workspace, gateSet, root, spawnImpl });

  return {
    schema_version: EVIDENCE_SCHEMA_VERSION,
    cycle_id: cycleId,
    milestone: found.name,
    milestone_id: found.id,
    base_commit_sha: baseSha,
    collected_at: now.toISOString(),
    roadmap_last_reconciled: roadmap.last_reconciled,

    exit_criteria: found.exit_criteria,
    issues,
    prs,
    ci: { required_checks: REQUIRED_CHECKS, merged_pr_checks: checks, fresh_run: fresh.run },
    tests: fresh.tests,
    rules_coverage: collectRulesCoverage(read(workspace, "docs/rules-coverage.md"), sections),
    adr_protocol_state: collectAdrProtocolState(workspace, found.exit_criteria, terms),
    todos_and_stubs: collectStubs(workspace, terms),
    documented_gaps: {
      partial_notes: found.exit_criteria
        .filter((c) => c.notes.length > 0)
        .map((c) => ({ criterion_id: c.criterion_id, notes: c.notes })),
      where_we_are: whereWeAre(roadmapText),
    },
  };
}

function read(workspace, path) {
  return readFileSync(join(workspace, path), "utf8");
}

/**
 * The milestone's issues, from both places the project records that mapping.
 *
 * GitHub's milestone field is only set on the recent waves; the roadmap's own tables are the
 * durable link for everything older. Taking the union and recording *which* source each
 * issue came from surfaces the drift between them (an issue the roadmap tracks but GitHub
 * does not tag, and vice versa) instead of quietly picking one and under-reporting.
 */
export async function collectIssues(gh, milestone) {
  const byNumber = new Map();

  const tagged = await taggedIssues(gh, milestone.name);
  for (const issue of tagged) byNumber.set(issue.number, record(issue, ["github-milestone"]));

  for (const number of milestone.issue_refs) {
    if (byNumber.has(number)) {
      byNumber.get(number).sources.push("roadmap");
      continue;
    }
    const issue = await gh.request("GET", gh.repoPath(`/issues/${number}`)).catch((err) => {
      if (err.status === 404) return null;
      throw err;
    });
    if (issue) byNumber.set(number, record(issue, ["roadmap"]));
  }

  return [...byNumber.values()].sort((a, b) => a.number - b.number);
}

function record(issue, sources) {
  return {
    number: issue.number,
    title: issue.title,
    state: issue.state,
    labels: (issue.labels || []).map((l) => (typeof l === "string" ? l : l.name)),
    milestone: issue.milestone?.title ?? null,
    is_pull_request: Boolean(issue.pull_request),
    closed_at: issue.closed_at ?? null,
    sources,
  };
}

async function taggedIssues(gh, name) {
  const milestones = await gh.request("GET", gh.repoPath("/milestones?state=all&per_page=100"));
  const match = (milestones || []).find((m) => m.title === name || name.startsWith(m.title));
  if (!match) return [];
  return paginate(gh, `/issues?milestone=${match.number}&state=all`);
}

async function paginate(gh, path, { max = 10 } = {}) {
  const out = [];
  const sep = path.includes("?") ? "&" : "?";
  for (let page = 1; page <= max; page++) {
    const batch = await gh.request("GET", gh.repoPath(`${path}${sep}per_page=100&page=${page}`));
    if (!batch || batch.length === 0) break;
    out.push(...batch);
    if (batch.length < 100) break;
  }
  return out;
}

/**
 * The PRs behind the milestone's issues, with their merge SHA and `Closes #N` linkage.
 *
 * Walked from each issue's timeline rather than by listing the repository's pull requests:
 * the timeline is the link GitHub itself drew, so a PR that closed an issue *without* the
 * magic keyword is still found. Both links are then recorded and neither is treated as the
 * other — `referenced_by` is GitHub's link, `closes` is the PR's own claim parsed from its
 * body — because "this PR mentioned the issue" and "this PR closed the issue" are different
 * facts, and collapsing them is how a milestone gets credited with work that never landed.
 *
 * A mere mention is kept rather than filtered out: an extra PR costs the Auditor a glance,
 * and a missing one costs it the evidence.
 */
export async function collectPrs(gh, issues) {
  const referenced = new Map();
  for (const issue of issues) {
    if (issue.is_pull_request) continue;
    const timeline = await paginate(gh, `/issues/${issue.number}/timeline`, { max: 3 }).catch(() => []);
    for (const event of timeline) {
      const source = event.source?.issue;
      if (!source?.pull_request) continue;
      if (event.event !== "cross-referenced" && event.event !== "connected") continue;
      if (!referenced.has(source.number)) referenced.set(source.number, new Set());
      referenced.get(source.number).add(issue.number);
    }
  }

  const tracked = new Set(issues.map((i) => i.number));
  const prs = [];
  for (const number of [...referenced.keys()].sort((a, b) => a - b)) {
    const pr = await gh.request("GET", gh.repoPath(`/pulls/${number}`)).catch((err) => {
      if (err.status === 404) return null;
      throw err;
    });
    if (!pr) continue;
    prs.push({
      number: pr.number,
      title: pr.title,
      state: pr.state,
      merged: Boolean(pr.merged_at),
      merged_at: pr.merged_at ?? null,
      // Only a merged PR has a merge commit; an abandoned one must not appear to have landed.
      merge_commit_sha: pr.merged_at ? (pr.merge_commit_sha ?? null) : null,
      closes: closesLinkage(pr.body).filter((n) => tracked.has(n)),
      referenced_by: [...referenced.get(number)].sort((a, b) => a - b),
    });
  }
  return prs;
}

export function closesLinkage(body) {
  const pattern = /\b(?:close[sd]?|fixe?[sd]?|resolve[sd]?)\s+#(\d+)/gi;
  return [...new Set([...String(body || "").matchAll(pattern)].map((m) => Number(m[1])))].sort((a, b) => a - b);
}

/**
 * What the required checks actually reported on each merged PR's merge commit.
 *
 * Recorded per PR, and never conflated with the fresh run below: green CI on a PR proves
 * that PR passed *at the time it merged*, not that `main` passes today. ADR 0017 keeps both
 * because a milestone can be built entirely out of green PRs and still be broken on `main`.
 */
export async function collectChecks(gh, prs) {
  const out = [];
  for (const pr of prs) {
    if (!pr.merge_commit_sha) continue;
    const runs = await gh
      .request("GET", gh.repoPath(`/commits/${pr.merge_commit_sha}/check-runs`))
      .catch(() => ({ check_runs: [] }));
    const relevant = (runs?.check_runs ?? [])
      .filter((c) => REQUIRED_CHECKS.includes(c.name))
      .map((c) => ({ name: c.name, status: c.status, conclusion: c.conclusion ?? null }));
    out.push({ pr: pr.number, sha: pr.merge_commit_sha, checks: relevant });
  }
  return out;
}

/**
 * A real gate run against `base_commit_sha` itself.
 *
 * This is the one piece of evidence nothing on GitHub can supply. It runs the same targets
 * the required checks run, in a clone pinned at the audited commit, and it is the difference
 * between "every PR in this milestone was green" and "this milestone is green".
 *
 * Unsandboxed, unlike the runner's gates — deliberately, and only because of *what* it
 * builds. `verify.js` wraps `make` in an isolation boundary because a run's workspace holds
 * provider-authored code, so a doctored `Makefile` or `build.rs` would execute with the
 * maintainer's credentials in reach. Here the workspace is a clone of `main`: reviewed,
 * approved, and merged code, no more dangerous to run than `make check` in the maintainer's
 * own checkout, which is where it runs from. **If a later stage ever runs gates over code
 * that is not yet on `main`** — a proposed fix, a provider's patch — it must go through
 * `verify.js` and its isolation, not through here.
 */
export async function runFreshGates({ cycleId, workspace, gateSet = "verify", root, spawnImpl = spawn }) {
  const wanted = GATE_SETS[gateSet] ?? GATE_SETS.verify;
  const dir = cycleDir(cycleId, root);
  mkdirSync(join(dir, "logs"), { recursive: true });
  const log = createWriteStream(join(dir, "logs", "gates.log"), { flags: "a" });
  const logClosed = new Promise((resolve) => log.on("close", resolve));
  const sink = redactor((line) => log.write(line));

  const gates = [];
  const suites = [];
  try {
    for (const gate of GATES.filter((g) => wanted.includes(g.name))) {
      const started = Date.now();
      sink.push(`\n=== ${gate.name}: make ${gate.targets.join(" ")}\n`);
      let output = "";

      const code = await new Promise((resolve, reject) => {
        const child = spawnImpl("make", gate.targets, {
          cwd: workspace,
          stdio: ["ignore", "pipe", "pipe"],
        });
        child.stdout.setEncoding("utf8");
        child.stderr.setEncoding("utf8");
        const take = (chunk) => {
          output += chunk;
          sink.push(chunk);
        };
        child.stdout.on("data", take);
        child.stderr.on("data", take);
        child.on("error", reject);
        child.on("close", resolve);
      });

      gates.push({ gate: gate.name, ok: code === 0, exit_code: code, duration_ms: Date.now() - started });
      suites.push(...parseTestCounts(output).map((suite) => ({ ...suite, gate: gate.name })));
      // Unlike a run's gates, a red gate does not stop collection: the Auditor needs to see
      // the whole surface, and "we stopped looking after the first failure" is exactly the
      // kind of missing evidence that turns into a wrong `met`.
    }
    return {
      run: {
        base_gate_set: gateSet,
        gates,
        ok: gates.length > 0 && gates.every((g) => g.ok),
        log: join(dir, "logs", "gates.log"),
      },
      tests: {
        suites,
        // Named, not inferred. A suite whose counts we could not read must not read as
        // "0 tests failed" — that is a claim, and we did not observe it.
        missing_counts: gates.flatMap((g) =>
          (EXPECTED_SUITES[g.gate] ?? [])
            .filter((suite) => !suites.some((s) => s.gate === g.gate && s.suite === suite))
            .map((suite) => ({ gate: g.gate, suite })),
        ),
      },
    };
  } finally {
    sink.flush();
    log.end();
    await logClosed;
  }
}

/**
 * Pass/fail counts per suite, scraped from gate output. Counts only — never test output.
 *
 * Each harness announces its totals in its own format, so each gets its own matcher; a format
 * we do not recognize yields no suite at all and is reported in `missing_counts`, because a
 * silently missing suite is indistinguishable from a passing one.
 *
 * Colour is stripped first. Vitest colourizes its summary even when its output is a pipe
 * rather than a terminal, so the line the parser is looking for arrives as
 * `\x1b[2m Tests \x1b[22m \x1b[32m178 passed\x1b[39m` — and a matcher written against the
 * plain text silently reads zero suites from a perfectly good run.
 */
export function parseTestCounts(output) {
  const suites = [];
  const text = String(output).replace(ANSI, "");

  const cargo = [...text.matchAll(/test result: \w+\.\s+(\d+) passed;\s+(\d+) failed/g)];
  if (cargo.length > 0) {
    suites.push({
      suite: "cargo",
      passed: cargo.reduce((sum, m) => sum + Number(m[1]), 0),
      failed: cargo.reduce((sum, m) => sum + Number(m[2]), 0),
    });
  }

  const vitest = /^\s*Tests\s+(?:(\d+) failed \|\s*)?(\d+) passed/m.exec(text);
  if (vitest) suites.push({ suite: "vitest", passed: Number(vitest[2]), failed: Number(vitest[1] ?? 0) });

  const nodePass = /^# pass (\d+)$/m.exec(text);
  const nodeFail = /^# fail (\d+)$/m.exec(text);
  if (nodePass && nodeFail) {
    suites.push({ suite: "node:test", passed: Number(nodePass[1]), failed: Number(nodeFail[1]) });
  }

  const playwright = /^\s*(?:(\d+) failed\s*\n\s*)?(\d+) passed \(\d/m.exec(text);
  if (playwright) {
    suites.push({ suite: "playwright", passed: Number(playwright[2]), failed: Number(playwright[1] ?? 0) });
  }

  return suites;
}

/**
 * The `docs/rules-coverage.md` rows in the milestone's stated CR scope.
 *
 * Matched on the top-level CR section, which over-includes (a criterion naming CR 704.5g
 * pulls in every 704 row). That is the safe direction: an extra row costs the Auditor a
 * glance, a missing one costs it the evidence.
 */
export function collectRulesCoverage(markdown, sections) {
  const wanted = new Set(sections);
  const rows = [];

  for (const line of markdown.split("\n")) {
    if (!/^\|\s*CR\s/.test(line)) continue;
    const cells = line
      .split("|")
      .slice(1, -1)
      .map((c) => c.trim());
    if (cells.length < 5) continue;

    const cited = crCitations(cells[0]);
    if (!cited.sections.some((s) => wanted.has(s))) continue;
    rows.push({
      cr: cells[0],
      sections: cited.sections,
      // `implemented` / `partial — <the gap, named>`: the roadmap's own convention, carried
      // verbatim. A `partial` row is evidence *against* a `met` verdict, and the reason it
      // is partial is the part the Auditor needs.
      status: cells[2],
      code_anchor: cells[3],
      test_anchor: cells[4],
    });
  }
  return rows;
}

/**
 * The status of every ADR the criteria name, and whether `docs/protocol.md` documents the
 * shapes they require.
 *
 * "Documents" is deliberately mechanical: the criteria name their artifacts in backticks
 * (`GameView.result`, `valid_actions`), and this reports whether that identifier literally
 * appears in the protocol spec. Presence is not correctness — but absence is decisive, and
 * this is the collector, not the Auditor.
 */
export function collectAdrProtocolState(workspace, criteria, terms) {
  const wanted = [...new Set(criteria.flatMap((c) => c.adr_refs))].sort();
  const decisions = join(workspace, "docs", "decisions");
  const files = existsSync(decisions) ? readdirSync(decisions) : [];

  const adrs = wanted.map((id) => {
    const file = files.find((name) => name.startsWith(`${id}-`));
    if (!file) return { adr: id, found: false, status: null, path: null };
    const text = readFileSync(join(decisions, file), "utf8");
    return {
      adr: id,
      found: true,
      status: /^-\s*Status:\s*(\S+)/mi.exec(text)?.[1]?.toLowerCase() ?? null,
      path: `docs/decisions/${file}`,
    };
  });

  const protocolPath = join(workspace, "docs", "protocol.md");
  const protocol = existsSync(protocolPath) ? readFileSync(protocolPath, "utf8") : "";
  const identifiers = terms.filter((t) => /^[A-Za-z_][\w.[\]]*$/.test(t) && !/\.(md|json|rs|ts|toml)$/.test(t));

  return {
    adrs,
    protocol: {
      path: "docs/protocol.md",
      sections: [...protocol.matchAll(/^#{2,4}\s+(.+)$/gm)].map((m) => m[1].trim()),
      terms: identifiers.map((term) => ({ term, documented: protocol.includes(term) })),
    },
  };
}

/**
 * Unfinished work, as `file:line`, scoped to the paths the criteria actually name.
 *
 * A `todo!()` under a path a criterion claims is done is the cheapest possible refutation of
 * a `met`, and no model has to be trusted to find it. Scope comes from the criteria's own
 * backticked paths, falling back to the source roots — a hand-maintained path list would
 * rot, and a repository-wide sweep would drown the signal.
 *
 * Locations only: the matched *line* is never recorded, so no source text — and no secret
 * some file happens to contain — can ride into the bundle.
 */
export function collectStubs(workspace, terms) {
  const named = terms
    .map((t) => t.replace(/^\.\//, ""))
    .filter((t) => t.includes("/") && existsSync(join(workspace, t)));
  const roots = (named.length > 0 ? named : STUB_ROOTS).filter((p) => existsSync(join(workspace, p)));
  if (roots.length === 0) return { roots: [], matches: [] };

  let output = "";
  try {
    output = git(["grep", "-nI", "-E", STUB_PATTERN, "--", ...roots], { cwd: workspace });
  } catch (err) {
    // `git grep` exits 1 on "no matches", which is a result, not a failure.
    if (err.status !== 1) throw err;
  }

  const matches = output
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [file, lineNo] = line.split(":");
      const marker = new RegExp(STUB_PATTERN).exec(line)?.[0] ?? null;
      return { file, line: Number(lineNo), marker };
    })
    .filter((m) => Number.isInteger(m.line));

  return { roots, matches };
}

/** Fields no bundle may ever carry: the ADR 0016 "never a payload" rule, enforced. */
const FORBIDDEN = ["prompt", "brief", "diff", "patch", "provider_log", "reasoning", "env"];

/**
 * Validates the bundle before anything downstream reads it (#189: validate against a
 * versioned schema *before* invoking a model).
 *
 * Reports every problem at once rather than throwing on the first: a bundle is collected in
 * minutes-long steps, and a validator that surfaces one fault per run turns a fix into a
 * morning. Structural only — it asserts the bundle is well-formed, never that the milestone
 * is in any particular state.
 */
export function validateBundle(bundle) {
  const problems = [];
  const fail = (path, problem) => problems.push({ path, problem });

  if (!bundle || typeof bundle !== "object") return { ok: false, problems: [{ path: "", problem: "not an object" }] };
  if (bundle.schema_version !== EVIDENCE_SCHEMA_VERSION) {
    fail("schema_version", `expected ${EVIDENCE_SCHEMA_VERSION}, got ${JSON.stringify(bundle.schema_version)}`);
  }
  for (const field of ["cycle_id", "milestone", "base_commit_sha", "collected_at"]) {
    if (typeof bundle[field] !== "string" || bundle[field] === "") fail(field, "missing");
  }
  for (const field of ["issues", "prs", "rules_coverage"]) {
    if (!Array.isArray(bundle[field])) fail(field, "missing");
  }

  if (!Array.isArray(bundle.exit_criteria) || bundle.exit_criteria.length === 0) {
    fail("exit_criteria", "missing — a milestone with no criteria cannot be audited");
  } else {
    const seen = new Set();
    bundle.exit_criteria.forEach((c, i) => {
      if (!c?.criterion_id) fail(`exit_criteria[${i}].criterion_id`, "missing");
      else if (seen.has(c.criterion_id)) fail(`exit_criteria[${i}].criterion_id`, `duplicate ${c.criterion_id}`);
      else seen.add(c.criterion_id);
      // `raw` is the load-bearing one: it is the sentence the human wrote, and the Auditor is
      // required to audit *it* rather than a paraphrase of it.
      if (typeof c?.raw !== "string" || c.raw === "") fail(`exit_criteria[${i}].raw`, "missing verbatim text");
      if (typeof c?.checked !== "boolean") fail(`exit_criteria[${i}].checked`, "missing");
    });
  }

  const fresh = bundle.ci?.fresh_run;
  if (!fresh || !Array.isArray(fresh.gates) || fresh.gates.length === 0) {
    fail("ci.fresh_run.gates", "missing — CI history proves those PRs passed, not that main passes now");
  }
  if (!bundle.tests || !Array.isArray(bundle.tests.suites)) fail("tests.suites", "missing");
  if (!bundle.adr_protocol_state) fail("adr_protocol_state", "missing");
  if (!bundle.todos_and_stubs) fail("todos_and_stubs", "missing");
  if (!bundle.documented_gaps) fail("documented_gaps", "missing");

  for (const key of FORBIDDEN) {
    if (key in bundle) fail(key, "forbidden: a bundle carries evidence, never a payload");
  }

  return { ok: problems.length === 0, problems };
}
