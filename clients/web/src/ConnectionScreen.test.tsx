import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ConnectionScreen, DEFAULT_SERVER_URL } from './ConnectionScreen';
import { useGameStore } from './store';

/**
 * These tests render the real store singleton and swap `connect`/`disconnect` for
 * spies (the pattern Table.test.tsx uses for `choose`), so each connection state
 * is asserted purely from what the store exposes — no sockets, no game logic.
 */
function withStore(status: 'idle' | 'connecting' | 'closed'): {
  connect: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
} {
  const connect = vi.fn();
  const disconnect = vi.fn();
  useGameStore.setState({ status, view: null, connect, disconnect });
  return { connect, disconnect };
}

afterEach(() => {
  cleanup();
  useGameStore.setState({ status: 'idle', view: null });
});

describe('ConnectionScreen (front-door landing)', () => {
  it('leads with Play and connects to the default server without opening settings', () => {
    // The blueprint's front door: the address is a default + advanced affordance,
    // never a form the player must fill before playing.
    const { connect } = withStore('idle');
    render(<ConnectionScreen />);

    const play = screen.getByTestId('connect-button');
    expect(play.textContent).toBe('Play');
    fireEvent.click(play);

    expect(connect).toHaveBeenCalledTimes(1);
    expect(connect).toHaveBeenCalledWith(DEFAULT_SERVER_URL, { autoReconnect: false });
  });

  it('tucks the server address behind a settings disclosure, pre-filled and closed', () => {
    withStore('idle');
    render(<ConnectionScreen />);

    const settings = screen.getByTestId('server-settings') as HTMLDetailsElement;
    expect(settings.open).toBe(false);

    const inputEl = screen.getByTestId('server-url') as HTMLInputElement;
    expect(inputEl.value).toBe(DEFAULT_SERVER_URL);
  });

  it('connects with an edited address from server settings', () => {
    const { connect } = withStore('idle');
    render(<ConnectionScreen />);

    fireEvent.change(screen.getByTestId('server-url'), { target: { value: 'ws://host:1234' } });
    fireEvent.click(screen.getByTestId('connect-button'));

    expect(connect).toHaveBeenCalledTimes(1);
    expect(connect).toHaveBeenCalledWith('ws://host:1234', { autoReconnect: false });
  });

  it('trims whitespace and ignores an empty address', () => {
    const { connect } = withStore('idle');
    render(<ConnectionScreen />);

    fireEvent.change(screen.getByTestId('server-url'), { target: { value: '   ' } });
    fireEvent.click(screen.getByTestId('connect-button'));
    expect(connect).not.toHaveBeenCalled();

    fireEvent.change(screen.getByTestId('server-url'), { target: { value: '  ws://x:9 ' } });
    fireEvent.click(screen.getByTestId('connect-button'));
    expect(connect).toHaveBeenCalledWith('ws://x:9', { autoReconnect: false });
  });

  it('shows a connecting indicator and a Cancel that disconnects', () => {
    const { disconnect } = withStore('connecting');
    render(<ConnectionScreen />);

    expect(screen.getByTestId('connection-status').textContent).toContain('Opening a connection');
    // No dead screen: there is always an action available.
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(disconnect).toHaveBeenCalledTimes(1);
  });

  it('surfaces a closed connection as a retryable error with server settings opened', () => {
    const { connect } = withStore('closed');
    render(<ConnectionScreen />);

    expect(screen.getByRole('alert').textContent).toContain('Connection closed');
    const retry = screen.getByTestId('connect-button');
    expect(retry.textContent).toBe('Retry');

    // The address is the likely fix, so the disclosure auto-opens on failure and
    // the URL stays editable before retrying.
    const settings = screen.getByTestId('server-settings') as HTMLDetailsElement;
    expect(settings.open).toBe(true);
    fireEvent.change(screen.getByTestId('server-url'), { target: { value: 'ws://retry:9000' } });
    fireEvent.click(retry);
    expect(connect).toHaveBeenCalledWith('ws://retry:9000', { autoReconnect: false });
  });

  it('renders RUNE identity procedurally — a wordmark and an SVG mark, no image assets (#300)', () => {
    withStore('idle');
    const { container } = render(<ConnectionScreen />);

    // The wordmark carries the accessible product name…
    expect(screen.getByRole('heading', { name: 'RUNE' })).toBeDefined();
    // …and the motif is procedural geometry (an inline SVG), never a bundled image.
    expect(container.querySelector('svg')).not.toBeNull();
    expect(container.querySelector('img')).toBeNull();
  });

  it('keeps the three connection states visually distinct (#300)', () => {
    // Each lifecycle state advertises a distinct status; the closed one is an alert.
    withStore('idle');
    const idle = render(<ConnectionScreen />);
    expect(screen.getByTestId('connection-status').textContent).toContain('Ready to play');
    expect(screen.queryByRole('alert')).toBeNull();
    idle.unmount();

    withStore('connecting');
    const connecting = render(<ConnectionScreen />);
    expect(screen.getByTestId('connection-status').textContent).toContain('Opening a connection');
    expect(screen.queryByRole('alert')).toBeNull();
    connecting.unmount();

    withStore('closed');
    render(<ConnectionScreen />);
    // The closed state is the only one announced to assistive tech via role=alert.
    expect(screen.getByRole('alert').textContent).toContain('Connection closed');
  });
});
