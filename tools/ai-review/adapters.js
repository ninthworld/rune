/**
 * Provider-neutral reviewer adapters.
 *
 * This is the ADR 0016 adapter idea — one bounded invocation per provider, the provider is the
 * only thing that varies, and no provider owns any workflow state — applied to a role that must
 * be *weaker* than a coding provider, not stronger. The reviewer must have **no shell, no
 * filesystem write, no GitHub write, no network beyond the model call, and no MCP** (#243).
 *
 * Two ways to get that, and they are not equally strong. Both are supported, because the
 * difference is a real trade and the maintainer, not this file, should get to make it:
 *
 * **`anthropic` / `openai` — a raw HTTPS request.** The strongest form: there is no tool loop to
 * escape from because there *is* no tool loop. The request carries no tool definitions at all;
 * the model returns text and the text is parsed. Requires a metered API key.
 *
 * **`claude` — the Claude Code CLI in print mode.** What a Pro/Max subscription can actually
 * authenticate (`claude setup-token`), which is why it is the default: the alternative would be
 * asking this project to pay per token for a review when it already has a subscription that
 * covers it. But the CLI *is* an agent harness, so "no tools" here is **enforced rather than
 * structural**, and that deserves saying out loud instead of being buried:
 *
 *   1. every built-in tool is denied (`--disallowed-tools`), and MCP is empty and strict, so no
 *      server can add one back;
 *   2. it runs in an **empty scratch directory**, so `CLAUDE.md`/`AGENTS.md` auto-discovery finds
 *      nothing and a tool that somehow ran would have nothing to read;
 *   3. its environment is an **allowlist** — `PATH`, a scratch `HOME`, and its own model token.
 *      `GITHUB_TOKEN` is *not* in it, so even a shell would have no credential to push with;
 *   4. a tool-less review is **exactly one turn**, and the adapter refuses a result with more
 *      than one. A tool call cannot happen without raising the turn count, so this is a
 *      runtime check on the property, not a hope.
 *
 * The residual gap versus the raw API is honest and worth stating: a future CLI that ignored
 * `--disallowed-tools` would be caught by (4) but not prevented by (1). Points (2) and (3) are
 * what make it survivable anyway — the thing the ADR 0015 threat model actually protects is the
 * credential, and the credential is not reachable.
 *
 * What makes prompt injection *bounded* under either adapter: the diff is hostile input and it
 * will try things (the prompt tells the reviewer to report it as a finding when it does), but
 * the worst a successful injection achieves is a **wrong review**. It cannot make the reviewer
 * read a secret or push a commit. Compare the interim `claude-code-review.yml` this replaces,
 * which held a credential *and* checked out the pull request.
 *
 * `local` exists so no vendor is canonical: any command that reads a prompt on stdin and writes
 * the model's text to stdout. It prescribes no model, harness, or vendor.
 */

import { execFile } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { CLAUDE_CLI, LIMITS } from "./config.js";

export class ProviderError extends Error {
  constructor(message, { retryable = false } = {}) {
    super(message);
    this.retryable = retryable;
  }
}

export const PROVIDERS = ["claude", "anthropic", "openai", "local"];

/** Which environment variable each provider takes its credential from. Nothing else is passed. */
export const CREDENTIAL_ENV = {
  claude: "CLAUDE_CODE_OAUTH_TOKEN",
  anthropic: "ANTHROPIC_API_KEY",
  openai: "OPENAI_API_KEY",
  local: null,
};

/** HTTP statuses worth another attempt: transient by definition, not a bad request. */
const RETRYABLE_STATUS = new Set([408, 409, 429, 500, 502, 503, 504]);

async function httpJson(url, { headers, body, timeoutMs, fetchImpl }) {
  let res;
  try {
    res = await fetchImpl(url, {
      method: "POST",
      headers: { "content-type": "application/json", ...headers },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (err) {
    // A timeout or a socket error is infrastructure, and ADR 0015 requires that to be legible as
    // an outage rather than a clean review.
    throw new ProviderError(`request failed: ${err.message}`, { retryable: true });
  }

  const text = await res.text();
  if (!res.ok) {
    throw new ProviderError(`provider returned ${res.status}: ${text.slice(0, 300)}`, {
      retryable: RETRYABLE_STATUS.has(res.status),
    });
  }
  try {
    return JSON.parse(text);
  } catch {
    throw new ProviderError(`provider returned non-JSON: ${text.slice(0, 300)}`, { retryable: false });
  }
}

const ANTHROPIC = {
  model: "claude-opus-4-8",
  async call(prompt, { apiKey, maxOutputTokens, timeoutMs, fetchImpl }) {
    const data = await httpJson("https://api.anthropic.com/v1/messages", {
      headers: { "x-api-key": apiKey, "anthropic-version": "2023-06-01" },
      body: {
        model: ANTHROPIC.model,
        max_tokens: maxOutputTokens,
        // No `tools` key. Not an empty array to be disabled later — absent. The model has no
        // tool it could call even if the diff talked it into wanting one.
        messages: [{ role: "user", content: prompt }],
      },
      timeoutMs,
      fetchImpl,
    });
    const text = (data.content ?? [])
      .filter((b) => b.type === "text")
      .map((b) => b.text)
      .join("");
    if (!text) throw new ProviderError("provider returned an empty message", { retryable: true });
    return { text, model: data.model ?? ANTHROPIC.model, usage: data.usage ?? null };
  },
};

const OPENAI = {
  model: "gpt-5",
  async call(prompt, { apiKey, maxOutputTokens, timeoutMs, fetchImpl }) {
    const data = await httpJson("https://api.openai.com/v1/chat/completions", {
      headers: { authorization: `Bearer ${apiKey}` },
      body: {
        model: OPENAI.model,
        max_completion_tokens: maxOutputTokens,
        messages: [{ role: "user", content: prompt }],
      },
      timeoutMs,
      fetchImpl,
    });
    const text = data.choices?.[0]?.message?.content ?? "";
    if (!text) throw new ProviderError("provider returned an empty message", { retryable: true });
    return { text, model: data.model ?? OPENAI.model, usage: data.usage ?? null };
  },
};

/**
 * The Claude Code CLI, in print mode, with everything that makes it an *agent* switched off.
 *
 * This is the adapter a Pro/Max subscription can authenticate. See the file header for why its
 * "no tools" guarantee is enforced rather than structural, and what carries the weight instead.
 */
const CLAUDE = {
  model: CLAUDE_CLI.model,

  /** Exactly what the subprocess may see. Note what is absent: GITHUB_TOKEN, and everything else. */
  env(apiKey, { home }) {
    return {
      PATH: process.env.PATH,
      HOME: home,
      [CREDENTIAL_ENV.claude]: apiKey,
      // Print mode is not a TTY; some terminals confuse the CLI's renderer without this.
      CI: "1",
    };
  },

  args() {
    return [
      "-p",
      "--output-format",
      "json",
      "--model",
      CLAUDE_CLI.model,
      // Every built-in tool, denied. A reviewer reads a diff it was handed; it does not go
      // looking for more, and it certainly does not run anything.
      "--disallowed-tools",
      CLAUDE_CLI.deniedTools.join(" "),
      // No MCP server may add a tool back. `--strict-mcp-config` ignores every other MCP source
      // (user settings, project config), and the config we do pass is empty.
      "--strict-mcp-config",
      "--mcp-config",
      '{"mcpServers":{}}',
    ];
  },

  call(prompt, { apiKey, timeoutMs, execImpl = execFile, mkdtempImpl = mkdtempSync }) {
    // An empty directory: no CLAUDE.md or AGENTS.md to auto-discover, and nothing for a tool to
    // read if one somehow ran. The reviewer's entire context is the prompt we versioned.
    const scratch = mkdtempImpl(join(tmpdir(), "rune-review-"));

    return new Promise((resolve, reject) => {
      const child = execImpl(
        CLAUDE_CLI.command,
        CLAUDE.args(),
        {
          cwd: scratch,
          env: CLAUDE.env(apiKey, { home: scratch }),
          timeout: timeoutMs,
          maxBuffer: 16 * 1024 * 1024,
        },
        (err, stdout, stderr) => {
          if (err) {
            const killed = err.killed || err.signal;
            return reject(
              new ProviderError(`claude CLI failed: ${err.message}${stderr ? ` — ${String(stderr).slice(0, 200)}` : ""}`, {
                // A timeout is transient; a non-zero exit from a bad flag is not.
                retryable: Boolean(killed),
              }),
            );
          }

          let out;
          try {
            const text = String(stdout);
            out = JSON.parse(text.slice(text.indexOf("{")));
          } catch (parseErr) {
            return reject(new ProviderError(`claude CLI did not return JSON: ${parseErr.message}`, { retryable: true }));
          }

          if (out.is_error || out.subtype !== "success") {
            return reject(
              new ProviderError(`claude CLI reported ${out.subtype ?? "an error"}: ${String(out.result ?? "").slice(0, 200)}`, {
                retryable: out.subtype === "error_during_execution",
              }),
            );
          }

          // The invariant that turns "tools are denied" from a flag into an observation: a
          // tool-less review is one turn. A tool call — any tool call — costs a turn. If this is
          // ever not 1, something reached a tool it should not have, and the safe thing to do
          // with that review is refuse it, not publish it.
          if (out.num_turns !== 1) {
            return reject(
              new ProviderError(
                `the reviewer took ${out.num_turns} turns; a tool-less review is exactly one. ` +
                  "Something reached a tool it should not have — refusing this review rather than publishing it.",
                { retryable: false },
              ),
            );
          }

          if (!out.result) return reject(new ProviderError("claude CLI returned an empty result", { retryable: true }));

          resolve({
            text: out.result,
            model: Object.keys(out.modelUsage ?? {})[0] ?? CLAUDE_CLI.model,
            usage: out.usage ?? null,
          });
        },
      );
      child.stdin?.end(prompt);
    });
  },
};

const LOCAL = {
  model: "local",
  call(prompt, { timeoutMs, command = process.env.RUNE_REVIEW_CMD, execImpl = execFile }) {
    if (!command) throw new ProviderError("provider `local` needs RUNE_REVIEW_CMD", { retryable: false });
    return new Promise((resolve, reject) => {
      const child = execImpl(
        "sh",
        ["-c", command],
        { timeout: timeoutMs, maxBuffer: 8 * 1024 * 1024, env: { PATH: process.env.PATH } },
        (err, stdout) => {
          if (err) return reject(new ProviderError(`local reviewer failed: ${err.message}`, { retryable: false }));
          if (!String(stdout).trim()) return reject(new ProviderError("local reviewer produced no output", { retryable: false }));
          resolve({ text: String(stdout), model: process.env.RUNE_REVIEW_MODEL ?? "local", usage: null });
        },
      );
      child.stdin?.end(prompt);
    });
  },
};

const ADAPTERS = { claude: CLAUDE, anthropic: ANTHROPIC, openai: OPENAI, local: LOCAL };

/**
 * One review: bounded attempts, exponential backoff, and a hard per-request deadline.
 *
 * `maxModelCalls` is the cost ceiling per head SHA. Exhausting it is an **infrastructure
 * failure** — the check fails and says so — and is never reported as a review that found
 * nothing, which is the failure mode that would quietly turn an outage into a green light.
 */
export async function review(
  prompt,
  {
    provider,
    apiKey,
    maxOutputTokens,
    timeoutMs = LIMITS.requestTimeoutMs,
    maxAttempts = LIMITS.maxAttempts,
    backoffMs = LIMITS.backoffMs,
    fetchImpl = globalThis.fetch,
    execImpl,
    mkdtempImpl,
    sleepImpl = (ms) => new Promise((r) => setTimeout(r, ms)),
  },
) {
  const adapter = ADAPTERS[provider];
  if (!adapter) throw new ProviderError(`unknown provider ${JSON.stringify(provider)} (have: ${PROVIDERS.join(", ")})`);

  const credentialEnv = CREDENTIAL_ENV[provider];
  if (credentialEnv && !apiKey) throw new ProviderError(`provider ${provider} needs ${credentialEnv}`, { retryable: false });

  // The cost ceiling wins over the caller: `maxModelCalls` is the number of times this head SHA
  // may ever hit a paid endpoint, and a caller asking for more attempts does not get them.
  const ceiling = Math.min(maxAttempts, LIMITS.maxModelCalls);

  let last;
  let made = 0;
  for (let attempt = 1; attempt <= ceiling; attempt++) {
    made = attempt;
    try {
      const out = await adapter.call(prompt, { apiKey, maxOutputTokens, timeoutMs, fetchImpl, execImpl, mkdtempImpl });
      return { ...out, provider, attempts: attempt };
    } catch (err) {
      last = err;
      if (!(err instanceof ProviderError) || !err.retryable || attempt === ceiling) break;
      await sleepImpl(backoffMs * 2 ** (attempt - 1));
    }
  }
  // Report the attempts actually made, not the ceiling: "failed after 3 attempts" when we gave up
  // on the first 400 would misreport a bad request as an outage we fought hard against.
  throw new ProviderError(`review failed after ${made} attempt(s): ${last?.message}`, { retryable: false });
}

/**
 * Extracts the JSON object from a model's reply.
 *
 * Models wrap JSON in prose and fences however much you ask them not to, and a review thrown
 * away because of a fence is an outage that was really a formatting quirk. Prose *around* the
 * object is tolerated; a reply with no object in it at all is not, and fails as infrastructure.
 */
export function parseModelJson(text) {
  const fenced = /```(?:json)?\s*(\{[\s\S]*?\})\s*```/.exec(text);
  const candidate = fenced ? fenced[1] : text.slice(text.indexOf("{"), text.lastIndexOf("}") + 1);
  if (!candidate || !candidate.startsWith("{")) {
    throw new ProviderError(`no JSON object in the model's reply: ${text.slice(0, 200)}`, { retryable: false });
  }
  try {
    return JSON.parse(candidate);
  } catch (err) {
    throw new ProviderError(`the model's JSON did not parse: ${err.message}`, { retryable: false });
  }
}
