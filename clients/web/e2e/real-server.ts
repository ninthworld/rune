/**
 * The real `rune-server` backend for the browser e2e smoke tier (ADR 0011).
 *
 * The mock tier ({@link ./mock-server}) replays canned frames and is the fast
 * default; this helper is its opposite number — it launches the **actual**
 * `rune-server` binary so a handful of smoke tests can prove the true end-to-end
 * path (built client → real browser → socket → real server → real protocol),
 * catching the mock-vs-reality drift the mock tier cannot. It holds no game logic:
 * it only starts, addresses, and stops a real server process.
 *
 * The server binds an **ephemeral** loopback port (`--addr 127.0.0.1:0`) so
 * parallel runs never collide and nothing is hardcoded; the harness learns the
 * real port by parsing the server's own "listening" log line (deterministic — it
 * waits for that line rather than sleeping). The process is started and stopped by
 * the harness, and its captured logs are exposed so a failing test can attach them
 * for diagnosis.
 */
import { type ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

/** A running real `rune-server`: its `ws://` address, its logs, and a stop handle. */
export interface RealServer {
  /** The `ws://127.0.0.1:<port>` address the browser client connects to. */
  url: string;
  /** Everything the server has written to stdout/stderr so far (for diagnostics). */
  logs: () => string;
  /** Stop the server process and wait for it to exit. */
  close: () => Promise<void>;
}

/** How long to wait for the server to announce its listening port before failing. */
const STARTUP_TIMEOUT_MS = 15_000;

/**
 * Resolve the `rune-server` binary path: the `RUNE_SERVER_BIN` override if set,
 * else the workspace debug build at `<repo>/target/debug/rune-server`. The e2e job
 * (and `make e2e`) build it with `cargo build -p rune-server` first; a clear error
 * here beats an opaque spawn failure when that step was skipped.
 */
function resolveServerBinary(): string {
  const override = process.env.RUNE_SERVER_BIN;
  if (override) return override;
  // This file lives in clients/web/e2e/, so the repo root is three levels up.
  const binary = fileURLToPath(new URL('../../../target/debug/rune-server', import.meta.url));
  if (!existsSync(binary)) {
    throw new Error(
      `rune-server binary not found at ${binary}. Build it first with ` +
        '`cargo build -p rune-server` (or set RUNE_SERVER_BIN to its path).',
    );
  }
  return binary;
}

/** Strip ANSI colour escapes so the port regex matches regardless of `NO_COLOR`. */
function stripAnsi(text: string): string {
  // eslint-disable-next-line no-control-regex -- matching the CSI escape prefix.
  return text.replace(/\[[0-9;]*m/g, '');
}

/**
 * Launch the real `rune-server` on an ephemeral loopback port and resolve once it
 * is listening. Rejects (killing the process) if it neither announces a port nor
 * comes up within {@link STARTUP_TIMEOUT_MS}. The caller owns the returned handle
 * and must {@link RealServer.close} it.
 */
export function startRealServer(): Promise<RealServer> {
  const binary = resolveServerBinary();
  // Bind port 0: the OS assigns a free port, which the server logs back to us.
  const child: ChildProcessWithoutNullStreams = spawn(binary, ['--addr', '127.0.0.1:0'], {
    // `NO_COLOR` keeps the log line ANSI-free (we also strip defensively);
    // `RUST_LOG=info` guarantees the "listening" line the port is parsed from.
    env: { ...process.env, NO_COLOR: '1', RUST_LOG: process.env.RUST_LOG ?? 'info' },
  });

  let buffer = '';
  const record = (chunk: Buffer): void => {
    buffer += chunk.toString();
  };
  child.stdout.on('data', record);
  child.stderr.on('data', record);

  const close = (): Promise<void> =>
    new Promise((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) {
        resolve();
        return;
      }
      child.once('exit', () => resolve());
      child.kill('SIGKILL');
    });

  return new Promise<RealServer>((resolve, reject) => {
    const fail = async (message: string): Promise<void> => {
      clearTimeout(timer);
      await close();
      reject(new Error(`${message}\n--- rune-server logs ---\n${buffer}`));
    };

    const timer = setTimeout(() => {
      void fail('rune-server did not announce a listening port in time');
    }, STARTUP_TIMEOUT_MS);

    const scan = (): void => {
      const match = stripAnsi(buffer).match(/rune-server listening[\s\S]*?127\.0\.0\.1:(\d+)/);
      if (!match) return;
      clearTimeout(timer);
      child.stdout.off('data', scan);
      child.stderr.off('data', scan);
      resolve({
        url: `ws://127.0.0.1:${match[1]}`,
        logs: () => buffer,
        close,
      });
    };
    child.stdout.on('data', scan);
    child.stderr.on('data', scan);

    child.once('error', (error) => {
      void fail(`failed to spawn rune-server: ${error.message}`);
    });
    child.once('exit', (code, signal) => {
      // An early exit before the port line means the server never came up.
      if (!buffer.includes('rune-server listening')) {
        void fail(`rune-server exited early (code=${code}, signal=${signal})`);
      }
    });
  });
}
