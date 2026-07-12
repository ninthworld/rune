import { git } from "./git.js";

/**
 * Paths that govern CI, and therefore govern the runner itself.
 *
 * The app holds `workflows: write`, so a provider *can* rewrite the checks that gate its own
 * PR — including hollowing out a required job while keeping its name, which reports green.
 * ADR 0016 takes that capability in exchange for making every use of it loud.
 */
export const CI_GOVERNANCE = [
  ".github/workflows/",
  ".github/actions/",
  ".github/rulesets/",
  ".github/CODEOWNERS",
  "Makefile",
  "scripts/bot-",
];

const GENERATED = ["target/", "node_modules/", "clients/web/dist/", "clients/web/playwright-report/"];

/** Credential shapes that must never be committed. Matched against added lines only. */
const SECRETS = [
  /\bgh[posru]_[A-Za-z0-9]{20,}/,
  /\bgithub_pat_[A-Za-z0-9_]{20,}/,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
  /\bsk-(ant-)?[A-Za-z0-9_-]{20,}/,
];

export function isCiPath(file) {
  return CI_GOVERNANCE.some((prefix) => file.startsWith(prefix));
}

/**
 * Inspects what the provider actually did, independently of what it claims it did.
 *
 * Runs before anything is committed or pushed, so every rejection here leaves the work in the
 * sandbox and the claim intact — the run stays resumable rather than being thrown away.
 */
export function inspect(workspace, { allowCi = false, baseSha } = {}) {
  // Compared against the *base*, not HEAD. A provider that committed its work would otherwise
  // show an empty `git diff HEAD` and read as having done nothing at all.
  const from = baseSha ?? "HEAD";
  const files = git(["diff", "--name-only", from], { cwd: workspace })
    .split("\n")
    .filter(Boolean)
    .concat(git(["ls-files", "--others", "--exclude-standard"], { cwd: workspace }).split("\n").filter(Boolean));

  const violations = [];
  const ciPaths = files.filter(isCiPath);

  // The provider was told not to commit: the runner owns the commit, so that its author and
  // message are the runner's rather than whatever identity the provider happened to carry.
  const head = git(["rev-parse", "HEAD"], { cwd: workspace });
  if (baseSha && head !== baseSha) {
    violations.push({
      outcome: "scope_rejected",
      detail: `the provider created commits (HEAD is ${head.slice(0, 7)}, expected ${baseSha.slice(0, 7)}); the runner owns commits`,
    });
  }

  if (files.length === 0) {
    violations.push({ outcome: "no_op", detail: "the provider changed nothing" });
  }

  const generated = files.filter((f) => GENERATED.some((dir) => f.startsWith(dir)));
  if (generated.length > 0) {
    violations.push({ outcome: "scope_rejected", detail: `generated paths in the diff: ${generated.join(", ")}` });
  }

  const added = git(["diff", from, "--unified=0"], { cwd: workspace })
    .split("\n")
    .filter((line) => line.startsWith("+") && !line.startsWith("+++"));
  if (added.some((line) => SECRETS.some((re) => re.test(line)))) {
    violations.push({ outcome: "scope_rejected", detail: "the diff contains something shaped like a credential" });
  }

  if (ciPaths.length > 0 && !allowCi) {
    violations.push({
      outcome: "ci_change_refused",
      detail:
        `the diff touches CI-governance paths: ${ciPaths.join(", ")}\n` +
        "These gate the runner itself. Re-run with --allow-ci if that is intended; the PR will be\n" +
        "labelled ci-change and the paths called out for review.",
    });
  }

  return { files, ciPaths, violations, ok: violations.length === 0 };
}
