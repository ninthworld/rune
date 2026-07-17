/**
 * Test harness: launch and tear down a real `rune-server` for the smoke canary.
 *
 * The spec drives the actual server binary (not a mock) so it exercises the true
 * end-to-end path build → browser → socket → server → engine. The server is bound
 * to an **ephemeral** port (`--addr 127.0.0.1:0`) so parallel runs never collide,
 * and to a **pinned shuffle seed** (`--rng-seed`, ADR 0014) so the opening hands —
 * and therefore the land the test plays — are fully deterministic.
 *
 * The bound port is discovered by reading the server's own startup log line
 * (`rune-server listening addr=127.0.0.1:<port>`) off stderr, rather than guessing
 * a port, which is what keeps the harness race-free.
 */
import { type ChildProcess, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));

/** Resolve the prebuilt server binary (built by `make smoke`; override with env). */
function serverBinary(): string {
  const override = process.env.RUNE_SERVER_BIN;
  if (override) return override;
  // clients/web/e2e -> repo root -> target/debug/rune-server
  return resolve(HERE, '../../../target/debug/rune-server');
}

/** A running server under test: the ws URL to connect to and a teardown hook. */
export interface RunningServer {
  /** `ws://127.0.0.1:<port>` — type this into the client's connection screen. */
  url: string;
  /** Terminate the server process (idempotent). */
  close: () => Promise<void>;
}

/**
 * Start a seeded `rune-server` on an ephemeral port and resolve once it is
 * listening. The pinned seed makes the game deterministic across runs.
 */
export async function startServer(seed: number): Promise<RunningServer> {
  const bin = serverBinary();
  if (!existsSync(bin)) {
    throw new Error(
      `rune-server binary not found at ${bin}. Build it first: cargo build -p rune-server ` +
        `(the \`make smoke\` target does this for you).`,
    );
  }

  const child: ChildProcess = spawn(bin, ['--addr', '127.0.0.1:0', '--rng-seed', String(seed)], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, RUST_LOG: 'info' },
  });

  const port = await new Promise<number>((resolvePort, rejectPort) => {
    let buffer = '';
    const timer = setTimeout(() => {
      rejectPort(new Error(`server did not report a listen address in time:\n${buffer}`));
    }, 15_000);

    const onData = (chunk: Buffer): void => {
      buffer += chunk.toString();
      const match = buffer.match(/listening.*?127\.0\.0\.1:(\d+)/);
      if (match) {
        clearTimeout(timer);
        child.stderr?.off('data', onData);
        child.stdout?.off('data', onData);
        resolvePort(Number(match[1]));
      }
    };

    child.stderr?.on('data', onData);
    child.stdout?.on('data', onData);
    child.once('error', (error) => {
      clearTimeout(timer);
      rejectPort(error);
    });
    child.once('exit', (code) => {
      clearTimeout(timer);
      rejectPort(new Error(`server exited early (code ${code}) before listening:\n${buffer}`));
    });
  });

  const close = (): Promise<void> =>
    new Promise((done) => {
      if (child.exitCode !== null || child.signalCode !== null) {
        done();
        return;
      }
      child.once('exit', () => done());
      child.kill('SIGKILL');
    });

  return { url: `ws://127.0.0.1:${port}`, close };
}
