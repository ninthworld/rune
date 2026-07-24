/**
 * The scene's subscription to the sound/haptic hook layer (issue #507).
 *
 * The plane is the one production caller of `presentAudio`, so this is where
 * the wiring guarantees are asserted: cues follow the ordinary reconcile path,
 * a reconnect rebuild replays nothing, and a hook that fails cannot take the
 * scene or its instrumentation down with it. The effects layer is mocked to a
 * recorder, exactly as in the sibling presentation tests, so no WebGL context
 * is needed.
 */
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameView } from '../../protocol';
import { resetAudioChannel, setAudioSinks } from '../audio';
import type { AudioCue } from '../audio';
import { resetAudioSettings, setAudioMuted, setHapticsEnabled } from '../settings/audioSettings';
import { LivePlane } from './LivePlane';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

/** A recording sink pair standing in for real playback. */
function fakeSinks() {
  const played: AudioCue[] = [];
  const buzzed: AudioCue[] = [];
  setAudioSinks({
    audio: { play: (cue) => void played.push(cue) },
    haptics: { vibrate: (cue) => void buzzed.push(cue) },
  });
  return { played, buzzed };
}

/** The sample frame plus one structured draw entry — a guaranteed cue. */
function withDraw(sequence: number): GameView {
  return {
    ...SAMPLE_GAME_VIEW,
    log: [
      ...(SAMPLE_GAME_VIEW.log ?? []),
      { sequence, event: { type: 'cards_drawn', player: 'p1', count: 1 } },
    ],
  };
}

const plane = (view: GameView, sessionEpoch = 1) => (
  <LivePlane
    view={view}
    quality="standard"
    density="reduced"
    reducedMotion={false}
    sessionEpoch={sessionEpoch}
  />
);

describe('LivePlane sound and haptic hooks', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
    setAudioMuted(false);
  });

  afterEach(() => {
    cleanup();
    setAudioSinks(null);
    resetAudioChannel();
    resetAudioSettings();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('makes no sound on first mount — a fresh scene replays no history', () => {
    const { played } = fakeSinks();

    render(plane(SAMPLE_GAME_VIEW));

    expect(played).toEqual([]);
  });

  it('plays the transition’s cues on the ordinary reconcile path', () => {
    const { played } = fakeSinks();
    const { rerender } = render(plane(SAMPLE_GAME_VIEW));

    rerender(plane(withDraw(9001)));

    expect(played.map((cue) => cue.category)).toContain('draw');
  });

  it('replays nothing on a reconnect rebuild', () => {
    const { played } = fakeSinks();
    const { rerender } = render(plane(SAMPLE_GAME_VIEW, 1));
    played.length = 0;

    // A newer transport generation: the scene rebuilds without catch-up motion,
    // and the audio channel must not narrate what the scene did not show.
    rerender(plane(withDraw(9001), 2));

    expect(played).toEqual([]);
  });

  it('buzzes only once haptics are opted into', () => {
    const { buzzed } = fakeSinks();
    const { rerender } = render(plane(SAMPLE_GAME_VIEW));

    rerender(plane(withDraw(9001)));
    expect(buzzed).toEqual([]);

    setHapticsEnabled(true);
    rerender(plane(withDraw(9002)));
    expect(buzzed.map((cue) => cue.category)).toContain('draw');
  });

  it('renders the scene unharmed when the sink throws', () => {
    setAudioSinks({
      audio: {
        play: () => {
          throw new Error('audio device exploded');
        },
      },
      haptics: { vibrate: () => {} },
    });
    const { rerender, getByTestId } = render(plane(SAMPLE_GAME_VIEW));

    expect(() => rerender(plane(withDraw(9001)))).not.toThrow();
    expect(getByTestId('live-2-5d-plane')).toBeTruthy();
  });
});
