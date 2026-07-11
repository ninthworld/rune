/**
 * The mock WebSocket backend for the browser e2e suite (ADR 0011, "default tier").
 *
 * A tiny in-process `ws` server that accepts the client's connection and pushes a
 * single canned `GameView` frame — no engine, no game, no room lifecycle. This is
 * the deterministic, fast tier: "given this exact GameView, the browser paints
 * this." It replays the existing fixtures from `src/game-view.fixture.ts` so the
 * unit tests, these e2e tests, and (by construction) the real server cannot
 * silently disagree about the wire shape.
 *
 * It is a fixture replayer, never a game: it holds no logic and only echoes the
 * frame it was handed. Any `ChooseAction` the client sends back is captured for
 * assertion but not interpreted.
 */
import { AddressInfo } from 'node:net';
import { WebSocketServer, type WebSocket } from 'ws';

/** A running mock server: its `ws://` address and a lifecycle handle. */
export interface MockServer {
  /** The `ws://127.0.0.1:<port>` address the client connects to. */
  url: string;
  /** Raw text frames the client has sent back (e.g. `ChooseAction`). */
  received: string[];
  /** Close the server and drop all connections. */
  close: () => Promise<void>;
}

/**
 * Start a mock WS server that sends `frameJson` to every client on connect.
 * Binds to an ephemeral port on loopback so tests never collide, and resolves
 * once the port is actually listening (deterministic; callers never sleep).
 */
export function startMockServer(frameJson: string): Promise<MockServer> {
  return new Promise((resolve) => {
    const received: string[] = [];
    const wss = new WebSocketServer({ host: '127.0.0.1', port: 0 });

    wss.on('connection', (socket: WebSocket) => {
      socket.on('message', (data) => received.push(data.toString()));
      // Push the canned frame immediately; the client rebuilds its whole UI from
      // this single GameView (the reconstruct-from-one-GameView invariant).
      socket.send(frameJson);
    });

    wss.on('listening', () => {
      const { port } = wss.address() as AddressInfo;
      resolve({
        url: `ws://127.0.0.1:${port}`,
        received,
        close: () =>
          new Promise((done) => {
            for (const client of wss.clients) client.terminate();
            wss.close(() => done());
          }),
      });
    });
  });
}
