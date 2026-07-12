import assert from "node:assert/strict";
import { test } from "node:test";

import { adapterFor, permissionMode } from "./adapters.js";
import { PROVIDER_CREDENTIALS, hasCredential } from "./isolation.js";

test("a contained provider gets bypassPermissions, so it can actually run the tests", () => {
  // acceptEdits auto-approves edits but still asks before arbitrary Bash — and in print mode
  // there is nobody to ask, so `make check` is simply denied and the run limps to a failure.
  for (const mode of ["container", "uid"]) {
    assert.equal(permissionMode({ mode }), "bypassPermissions");
  }
});

test("an uncontained provider does not get bypassPermissions", () => {
  // Under --unsafe-same-uid the provider is running as the maintainer, with the app's private key
  // readable. Unattended bypassPermissions there is the exact thing ADR 0016 exists to prevent.
  assert.equal(permissionMode({ mode: "same-uid" }), "acceptEdits");
  assert.equal(permissionMode(undefined), "bypassPermissions");
});

test("the claude adapter does not use --bare", () => {
  const argv = adapterFor("claude").argv("the brief", { isolation: { mode: "container" } });

  // --bare skips auto-discovery of CLAUDE.md, which is how root and nested AGENTS.md reach the
  // model at all. ADR 0016 requires that behaviour be preserved.
  assert.equal(argv.includes("--bare"), false);
  assert.deepEqual(argv, ["claude", "-p", "the brief", "--permission-mode", "bypassPermissions"]);
});

test("the codex adapter runs non-interactively", () => {
  assert.deepEqual(adapterFor("codex").argv("brief"), ["codex", "exec", "--full-auto", "brief"]);
});

test("the local adapter demands a command rather than assuming a harness", () => {
  delete process.env.RUNE_LOCAL_CMD;
  assert.throws(() => adapterFor("local").argv("brief"), /RUNE_LOCAL_CMD/);

  process.env.RUNE_LOCAL_CMD = "my-harness";
  try {
    assert.deepEqual(adapterFor("local").argv("brief"), ["bash", "-c", "my-harness"]);
  } finally {
    delete process.env.RUNE_LOCAL_CMD;
  }
});

test("an unknown provider is rejected", () => {
  assert.throws(() => adapterFor("gpt"), /unknown provider/);
});

test("a token, not an interactive login, is what authenticates a sandboxed provider", () => {
  // The sandbox replaces HOME, so ~/.claude — where `/login` puts its session — is out of reach.
  // That is the same isolation that hides the rune-agent key, so it is a cost worth paying, but
  // it does mean a headless run needs a token in the environment.
  assert.ok(PROVIDER_CREDENTIALS.claude.includes("CLAUDE_CODE_OAUTH_TOKEN"), "claude setup-token's output");
  assert.ok(PROVIDER_CREDENTIALS.claude.includes("ANTHROPIC_API_KEY"));

  delete process.env.CLAUDE_CODE_OAUTH_TOKEN;
  delete process.env.ANTHROPIC_API_KEY;
  delete process.env.ANTHROPIC_AUTH_TOKEN;
  assert.equal(hasCredential("claude"), false);

  process.env.CLAUDE_CODE_OAUTH_TOKEN = "sk-ant-oat-test";
  try {
    assert.equal(hasCredential("claude"), true);
  } finally {
    delete process.env.CLAUDE_CODE_OAUTH_TOKEN;
  }
});
