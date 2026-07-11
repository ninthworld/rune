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

describe('ConnectionScreen', () => {
  it('renders the URL entry pre-filled from the env default when idle', () => {
    withStore('idle');
    render(<ConnectionScreen />);

    const inputEl = screen.getByTestId('server-url') as HTMLInputElement;
    expect(inputEl.value).toBe(DEFAULT_SERVER_URL);
    expect(screen.getByTestId('connect-button').textContent).toBe('Connect');
  });

  it('connects with the entered URL and no auto-reconnect', () => {
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

  it('surfaces a closed connection as a retryable error with an editable URL', () => {
    const { connect } = withStore('closed');
    render(<ConnectionScreen />);

    expect(screen.getByRole('alert').textContent).toContain('Connection closed');
    const retry = screen.getByTestId('connect-button');
    expect(retry.textContent).toBe('Retry');

    // The URL stays editable so the user can fix a bad address before retrying.
    fireEvent.change(screen.getByTestId('server-url'), { target: { value: 'ws://retry:9000' } });
    fireEvent.click(retry);
    expect(connect).toHaveBeenCalledWith('ws://retry:9000', { autoReconnect: false });
  });
});
