/**
 * The shared stage's contract (issue #506; `front-door-and-lobby.md` §8
 * criteria 2, 4, 17).
 *
 * The stage is what makes the crossing into the match invisible, so the thing
 * worth pinning is node *identity*: the environment must be the same element
 * from the front door through the room. A re-mount would be invisible in a
 * screenshot and fatal to the intent.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { App } from './../App';
import { useGameStore, type SocketFactory } from '../store';
import { LOBBY_ROOMLESS_JSON, LOBBY_ROOM_DECKED_JSON } from '../lobby-view.fixture';
import { setQuality } from '../table/settings/presentationSettings';
import { PregameStage } from './PregameStage';

/** A manually-driven WebSocket stand-in (the shape the other suites use). */
class FakeSocket {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: unknown) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  send(): void {}
  close(): void {
    this.onclose?.({});
  }
  emitOpen(): void {
    this.onopen?.({});
  }
  emitMessage(data: string): void {
    this.onmessage?.({ data });
  }
}

function recordingFactory(): { factory: SocketFactory; sockets: FakeSocket[] } {
  const sockets: FakeSocket[] = [];
  const factory: SocketFactory = () => {
    const s = new FakeSocket();
    sockets.push(s);
    return s as unknown as WebSocket;
  };
  return { factory, sockets };
}

afterEach(() => {
  cleanup();
  act(() => useGameStore.getState().disconnect());
  useGameStore.setState({ status: 'idle', view: null, lobby: null, lobbyError: null });
  setQuality('standard');
});

describe('PregameStage — criterion 2: one stage, three places', () => {
  it('never re-mounts the environment from the front door through the room', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);

    // Front door.
    expect(screen.getByTestId('connection-screen')).toBeDefined();
    const environment = screen.getByTestId('pregame-environment');

    // → Lobby (socket open, first view).
    act(() =>
      useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
    );
    act(() => sockets[0]!.emitOpen());
    act(() => sockets[0]!.emitMessage(LOBBY_ROOMLESS_JSON));
    expect(screen.getByTestId('lobby-screen')).toBeDefined();
    expect(screen.getByTestId('pregame-environment')).toBe(environment);

    // → Room. Same node, still: the place changed, the world did not.
    act(() => sockets[0]!.emitMessage(LOBBY_ROOM_DECKED_JSON));
    expect(screen.getByTestId('room-panel')).toBeDefined();
    expect(screen.getByTestId('pregame-environment')).toBe(environment);

    // …and back out to the lobby.
    act(() => sockets[0]!.emitMessage(LOBBY_ROOMLESS_JSON));
    expect(screen.getByTestId('pregame-environment')).toBe(environment);
  });

  it('mounts the SHARED environment system, with no image asset', () => {
    const { container } = render(
      <PregameStage place="front-door">
        <div />
      </PregameStage>,
    );
    const stage = screen.getByTestId('pregame-stage');
    // The place accents still reach the CSS as pregame custom properties…
    expect(stage.style.getPropertyValue('--pregame-glow')).not.toBe('');
    // …and the backdrop itself is now the one `table/environment` stack the
    // match mounts (issue #530), publishing the §5.4 palette slots as `--env-*`
    // from the same token set — so the pregame cannot drift from the match.
    const environment = screen.getByTestId('scene-environment');
    expect(environment.style.getPropertyValue('--env-plaza-core')).not.toBe('');
    expect(environment.style.getPropertyValue('--env-surround-base')).not.toBe('');
    expect(environment.dataset.theme).toBe('runicVale');
    // Layered SVG built from tokens: zero asset bytes against the load budget.
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelectorAll('svg').length).toBeGreaterThan(0);
    expect(screen.getByTestId('pregame-environment').getAttribute('aria-hidden')).toBe('true');
    expect(environment.getAttribute('aria-hidden')).toBe('true');
  });
});

describe('PregameStage — criterion 4: the quality tier scales ambient only', () => {
  it('maps High / Standard / Lite to on / reduced / off, content identical', () => {
    const contentAt = (quality: 'high' | 'standard' | 'lite'): string => {
      setQuality(quality);
      const view = render(
        <PregameStage place="lobby">
          <p>content layer</p>
        </PregameStage>,
      );
      // The ambient level is now resolved by the ONE environment system
      // (`table/environment` `ambientLevel`), which is what makes the pregame
      // and the match provably identical here rather than merely similar.
      const level = screen.getByTestId('scene-environment').getAttribute('data-ambient');
      const content = screen.getByText('content layer').outerHTML;
      view.unmount();
      return `${level}|${content}`;
    };

    const [high, standard, lite] = [contentAt('high'), contentAt('standard'), contentAt('lite')];
    expect(high.split('|')[0]).toBe('l0+l3');
    expect(standard.split('|')[0]).toBe('l0-half');
    expect(lite.split('|')[0]).toBe('off');
    // The content layer is byte-identical at all three levels — the quality
    // tier only ever scales the ambient backdrop.
    expect(standard.split('|')[1]).toBe(high.split('|')[1]);
    expect(lite.split('|')[1]).toBe(high.split('|')[1]);
  });
});

describe('PregameStage — criterion 17: a place change never gates on animation', () => {
  it('renders the destination’s controls on the frame the state changes', () => {
    const { factory, sockets } = recordingFactory();
    render(<App />);
    act(() =>
      useGameStore.getState().connect('ws://test', { createSocket: factory, autoReconnect: false }),
    );
    act(() => sockets[0]!.emitOpen());
    // No timers advanced, no animation awaited: the room's gold control is in
    // the document and hit-testable the moment the view names the room.
    act(() => sockets[0]!.emitMessage(LOBBY_ROOM_DECKED_JSON));
    const gold = screen.getByTestId('ready-button');
    expect(gold.getAttribute('data-gold')).toBe('true');
    expect(gold.hasAttribute('disabled')).toBe(false);
  });
});
