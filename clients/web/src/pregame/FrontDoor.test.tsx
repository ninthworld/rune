/**
 * The front door's reconnect copy (issue #506; `front-door-and-lobby.md` §5.1,
 * §8 criterion 14) — P11's fix.
 *
 * A returning player used to see the same generic "Opening a connection to …"
 * as a cold start. The socket lifecycle is unchanged; only the words are, and
 * only when `restoreSession()` actually had a stored session for this address.
 *
 * The rest of the front door's behavior is covered by the shipped
 * `ConnectionScreen.test.tsx`, which passes unmigrated through the restyle.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ConnectionScreen } from '../ConnectionScreen';
import { useGameStore, type SocketFactory } from '../store';

const TOKEN_KEY = 'rune.session.token';
const URL_KEY = 'rune.session.url';

/** A socket that never opens, so the screen stays in `connecting`. */
class PendingSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  send(): void {}
  close(): void {
    this.onclose?.({});
  }
}

const factory: SocketFactory = () => new PendingSocket() as unknown as WebSocket;

beforeEach(() => sessionStorage.clear());

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({
    status: 'idle',
    view: null,
    lobby: null,
    reclaimingSession: false,
    lastMatch: null,
    // The front door reads this now, so a leaked address from a prior test would
    // pre-fill the next one's field.
    serverUrl: null,
  });
  sessionStorage.clear();
});

describe('Front door — criterion 14: reclaiming vs connecting', () => {
  it('reads “Reclaiming your seat” when restoreSession had a stored session', () => {
    sessionStorage.setItem(TOKEN_KEY, 's:held');
    sessionStorage.setItem(URL_KEY, 'ws://seat:9000');

    act(() => {
      useGameStore.getState().restoreSession({ createSocket: factory, autoReconnect: false });
    });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('connection-status').textContent).toContain('Reclaiming your seat');
    // The pill keeps the connecting treatment; only the word changes.
    expect(screen.getByTestId('connection-screen').textContent).toContain('Reclaiming');
    // Still never a dead screen.
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDefined();
  });

  it('reads “Opening a connection” for an ordinary, user-driven connect', () => {
    act(() => {
      useGameStore.getState().connect('ws://fresh:9000', {
        createSocket: factory,
        autoReconnect: false,
      });
    });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('connection-status').textContent).toContain('Opening a connection');
    expect(screen.getByTestId('connection-status').textContent).not.toContain('Reclaiming');
  });

  it('reads “Returning to the lobby” while #452’s postgame exit reconnects', () => {
    // The exit closes the bridged match socket and reopens the server, so the
    // landing is reached ACROSS a reconnect (front-door-and-lobby §2, as
    // shipped). The front door is passed through and says so rather than
    // reading as a detour. Derived from the pending record — no extra state.
    act(() => {
      useGameStore.getState().connect('ws://back:9000', {
        createSocket: factory,
        autoReconnect: false,
      });
      useGameStore
        .getState()
        .recordLastMatch({ outcome: 'victory', opponents: ['Bob'], gameSetup: '1v1', seats: 2 });
    });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('connection-status').textContent).toContain('Returning to the lobby');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDefined();
  });

  it('drops the reclaim state as soon as the socket answers', () => {
    sessionStorage.setItem(TOKEN_KEY, 's:held');
    sessionStorage.setItem(URL_KEY, 'ws://seat:9000');
    act(() => {
      useGameStore.getState().restoreSession({ createSocket: factory, autoReconnect: false });
    });
    expect(useGameStore.getState().reclaimingSession).toBe(true);

    // Where the reclaimed session lands is entirely the server's answer; the
    // client's reclaim copy is over the moment the transport resolves.
    act(() => useGameStore.getState().disconnect());
    expect(useGameStore.getState().reclaimingSession).toBe(false);
  });
});

describe('Front door — one blue primary, and settings preserved (#546, criterion 18)', () => {
  it('spends the one blue primary on Connect and keeps the settings handle', () => {
    useGameStore.setState({ status: 'idle', view: null, lobby: null });
    const { container } = render(<ConnectionScreen />);

    // `control-language.md` §4.1: at most one blue primary on screen. The
    // variant is drawn by `ControlButton`, so counting the family's own
    // `data-variant` marker is counting the thing the player sees.
    const primaries = container.querySelectorAll('[data-variant="primary"]');
    expect(primaries).toHaveLength(1);
    expect(primaries[0]!.getAttribute('data-testid')).toBe('connect-button');
    expect(screen.getByTestId('connect-button').textContent).toBe('Connect');
    // The #505 device-local settings path survives the restyle, now as the
    // bottom-right handle every baseline draws.
    expect(screen.getByTestId('front-door-settings')).toBeDefined();
  });

  it('states which server the one action connects to, and hides the address', () => {
    // "The default server is already selected; ordinary players do not configure
    // anything" — so the plaque names it and the address stays behind Change
    // server, closed.
    useGameStore.setState({ status: 'idle', view: null, lobby: null });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('server-name').textContent).toBe('Default Server');
    expect((screen.getByTestId('server-settings') as HTMLDetailsElement).open).toBe(false);
  });

  it('names a custom address rather than calling it the default server', () => {
    useGameStore.setState({ status: 'idle', view: null, lobby: null, serverUrl: null });
    render(<ConnectionScreen />);

    fireEvent.change(screen.getByTestId('server-url'), { target: { value: 'ws://elsewhere:9' } });
    expect(screen.getByTestId('server-name').textContent).toBe('ws://elsewhere:9');
  });
});

/**
 * The field Retry sends to has to be the server the session is actually on.
 *
 * A reclaim (`restoreSession`) and a postgame return (#452) both reopen the
 * stored address without going through this field, so a custom server was
 * displayed as "Default Server" — and after a failure, Retry sent the BUILD
 * DEFAULT rather than the address that had just failed.
 */
describe('Front door — the address follows the connection', () => {
  it('adopts the address the live connection was opened against', () => {
    useGameStore.setState({ status: 'closed', view: null, lobby: null, serverUrl: 'ws://mine:7' });
    render(<ConnectionScreen />);

    expect(screen.getByTestId('server-name').textContent).toBe('ws://mine:7');
    expect((screen.getByTestId('server-url') as HTMLInputElement).value).toBe('ws://mine:7');
  });

  it('retries the address that failed, not the build default', () => {
    const opened: string[] = [];
    const realConnect = useGameStore.getState().connect;
    useGameStore.setState({
      status: 'closed',
      view: null,
      lobby: null,
      serverUrl: 'ws://mine:7',
      connect: (url: string) => {
        opened.push(url);
      },
    } as never);
    render(<ConnectionScreen />);

    fireEvent.click(screen.getByTestId('connect-button'));
    useGameStore.setState({ connect: realConnect } as never);
    expect(opened).toEqual(['ws://mine:7']);
  });

  it('never overwrites an address the player is typing', () => {
    useGameStore.setState({ status: 'idle', view: null, lobby: null, serverUrl: null });
    render(<ConnectionScreen />);

    fireEvent.change(screen.getByTestId('server-url'), { target: { value: 'ws://typed:1' } });
    // A connection resolving underneath — the reclaim path setting `serverUrl` —
    // must not pull the field out from under the edit in progress.
    act(() => useGameStore.setState({ serverUrl: 'ws://elsewhere:2' }));

    expect((screen.getByTestId('server-url') as HTMLInputElement).value).toBe('ws://typed:1');
    expect(screen.getByTestId('server-name').textContent).toBe('ws://typed:1');
  });
});
