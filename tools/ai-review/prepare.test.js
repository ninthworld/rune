import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { CAPS } from "./config.js";
import { buildArtifact, collectContext, collectDiff, git, sha256 } from "./prepare.js";

const BASE = "b".repeat(40);
const HEAD = "h".repeat(40);

/** A fake `git` that answers `--name-only` with a file list and per-file diffs with a patch. */
function fakeGit({ names, patch = (p) => `--- a/${p}\n+++ b/${p}\n+line\n` }) {
  const calls = [];
  const execImpl = (_cmd, args) => {
    calls.push(args);
    if (args.includes("--name-only")) return `${names.join("\n")}\n`;
    const path = args[args.length - 1];
    return patch(path);
  };
  return { execImpl, calls };
}

test("git runs with every execution mechanism a hostile repo could use turned off", () => {
  const calls = [];
  git(["diff"], { execImpl: (_c, args) => (calls.push(args), "") });

  const flags = calls[0].join(" ");
  assert.match(flags, /core\.hooksPath=\/dev\/null/, "a head-controlled hook must never run");
  assert.match(flags, /core\.attributesFile=\/dev\/null/, "a head .gitattributes must not name a textconv driver");
});

test("the diff is collected per file, and the patch is asked for with --no-textconv", () => {
  const { execImpl, calls } = fakeGit({ names: ["src/a.rs", "src/b.rs"] });
  const diff = collectDiff(BASE, HEAD, { execImpl });

  assert.deepEqual(diff.changed_paths, ["src/a.rs", "src/b.rs"]);
  assert.equal(diff.patches.length, 2);
  assert.ok(calls.slice(1).every((c) => c.includes("--no-textconv")));
  assert.equal(diff.truncation.length, 0);
});

test("too many files is a recorded truncation, not a silent drop", () => {
  const names = Array.from({ length: CAPS.files + 5 }, (_, i) => `f${i}.rs`);
  const { execImpl } = fakeGit({ names });
  const diff = collectDiff(BASE, HEAD, { execImpl });

  assert.equal(diff.patches.length, CAPS.files);
  assert.equal(diff.changed_paths.length, names.length, "the full list is still reported");
  const t = diff.truncation.find((x) => x.kind === "files");
  assert.equal(t.actual, names.length);
  assert.equal(t.omitted.length, 5);
});

test("one enormous file is truncated in place rather than crowding out every other file", () => {
  const { execImpl } = fakeGit({
    names: ["huge.rs", "small.rs"],
    patch: (p) => (p === "huge.rs" ? "x".repeat(CAPS.fileDiffBytes + 100) : "+ok\n"),
  });
  const diff = collectDiff(BASE, HEAD, { execImpl });

  assert.equal(diff.patches.length, 2, "small.rs still made it in");
  assert.ok(diff.patches[0].patch.includes("truncated"));
  assert.ok(diff.patches[0].patch.length < CAPS.fileDiffBytes + 200);
  assert.equal(diff.truncation.some((t) => t.kind === "file_diff" && t.path === "huge.rs"), true);
});

test("a diff that blows the total budget stops, and says which files it never saw", () => {
  const per = CAPS.fileDiffBytes;
  const count = Math.ceil(CAPS.diffBytes / per) + 3;
  const names = Array.from({ length: count }, (_, i) => `f${i}.rs`);
  const { execImpl } = fakeGit({ names, patch: () => "x".repeat(per) });
  const diff = collectDiff(BASE, HEAD, { execImpl });

  const t = diff.truncation.find((x) => x.kind === "diff_total");
  assert.ok(t, "the total-bytes cap was recorded");
  assert.ok(t.omitted.length > 0, "and it names what was left out");
  const bytes = diff.patches.reduce((n, p) => n + p.patch.length, 0);
  assert.ok(bytes <= CAPS.diffBytes);
});

// --- context comes from the BASE, and nested docs only when they apply -------------------------

test("nested AGENTS.md is included only when the diff touches its directory", () => {
  const files = {
    "AGENTS.md": "root rules",
    "docs/coding-standards.md": "standards",
    "crates/rune-engine/AGENTS.md": "engine rules",
    "clients/web/AGENTS.md": "client rules",
  };
  const readImpl = (p) => files[p.replace(/^\.\//, "")];
  const existsImpl = (p) => p.replace(/^\.\//, "") in files;

  const engineOnly = collectContext(["crates/rune-engine/src/combat.rs"], { readImpl, existsImpl });
  const paths = engineOnly.docs.map((d) => d.path);
  assert.deepEqual(paths, ["AGENTS.md", "docs/coding-standards.md", "crates/rune-engine/AGENTS.md"]);
  assert.equal(paths.includes("clients/web/AGENTS.md"), false, "the client's rules are not this diff's business");

  const both = collectContext(["crates/rune-engine/src/x.rs", "clients/web/src/y.ts"], { readImpl, existsImpl });
  assert.equal(both.docs.length, 4);
});

test("each context document is hashed, so the trusted stage knows what it was handed", () => {
  const readImpl = () => "rules";
  const existsImpl = (p) => p.endsWith("AGENTS.md");
  const { docs } = collectContext([], { readImpl, existsImpl });
  assert.equal(docs[0].sha256, sha256("rules"));
});

// --- the manifest -------------------------------------------------------------------------------

const diffOf = (patches) => ({ changed_paths: patches.map((p) => p.path), patches, truncation: [] });

test("the manifest hashes the exact bytes the trusted stage will re-hash", () => {
  const { manifest, body } = buildArtifact({
    repository: "ninthworld/rune",
    prNumber: 42,
    baseSha: BASE,
    headSha: HEAD,
    runId: 777,
    title: "t",
    diff: diffOf([{ path: "a.rs", patch: "+x" }]),
    context: { docs: [], truncation: [] },
  });

  assert.equal(manifest.input_sha256, createHash("sha256").update(body).digest("hex"));
  assert.equal(manifest.input_bytes, body.length);
  assert.equal(manifest.head_sha, HEAD);
  assert.equal(manifest.prepare_run_id, "777");
  assert.equal(manifest.truncated, false);
});

test("truncation is carried into the manifest so the review can say it saw a partial diff", () => {
  const diff = { changed_paths: ["a.rs"], patches: [], truncation: [{ kind: "files", limit: 1, actual: 9 }] };
  const { manifest } = buildArtifact({
    repository: "r",
    prNumber: 1,
    baseSha: BASE,
    headSha: HEAD,
    runId: 1,
    title: "t",
    diff,
    context: { docs: [], truncation: [] },
  });
  assert.equal(manifest.truncated, true);
  assert.equal(manifest.truncation[0].kind, "files");
});

test("the PR body is never included — it is prose whose only use here is talking to the reviewer", () => {
  const { body } = buildArtifact({
    repository: "r",
    prNumber: 1,
    baseSha: BASE,
    headSha: HEAD,
    runId: 1,
    title: "t",
    diff: diffOf([]),
    context: { docs: [], truncation: [] },
  });
  const input = JSON.parse(body);
  assert.equal("body" in input, false);
  assert.equal("pr_body" in input, false);
});

test("an oversized prepared input is refused at build time rather than at the trusted stage", () => {
  const huge = [{ path: "a.rs", patch: "x".repeat(CAPS.artifactBytes + 10) }];
  assert.throws(
    () =>
      buildArtifact({
        repository: "r",
        prNumber: 1,
        baseSha: BASE,
        headSha: HEAD,
        runId: 1,
        title: "t",
        diff: diffOf(huge),
        context: { docs: [], truncation: [] },
      }),
    /over the .* artifact cap/,
  );
});
