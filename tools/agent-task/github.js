import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const API = "https://api.github.com";

/**
 * Mints an installation token for the `rune-agent` GitHub App.
 *
 * Shells out to `scripts/bot-token.sh` rather than reimplementing the JWT exchange:
 * ADR 0016 keeps exactly one implementation of the credential path, and it is the one
 * already hardened by #206/#208.
 */
export function mintToken() {
  const here = dirname(fileURLToPath(import.meta.url));
  const script = join(here, "..", "..", "scripts", "bot-token.sh");
  return execFileSync(script, { encoding: "utf8" }).trim();
}

export class ApiError extends Error {
  constructor(status, method, path, body) {
    super(`${method} ${path} -> ${status}: ${body}`);
    this.status = status;
  }
}

/**
 * The GitHub surface the runner needs. `fetchImpl` is injected so tests can fake the
 * API at the `fetch` boundary without a network or a token.
 */
export class GitHub {
  constructor({ owner, repo, token, fetchImpl = globalThis.fetch }) {
    this.owner = owner;
    this.repo = repo;
    this.token = token;
    this.fetchImpl = fetchImpl;
  }

  async request(method, path, body) {
    const res = await this.fetchImpl(`${API}${path}`, {
      method,
      headers: {
        accept: "application/vnd.github+json",
        authorization: `token ${this.token}`,
        "x-github-api-version": "2022-11-28",
        ...(body ? { "content-type": "application/json" } : {}),
      },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const text = await res.text();
    if (!res.ok) throw new ApiError(res.status, method, path, text);
    return text ? JSON.parse(text) : null;
  }

  repoPath(suffix) {
    return `/repos/${this.owner}/${this.repo}${suffix}`;
  }

  issue(number) {
    return this.request("GET", this.repoPath(`/issues/${number}`));
  }

  comment(number, body) {
    return this.request("POST", this.repoPath(`/issues/${number}/comments`), { body });
  }

  addLabels(number, labels) {
    return this.request("POST", this.repoPath(`/issues/${number}/labels`), { labels });
  }

  /** A label that is already absent is not an error — label moves must be re-runnable. */
  async removeLabel(number, label) {
    try {
      await this.request("DELETE", this.repoPath(`/issues/${number}/labels/${encodeURIComponent(label)}`));
    } catch (err) {
      if (err.status !== 404) throw err;
    }
  }

  async branchSha(branch) {
    try {
      const ref = await this.request("GET", this.repoPath(`/git/ref/heads/${branch}`));
      return ref.object.sha;
    } catch (err) {
      if (err.status === 404) return null;
      throw err;
    }
  }

  /**
   * Creates a branch, returning false if it already exists.
   *
   * This is the claim (ADR 0016). GitHub answers 422 when the ref exists, so the call is
   * a compare-and-swap: exactly one runner can win, with no lease file and no race window.
   * A GitHub App cannot be an issue assignee, so this — not assignment — is the lock.
   */
  async createBranch(branch, sha) {
    try {
      await this.request("POST", this.repoPath("/git/refs"), { ref: `refs/heads/${branch}`, sha });
      return true;
    } catch (err) {
      if (err.status === 422) return false;
      throw err;
    }
  }

  async deleteBranch(branch) {
    try {
      await this.request("DELETE", this.repoPath(`/git/refs/heads/${branch}`));
      return true;
    } catch (err) {
      if (err.status === 404) return false;
      throw err;
    }
  }
}
