import assert from "node:assert/strict";
import test from "node:test";

import { CREDENTIAL_ENV, PROVIDERS, ProviderError, parseModelJson, review } from "./adapters.js";

const reply = (text) => ({
  ok: true,
  status: 200,
  text: async () => JSON.stringify({ model: "test-model", content: [{ type: "text", text }] }),
});

const fail = (status) => ({ ok: false, status, text: async () => `boom ${status}` });

const noSleep = async () => {};

test("a bounded model request carries no tools — there is no tool loop to escape", async () => {
  let sent;
  const fetchImpl = async (_url, init) => {
    sent = JSON.parse(init.body);
    return reply('{"findings":[]}');
  };
  await review("prompt", { provider: "anthropic", apiKey: "k", maxOutputTokens: 100, fetchImpl, sleepImpl: noSleep });

  assert.equal("tools" in sent, false, "no tools key at all — not an empty array someone could fill in later");
  assert.equal(sent.max_tokens, 100);
  assert.equal(sent.messages.length, 1);
});

test("the credential goes in the header and the prompt goes in the body", async () => {
  let init;
  const fetchImpl = async (_url, i) => {
    init = i;
    return reply('{"findings":[]}');
  };
  await review("the prompt", { provider: "anthropic", apiKey: "secret-key", maxOutputTokens: 10, fetchImpl, sleepImpl: noSleep });
  assert.equal(init.headers["x-api-key"], "secret-key");
  assert.match(init.body, /the prompt/);
});

test("a provider with no credential fails before any request is made", async () => {
  let called = false;
  const fetchImpl = async () => {
    called = true;
    return reply("{}");
  };
  await assert.rejects(
    () => review("p", { provider: "anthropic", apiKey: null, maxOutputTokens: 10, fetchImpl, sleepImpl: noSleep }),
    /needs ANTHROPIC_API_KEY/,
  );
  assert.equal(called, false);
});

test("an unknown provider is refused rather than defaulted", async () => {
  await assert.rejects(() => review("p", { provider: "acme", apiKey: "k", maxOutputTokens: 10 }), /unknown provider/);
});

test("every provider names the credential it takes, and none is hard-wired as canonical", () => {
  assert.deepEqual(PROVIDERS, ["claude", "anthropic", "openai", "local"]);
  assert.equal(CREDENTIAL_ENV.local, null, "the local adapter prescribes no vendor and needs no key");
  // `claude` is the *default*, not the only option: a maintainer who wants the stronger
  // no-tool-loop guarantee of a raw API request can switch with one repository variable.
  assert.ok(PROVIDERS.includes("anthropic") && PROVIDERS.includes("openai") && PROVIDERS.includes("local"));
});

// --- retries: a transient outage is retried; a bad request is not ------------------------------

test("a transient failure is retried with backoff and can still succeed", async () => {
  const calls = [];
  const slept = [];
  let n = 0;
  const fetchImpl = async () => {
    n++;
    return n < 3 ? fail(503) : reply('{"findings":[]}');
  };
  const out = await review("p", {
    provider: "anthropic",
    apiKey: "k",
    maxOutputTokens: 10,
    fetchImpl,
    sleepImpl: async (ms) => slept.push(ms),
  });
  assert.equal(out.attempts, 3);
  assert.deepEqual(slept, [2000, 4000], "exponential backoff");
  calls.push(out);
});

test("a non-retryable failure is not retried — a 400 will be a 400 again", async () => {
  let n = 0;
  const fetchImpl = async () => {
    n++;
    return fail(400);
  };
  await assert.rejects(
    () => review("p", { provider: "anthropic", apiKey: "k", maxOutputTokens: 10, fetchImpl, sleepImpl: noSleep }),
    /after 1 attempt/,
  );
  assert.equal(n, 1);
});

test("exhausted retries are an infrastructure failure, never an empty review", async () => {
  const fetchImpl = async () => fail(503);
  await assert.rejects(
    () => review("p", { provider: "anthropic", apiKey: "k", maxOutputTokens: 10, fetchImpl, sleepImpl: noSleep }),
    (err) => err instanceof ProviderError && /after 3 attempt\(s\)/.test(err.message),
  );
});

test("a network error or timeout is retryable and surfaces as an outage", async () => {
  const fetchImpl = async () => {
    throw new Error("The operation was aborted due to timeout");
  };
  await assert.rejects(
    () => review("p", { provider: "anthropic", apiKey: "k", maxOutputTokens: 10, fetchImpl, sleepImpl: noSleep }),
    /timeout/,
  );
});

test("the model call is capped: retries never exceed the per-head-SHA call ceiling", async () => {
  let n = 0;
  const fetchImpl = async () => {
    n++;
    return fail(503);
  };
  await assert.rejects(
    () => review("p", { provider: "anthropic", apiKey: "k", maxOutputTokens: 10, maxAttempts: 99, fetchImpl, sleepImpl: noSleep }),
    /after 3 attempt/,
  );
  assert.equal(n, 3, "LIMITS.maxModelCalls is the ceiling even if maxAttempts is raised");
});

test("an empty model reply is retried, then reported as an outage", async () => {
  const fetchImpl = async () => ({ ok: true, status: 200, text: async () => JSON.stringify({ content: [] }) });
  await assert.rejects(
    () => review("p", { provider: "anthropic", apiKey: "k", maxOutputTokens: 10, fetchImpl, sleepImpl: noSleep }),
    /empty message/,
  );
});

// --- parsing what the model said ---------------------------------------------------------------

test("JSON is extracted from a fence, from prose, or from a bare object", () => {
  assert.deepEqual(parseModelJson('{"findings":[]}'), { findings: [] });
  assert.deepEqual(parseModelJson('```json\n{"findings":[]}\n```'), { findings: [] });
  assert.deepEqual(parseModelJson('Sure! Here you go:\n```\n{"findings":[]}\n```\nHope that helps.'), { findings: [] });
  assert.deepEqual(parseModelJson('Here is the result: {"findings":[{"title":"x"}]} done'), { findings: [{ title: "x" }] });
});

test("a reply with no JSON at all is an outage, not an empty review", () => {
  assert.throws(() => parseModelJson("I could not review this."), /no JSON object/);
  assert.throws(() => parseModelJson("{ not valid json"), /no JSON object/); // no closing brace
  assert.throws(() => parseModelJson('{"findings": }'), /did not parse/);
});

// --- the Claude Code CLI adapter: "no tools" must be enforced, not asserted -------------------
//
// This is the default provider (a Pro/Max subscription authenticates the CLI, not the metered
// API), and it is the one adapter where "the reviewer has no tools" is a claim about flags rather
// than a structural fact. So the flags, the environment, and the turn-count invariant are all
// pinned here: if any of them regressed, the reviewer would quietly become an agent with a
// credential, which is the exact thing ADR 0015 exists to prevent.

const cliResult = (over = {}) =>
  JSON.stringify({
    type: "result",
    subtype: "success",
    is_error: false,
    num_turns: 1,
    result: '{"findings":[]}',
    modelUsage: { "claude-opus-4-8": {} },
    ...over,
  });

/** A fake `execFile` that records how the CLI was invoked and replies with `stdout`. */
function fakeCli(stdout, { err = null } = {}) {
  const seen = {};
  const execImpl = (cmd, args, opts, cb) => {
    seen.cmd = cmd;
    seen.args = args;
    seen.opts = opts;
    setImmediate(() => cb(err, stdout, ""));
    return { stdin: { end: (p) => (seen.prompt = p) } };
  };
  return { execImpl, seen, mkdtempImpl: () => "/tmp/scratch-xyz" };
}

const runCli = (fake, over = {}) =>
  review("the prompt", {
    provider: "claude",
    apiKey: "oauth-token",
    maxOutputTokens: 100,
    execImpl: fake.execImpl,
    mkdtempImpl: fake.mkdtempImpl,
    sleepImpl: noSleep,
    ...over,
  });

test("the CLI is invoked with every built-in tool denied and MCP shut off", async () => {
  const fake = fakeCli(cliResult());
  await runCli(fake);

  const argv = fake.seen.args.join(" ");
  assert.match(argv, /--disallowed-tools/);
  for (const tool of ["Bash", "Edit", "Write", "Read", "Glob", "Grep", "WebFetch", "WebSearch", "Task"]) {
    assert.ok(argv.includes(tool), `${tool} must be denied`);
  }
  // Strict + empty: no MCP server from user settings or project config can add a tool back.
  assert.match(argv, /--strict-mcp-config/);
  assert.match(argv, /\{"mcpServers":\{\}\}/);
  assert.ok(fake.seen.args.includes("-p"), "print mode: non-interactive, one shot");
});

test("the CLI subprocess gets an environment allowlist with NO GitHub credential in it", async () => {
  process.env.GITHUB_TOKEN = "ghs_should_never_be_visible";
  process.env.RUNE_BOT_KEY = "/home/me/.config/rune/rune-agent.pem";
  try {
    const fake = fakeCli(cliResult());
    await runCli(fake);

    const env = fake.seen.opts.env;
    assert.deepEqual(Object.keys(env).sort(), ["CI", "CLAUDE_CODE_OAUTH_TOKEN", "HOME", "PATH"]);
    assert.equal(env.CLAUDE_CODE_OAUTH_TOKEN, "oauth-token");
    // The point: even a shell that escaped the denylist has nothing to push or authenticate with.
    assert.equal("GITHUB_TOKEN" in env, false);
    assert.equal("RUNE_BOT_KEY" in env, false);
  } finally {
    delete process.env.GITHUB_TOKEN;
    delete process.env.RUNE_BOT_KEY;
  }
});

test("the CLI runs in an empty scratch dir, so there is nothing for a tool to find", async () => {
  const fake = fakeCli(cliResult());
  await runCli(fake);

  assert.equal(fake.seen.opts.cwd, "/tmp/scratch-xyz", "not the repository — no CLAUDE.md to auto-discover");
  assert.equal(fake.seen.opts.env.HOME, "/tmp/scratch-xyz", "a scratch HOME: no user settings, no plugins");
});

test("the prompt goes in on stdin, not argv", async () => {
  const fake = fakeCli(cliResult());
  await runCli(fake);
  assert.equal(fake.seen.prompt, "the prompt");
  assert.equal(fake.seen.args.includes("the prompt"), false, "a 150KB prompt does not belong in argv");
});

test("a review that took more than one turn is REFUSED — a tool call costs a turn", async () => {
  // The invariant that turns "tools are denied" from a flag into an observation. If the CLI ever
  // stops honouring --disallowed-tools, the turn count is what tells us, and the right response is
  // to refuse the review rather than publish one produced by an agent with a credential.
  const fake = fakeCli(cliResult({ num_turns: 4 }));
  await assert.rejects(() => runCli(fake), /took 4 turns; a tool-less review is exactly one/);
});

test("a CLI error is an outage, not an empty review", async () => {
  await assert.rejects(() => runCli(fakeCli(cliResult({ is_error: true, subtype: "error_during_execution" }))), /reported/);
  await assert.rejects(() => runCli(fakeCli(cliResult({ result: "" }))), /empty result/);
  await assert.rejects(() => runCli(fakeCli("not json at all")), /did not return JSON/);
});

test("a CLI timeout is retryable; a bad invocation is not", async () => {
  const killed = Object.assign(new Error("timed out"), { killed: true });
  let calls = 0;
  const execImpl = (_c, _a, _o, cb) => {
    calls++;
    setImmediate(() => cb(killed, "", ""));
    return { stdin: { end() {} } };
  };
  await assert.rejects(
    () => review("p", { provider: "claude", apiKey: "t", execImpl, mkdtempImpl: () => "/tmp/x", sleepImpl: noSleep }),
    /after 3 attempt/,
  );
  assert.equal(calls, 3, "a timeout is transient, so it is retried");

  calls = 0;
  const badFlag = Object.assign(new Error("unknown option"), { code: 1 });
  const execImpl2 = (_c, _a, _o, cb) => {
    calls++;
    setImmediate(() => cb(badFlag, "", "bad flag"));
    return { stdin: { end() {} } };
  };
  await assert.rejects(
    () => review("p", { provider: "claude", apiKey: "t", execImpl: execImpl2, mkdtempImpl: () => "/tmp/x", sleepImpl: noSleep }),
    /after 1 attempt/,
  );
  assert.equal(calls, 1, "a broken invocation will break identically next time");
});

test("`claude` is the default provider and takes the subscription's OAuth token", () => {
  assert.equal(PROVIDERS[0], "claude");
  assert.equal(CREDENTIAL_ENV.claude, "CLAUDE_CODE_OAUTH_TOKEN");
});
