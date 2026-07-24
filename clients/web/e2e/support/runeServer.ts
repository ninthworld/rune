/**
 * Launcher for a real `rune-server` process (ADR 0011's smoke tier, issue #279).
 *
 * The smoke spec drives the shipped client against the *actual* server binary,
 * not a mock, so the browser → socket → engine path is what is under test. This
 * module owns the process lifecycle only; it knows nothing about the game.
 *
 * Determinism (`crates/rune-server/src/lib.rs`, ADR 0014): the server is started
 * with a pinned `--rng-seed`, so every run shuffles the same starter decks the
 * same way, and a pinned `--starting-life`, so a game can never run long. Both
 * are server/operator flags — deliberately not client-settable — so nothing in
 * the client is aware the run is scripted.
 *
 * The listen port is `0` (OS-assigned): parallel workers and a busy CI runner
 * can never collide on a fixed port. The real port is read back off the server's
 * own startup log line rather than guessed, and the launcher waits on that line
 * — never on a timer.
 */
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/** Repository root, resolved from this file (`<root>/clients/web/e2e/support`). */
const REPO_ROOT = fileURLToPath(new URL('../../../..', import.meta.url));

/** Default location of the debug binary `make e2e` builds. */
const DEFAULT_BINARY = `${REPO_ROOT}target/debug/rune-server`;

/**
 * Fixed shuffle seed for every game in the suite. Any value works; it is pinned
 * so a failure reproduces exactly.
 */
export const RNG_SEED = 279;

/** Fixed starting life: low enough that no scripted game can outlive its budget. */
export const STARTING_LIFE = 5;

/** What a launch may pin, per spec. Both are server/operator flags, never
 * client-settable, so nothing in the client is aware the run is scripted. */
export interface RuneServerOptions {
  /** Engine shuffle seed (ADR 0014). Defaults to {@link RNG_SEED}. */
  rngSeed?: number;
  /** Starting life for every seat. Defaults to {@link STARTING_LIFE}. */
  startingLife?: number;
}

/** How long to wait for the server to announce its listen address. */
const STARTUP_TIMEOUT_MS = 30_000;

/** A running server process plus the address a client should connect to. */
export interface RuneServerProcess {
  /** `ws://127.0.0.1:<port>` — what a player types into Server settings. */
  readonly url: string;
  /** Everything the process wrote, for failure diagnostics. */
  readonly log: () => string;
  /** Terminate the process and wait for it to exit. */
  readonly stop: () => Promise<void>;
}

/** Strip ANSI colour codes so the log is greppable regardless of TTY detection. */
// eslint-disable-next-line no-control-regex
const ANSI = /\[[0-9;]*m/g;

/**
 * Start `rune-server` on an OS-assigned port and resolve once it has logged the
 * address it bound. Rejects (after tearing the process down) if the binary is
 * missing, exits early, or never announces an address.
 *
 * Set `RUNE_SERVER_BIN` to point at a different build (e.g. a release binary).
 */
export async function startRuneServer(
  options: RuneServerOptions = {},
): Promise<RuneServerProcess> {
  const rngSeed = options.rngSeed ?? RNG_SEED;
  const startingLife = options.startingLife ?? STARTING_LIFE;
  const binary = process.env.RUNE_SERVER_BIN ?? DEFAULT_BINARY;
  if (!existsSync(binary)) {
    throw new Error(
      `rune-server binary not found at ${binary}. Run \`make e2e\` (which builds it) or set RUNE_SERVER_BIN.`,
    );
  }

  // `stdio` below pipes both output streams, so `child.stdout`/`child.stderr`
  // are non-null; the inferred type carries that.
  const child = spawn(
    binary,
    [
      '--addr',
      '127.0.0.1:0',
      '--rng-seed',
      String(rngSeed),
      '--starting-life',
      String(startingLife),
    ],
    {
      cwd: REPO_ROOT,
      // NO_COLOR keeps the startup line plain; RUST_LOG=info is what the address
      // line is emitted at.
      env: { ...process.env, NO_COLOR: '1', RUST_LOG: process.env.RUST_LOG ?? 'info' },
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );

  let output = '';
  const record = (chunk: Buffer): void => {
    output += chunk.toString().replace(ANSI, '');
  };
  child.stdout.on('data', record);
  child.stderr.on('data', record);

  const stop = async (): Promise<void> => {
    if (child.exitCode !== null || child.signalCode !== null) return;
    await new Promise<void>((resolve) => {
      child.once('exit', () => resolve());
      child.kill('SIGTERM');
    });
  };

  const address = await new Promise<string>((resolve, reject) => {
    const timer = setTimeout(() => {
      finish(() =>
        reject(new Error(`rune-server did not start in ${STARTUP_TIMEOUT_MS}ms:\n${output}`)),
      );
    }, STARTUP_TIMEOUT_MS);

    const finish = (settle: () => void): void => {
      clearTimeout(timer);
      child.stdout.off('data', check);
      child.stderr.off('data', check);
      child.off('exit', onExit);
      child.off('error', onError);
      settle();
    };

    // Wait on the condition (the announced address), never on a sleep.
    function check(): void {
      const match = /rune-server listening.*?addr=([0-9.]+:\d+)/.exec(output);
      if (match) finish(() => resolve(match[1]!));
    }
    function onExit(code: number | null): void {
      finish(() => reject(new Error(`rune-server exited early (code ${code}):\n${output}`)));
    }
    function onError(error: Error): void {
      finish(() => reject(error));
    }

    child.stdout.on('data', check);
    child.stderr.on('data', check);
    child.on('exit', onExit);
    child.on('error', onError);
    check();
  }).catch(async (error: Error) => {
    await stop();
    throw error;
  });

  return { url: `ws://${address}`, log: () => output, stop };
}
