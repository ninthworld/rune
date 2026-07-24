/**
 * The hook layer's dispatch contract (issue #507), asserted through a fake
 * audio sink: mutes, per-category controls, haptic opt-in, and the "never
 * load-bearing" guarantee that no failure below the hook can reach the scene.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import type { GameView } from '../../protocol';
import {
  resetAudioSettings,
  setAudioMuted,
  setAudioVolume,
  setCategoryMuted,
  setHapticsEnabled,
} from '../settings/audioSettings';
import { deriveGameViewPresentation } from '../live/gameViewPresentation';
import { presentAudio, resetAudioChannel, setAudioSinks } from './channel';
import type { AudioCue } from './types';

/** A fake sink pair that records everything it was asked to do. */
function fakeSinks() {
  const played: { cue: AudioCue; gain: number }[] = [];
  const buzzed: AudioCue[] = [];
  return {
    played,
    buzzed,
    sinks: {
      audio: { play: (cue: AudioCue, gain: number) => void played.push({ cue, gain }) },
      haptics: { vibrate: (cue: AudioCue) => void buzzed.push(cue) },
    },
  };
}

/** A transition that produces exactly one draw cue and one impact cue. */
function transition(): ReturnType<typeof deriveGameViewPresentation> {
  const previous: GameView = structuredClone(SAMPLE_GAME_VIEW);
  const current = structuredClone(previous);
  const start = Math.max(0, ...(previous.log ?? []).map((entry) => entry.sequence));
  current.log = [
    ...(previous.log ?? []),
    { sequence: start + 1, event: { type: 'cards_drawn', player: 'p1', count: 1 } },
    {
      sequence: start + 2,
      event: { type: 'damage_dealt', target: { kind: 'player', player: 'p2' }, amount: 4 },
    },
  ];
  return deriveGameViewPresentation(previous, current);
}

afterEach(() => {
  setAudioSinks(null);
  resetAudioChannel();
  resetAudioSettings();
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('presentAudio', () => {
  it('is silent by default — nothing plays until the player asks for it', () => {
    const { played, buzzed, sinks } = fakeSinks();
    setAudioSinks(sinks);

    presentAudio(transition());

    expect(played).toEqual([]);
    expect(buzzed).toEqual([]);
  });

  it('plays each derived cue at the resolved master volume once unmuted', () => {
    const { played, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);
    setAudioVolume(0.5);

    presentAudio(transition());

    expect(played.map((entry) => entry.cue.category).sort()).toEqual(['draw', 'impact']);
    expect(played.every((entry) => entry.gain === 0.5)).toBe(true);
  });

  it('honors the master mute', () => {
    const { played, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);
    presentAudio(transition());
    expect(played.length).toBe(2);

    played.length = 0;
    setAudioMuted(true);
    presentAudio(transition());
    expect(played).toEqual([]);
  });

  it('honors a per-category mute without touching the others', () => {
    const { played, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);
    setCategoryMuted('impact', true);

    presentAudio(transition());

    expect(played.map((entry) => entry.cue.category)).toEqual(['draw']);
  });

  it('never buzzes until haptics are opted into', () => {
    const { buzzed, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);

    presentAudio(transition());
    expect(buzzed).toEqual([]);

    setHapticsEnabled(true);
    presentAudio(transition());
    expect(buzzed.map((cue) => cue.category).sort()).toEqual(['draw', 'impact']);
  });

  it('buzzes with sound muted — the two channels are independent', () => {
    const { played, buzzed, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setHapticsEnabled(true);

    presentAudio(transition());

    expect(played).toEqual([]);
    expect(buzzed.length).toBe(2);
  });

  it('applies the per-category mute to haptics too', () => {
    const { buzzed, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setHapticsEnabled(true);
    setCategoryMuted('draw', true);

    presentAudio(transition());

    expect(buzzed.map((cue) => cue.category)).toEqual(['impact']);
  });

  it('drops a zero volume rather than handing a silent cue to the sink', () => {
    const { played, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);
    setAudioVolume(0);

    presentAudio(transition());

    expect(played).toEqual([]);
  });

  it('contains a throwing sink — playback failure never reaches the caller', () => {
    setAudioMuted(false);
    setHapticsEnabled(true);
    setAudioSinks({
      audio: {
        play: () => {
          throw new Error('audio device exploded');
        },
      },
      haptics: {
        vibrate: () => {
          throw new Error('vibration motor exploded');
        },
      },
    });

    expect(() => presentAudio(transition())).not.toThrow();
  });

  it('returns synchronously — nothing on the reconciler path is awaited', () => {
    const { sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);

    const result = presentAudio(transition());

    expect(Array.isArray(result)).toBe(true);
    expect((result as unknown as { then?: unknown }).then).toBeUndefined();
  });

  it('derives nothing for a first mount, so a reconnect replays no history', () => {
    const { played, sinks } = fakeSinks();
    setAudioSinks(sinks);
    setAudioMuted(false);

    presentAudio(deriveGameViewPresentation(undefined, structuredClone(SAMPLE_GAME_VIEW)));

    expect(played).toEqual([]);
  });

  it('makes no sound at all with no asset registered (the shipped default)', () => {
    // The real production sink, in an environment with no Web Audio API: the
    // specified outcome is complete silence and zero errors.
    setAudioMuted(false);
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
    const warns = vi.spyOn(console, 'warn').mockImplementation(() => {});

    expect(() => presentAudio(transition())).not.toThrow();

    expect(errors).not.toHaveBeenCalled();
    expect(warns).not.toHaveBeenCalled();
  });
});
