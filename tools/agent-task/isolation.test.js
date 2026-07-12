import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";

import { FORBIDDEN_ENV, providerEnv, resolveIsolation, scratchHomeFor, wrap } from "./isolation.js";
import { runDir } from "./runs.js";

let root;
const run = { run_id: "186-x", issue: 186, provider: "claude", branch: "agent/186-x" };

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "rune-iso-"));
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

test("with no isolation available, a run is refused rather than silently unprotected", () => {
  assert.throws(
    () => resolveIsolation({ providerUser: undefined, image: undefined }),
    /refusing to run a provider with no isolation/,
  );
});

test("--unsafe-same-uid is the only way to opt out, and it is never implicit", () => {
  const iso = resolveIsolation({ providerUser: undefined, image: undefined, unsafeSameUid: true });
  assert.equal(iso.mode, "same-uid");
});

test("the provider environment is built from an allowlist, so no credential can be inherited", () => {
  for (const name of FORBIDDEN_ENV) process.env[name] = "leak-me";
  process.env.AWS_SECRET_ACCESS_KEY = "leak-me-too";
  try {
    const env = providerEnv({ provider: "claude", workspace: "/w", run, root, scratchHome: "/h" });

    for (const name of [...FORBIDDEN_ENV, "AWS_SECRET_ACCESS_KEY"]) {
      assert.equal(env[name], undefined, `${name} must not reach the provider`);
    }
    assert.equal(Object.values(env).includes("leak-me"), false);
    assert.equal(env.HOME, "/h", "HOME must be the scratch dir, not the maintainer's home");
  } finally {
    for (const name of [...FORBIDDEN_ENV, "AWS_SECRET_ACCESS_KEY"]) delete process.env[name];
  }
});

test("a provider spawned with that environment genuinely cannot see the credentials", () => {
  // The real proof, not a unit-test proxy: spawn a process with the environment the runner
  // builds and let it try to read what it is forbidden. ADR 0016 requires this test.
  process.env.BOT_TOKEN = "ghs_thisisafaketokenvaluefortests12345";
  process.env.RUNE_BOT_KEY = "/home/someone/.config/rune/rune-agent.pem";
  try {
    const env = providerEnv({
      provider: "claude",
      workspace: root,
      run,
      root,
      scratchHome: scratchHomeFor(run, root),
    });

    const probe = join(root, "probe.sh");
    writeFileSync(probe, 'echo "TOKEN=[${BOT_TOKEN-unset}] KEY=[${RUNE_BOT_KEY-unset}] GH=[${GH_TOKEN-unset}]"\n');
    const out = execFileSync("bash", [probe], { env, encoding: "utf8" });

    assert.match(out, /TOKEN=\[unset\]/);
    assert.match(out, /KEY=\[unset\]/);
    assert.match(out, /GH=\[unset\]/);
  } finally {
    delete process.env.BOT_TOKEN;
    delete process.env.RUNE_BOT_KEY;
  }
});

test("a provider cannot reach the maintainer's gh login or git credentials", () => {
  const env = providerEnv({ provider: "claude", workspace: "/w", run, root, scratchHome: "/h" });
  assert.equal(env.GIT_CONFIG_GLOBAL, "/dev/null");
  assert.equal(env.GIT_CONFIG_SYSTEM, "/dev/null");
  assert.equal(env.GIT_TERMINAL_PROMPT, "0");
});

test("only the selected provider's model credential is passed through", () => {
  process.env.ANTHROPIC_API_KEY = "sk-ant-test";
  process.env.OPENAI_API_KEY = "sk-oai-test";
  try {
    const claude = providerEnv({ provider: "claude", workspace: "/w", run, root, scratchHome: "/h" });
    assert.equal(claude.ANTHROPIC_API_KEY, "sk-ant-test");
    assert.equal(claude.OPENAI_API_KEY, undefined, "codex's key must not reach claude");

    const codex = providerEnv({ provider: "codex", workspace: "/w", run: { ...run, provider: "codex" }, root, scratchHome: "/h" });
    assert.equal(codex.OPENAI_API_KEY, "sk-oai-test");
    assert.equal(codex.ANTHROPIC_API_KEY, undefined);
  } finally {
    delete process.env.ANTHROPIC_API_KEY;
    delete process.env.OPENAI_API_KEY;
  }
});

test("uid isolation runs the provider as another user with a reset environment", () => {
  const env = { HOME: "/h", PATH: "/usr/bin" };
  const { argv } = wrap(["claude", "-p", "brief"], {
    isolation: { mode: "uid", user: "rune-provider" },
    env,
    workspace: "/w",
    dir: runDir(run.run_id, root),
  });

  assert.deepEqual(argv.slice(0, 6), ["sudo", "-n", "-u", "rune-provider", "env", "-i"]);
  assert.ok(argv.includes("HOME=/h"));
  assert.deepEqual(argv.slice(-3), ["claude", "-p", "brief"]);
});

test("container isolation mounts only the run directory and the build cache, never the host home", () => {
  const dir = runDir(run.run_id, root);
  const { argv } = wrap(["codex", "exec", "brief"], {
    isolation: { mode: "container", engine: "podman", image: "rune/provider" },
    env: { HOME: join(dir, "home") },
    workspace: join(dir, "repo"),
    dir,
  });

  const mounts = argv.filter((_, i) => argv[i - 1] === "--volume");
  assert.equal(mounts.length, 2, mounts.join(", "));
  assert.ok(mounts.includes(`${dir}:${dir}`), "the run dir");
  assert.equal(
    argv.some((a) => a.includes(".config/rune")),
    false,
    "the key's directory must never be mounted",
  );
  assert.equal(
    argv.some((a) => a.includes("/.claude") || a.includes("/.config/gh")),
    false,
    "no provider login or gh login from the host either",
  );
});

test("the container uses the browser baked into the image, not a host path", () => {
  // `playwright install --with-deps` needs root to apt-get its system libraries, and the sandbox
  // is unprivileged by design — so the browser cannot be provisioned at run time. The image bakes
  // it in at /ms-playwright, and the host's PLAYWRIGHT_BROWSERS_PATH must not shadow it.
  const { argv } = wrap(["claude", "-p", "b"], {
    isolation: { mode: "container", engine: "docker", image: "rune/provider" },
    env: { HOME: "/h", PLAYWRIGHT_BROWSERS_PATH: "/home/someone/.cache/playwright" },
    workspace: "/w",
    dir: "/d",
  });

  assert.equal(
    argv.some((a) => a.startsWith("PLAYWRIGHT_BROWSERS_PATH=")),
    false,
  );
});

test("the container does not inherit the host's PATH", () => {
  // /home/you/.local/bin does not exist inside the image, so passing the host PATH leaves the
  // engine unable to resolve the provider binary at all — `docker run` fails with "executable
  // file not found in $PATH" before the provider even starts.
  const { argv } = wrap(["claude", "-p", "b"], {
    isolation: { mode: "container", engine: "docker", image: "rune/provider" },
    env: { PATH: "/home/someone/.local/bin:/usr/bin", HOME: "/h" },
    workspace: "/w",
    dir: "/d",
  });

  assert.equal(
    argv.some((a) => a.startsWith("PATH=")),
    false,
    "the image's own PATH must win",
  );
  assert.ok(argv.includes("HOME=/h"), "but the rest of the allowlist still goes in");
});

test("the container runs as the invoking user, not as root", () => {
  // As root, every file written into the mounted workspace is root-owned — and the runner, which
  // is not root, then cannot commit the work or clean the run up.
  const { argv } = wrap(["claude", "-p", "b"], {
    isolation: { mode: "container", engine: "docker", image: "rune/provider" },
    env: { HOME: "/h" },
    workspace: "/w",
    dir: "/d",
  });

  const user = argv[argv.indexOf("--user") + 1];
  assert.equal(user, `${process.getuid()}:${process.getgid()}`);
});

test("toolchain caches are shared across runs, since HOME is not", () => {
  const env = providerEnv({ provider: "claude", workspace: "/w", run, root, scratchHome: "/h", cache: join(root, "cache") });

  assert.equal(env.CARGO_HOME, join(root, "cache", "cargo"));
  assert.equal(env.NPM_CONFIG_CACHE, join(root, "cache", "npm"));
  assert.equal(env.PLAYWRIGHT_BROWSERS_PATH, join(root, "cache", "playwright"));
});

test("same-uid passes the argv through untouched", () => {
  const { argv, env } = wrap(["claude", "-p", "b"], {
    isolation: { mode: "same-uid" },
    env: { HOME: "/h" },
    workspace: "/w",
    dir: "/d",
  });
  assert.deepEqual(argv, ["claude", "-p", "b"]);
  assert.deepEqual(env, { HOME: "/h" });
});

test("the sudo wrapper does not fall back to the invoking user's environment", () => {
  // `sudo -u x cmd` would inherit parts of the caller's env; `env -i` is what makes the
  // allowlist actually hold on the other side of the UID boundary.
  const { argv, env } = wrap(["cmd"], {
    isolation: { mode: "uid", user: "p" },
    env: { HOME: "/h" },
    workspace: "/w",
    dir: "/d",
  });
  assert.ok(argv.includes("-i"));
  assert.deepEqual(env, {}, "nothing is passed through the spawn environment itself");
  assert.equal(spawnSync("true").status, 0);
});
