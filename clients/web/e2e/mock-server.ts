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
import { SAMPLE_GAME_VIEW_JSON } from '../src/game-view.fixture';
import {
  LOBBY_ROOMLESS_JSON,
  LOBBY_ROOM_DECKED_JSON,
  LOBBY_ROOM_UNDECKED_JSON,
} from '../src/lobby-view.fixture';

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

/**
 * Start a mock server that scripts the whole lobby handshake (ADR 0012), replying
 * to each `LobbyCommand` with the next canned `LobbyView` and finally the fixture
 * `GameView` when the player readies up — so the browser e2e drives the real
 * address → lobby → game flow (issue #114). Like {@link startMockServer} it holds
 * no logic: it maps a command `type` to a pre-baked frame from the shared
 * fixtures, so the client, this server, and the real server cannot silently
 * disagree about the lobby wire shape.
 */
export function startLobbyMockServer(): Promise<MockServer> {
  return new Promise((resolve) => {
    const received: string[] = [];
    const wss = new WebSocketServer({ host: '127.0.0.1', port: 0 });

    wss.on('connection', (socket: WebSocket) => {
      socket.on('message', (data) => {
        const raw = data.toString();
        received.push(raw);
        let type: string | undefined;
        try {
          type = (JSON.parse(raw) as { type?: string }).type;
        } catch {
          return;
        }
        switch (type) {
          case 'hello':
            socket.send(LOBBY_ROOMLESS_JSON);
            break;
          case 'create_room':
          case 'join_room':
            socket.send(LOBBY_ROOM_UNDECKED_JSON);
            break;
          case 'submit_deck':
            socket.send(LOBBY_ROOM_DECKED_JSON);
            break;
          case 'ready':
            // Every seat is filled, decked, and ready: the game is constructed and
            // the connection switches to the in-game GameView contract.
            socket.send(SAMPLE_GAME_VIEW_JSON);
            break;
          default:
            break;
        }
      });
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
