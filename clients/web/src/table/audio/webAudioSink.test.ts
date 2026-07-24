/**
 * The pooled Web Audio sink (issue #507): silence where the API is absent,
 * silence where no asset is registered, autoplay handled without console noise,
 * and no promise ever handed back to the caller.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { synthesizeCueTone } from './devTones';
import { WebAudioSink } from './webAudioSink';
import type { AudioCue } from './types';

const cue = (category: AudioCue['category'] = 'draw', extra: Partial<AudioCue> = {}): AudioCue => ({
  category,
  delayMs: 0,
  count: 1,
  ...extra,
});

/** A minimal fake `AudioContext` recording the graph the sink builds. */
function fakeContext(state: AudioContextState = 'running') {
  const started: number[] = [];
  const gains: { value: number }[] = [];
  const resume = vi.fn(() => Promise.resolve());
  const context = {
    state,
    currentTime: 10,
    sampleRate: 48_000,
    destination: { id: 'destination' },
    resume,
    close: vi.fn(() => Promise.resolve()),
    createGain: () => {
      const node = { gain: { value: 1 }, connect: vi.fn(), disconnect: vi.fn() };
      gains.push(node.gain);
      return node;
    },
    createBufferSource: () => ({
      buffer: null as AudioBuffer | null,
      onended: null as (() => void) | null,
      connect: vi.fn(),
      disconnect: vi.fn(),
      start: (when: number) => void started.push(when),
    }),
    createBuffer: (channels: number, frames: number, rate: number) => {
      const data = new Float32Array(frames);
      return {
        length: frames,
        sampleRate: rate,
        numberOfChannels: channels,
        getChannelData: () => data,
      };
    },
  };
  return { context: context as unknown as AudioContext, started, gains, resume };
}

/** A one-frame buffer standing in for a decoded asset. */
const stubBuffer = { length: 1 } as unknown as AudioBuffer;

let errorSpy: ReturnType<typeof vi.spyOn>;
let warnSpy: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('WebAudioSink — no asset means silence, not an error', () => {
  it('does nothing when the Web Audio API is unavailable', () => {
    const sink = new WebAudioSink({ contextFactory: () => null });

    expect(() => sink.play(cue(), 1)).not.toThrow();
    expect(errorSpy).not.toHaveBeenCalled();
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it('does nothing when a context exists but no asset is registered', () => {
    const { context, started } = fakeContext();
    const sink = new WebAudioSink({ contextFactory: () => context });

    sink.play(cue(), 1);

    expect(started).toEqual([]);
    expect(sink.has('draw')).toBe(false);
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it('survives a context factory that throws', () => {
    const sink = new WebAudioSink({
      contextFactory: () => {
        throw new Error('no audio device');
      },
    });

    expect(() => sink.play(cue(), 1)).not.toThrow();
  });

  it('plays a registered buffer, scheduled at the cue’s batch delay', () => {
    const { context, started } = fakeContext();
    const sink = new WebAudioSink({ contextFactory: () => context });
    sink.register('draw', stubBuffer);

    sink.play(cue('draw', { delayMs: 120 }), 1);

    expect(started).toEqual([10.12]);
  });

  it('never plays at a zero gain', () => {
    const { context, started } = fakeContext();
    const sink = new WebAudioSink({ contextFactory: () => context });
    sink.register('draw', stubBuffer);

    sink.play(cue(), 0);

    expect(started).toEqual([]);
  });

  it('caps simultaneous voices so a dense moment cannot pile up', () => {
    const { context, started } = fakeContext();
    const sink = new WebAudioSink({ contextFactory: () => context, maxVoices: 2 });
    sink.register('draw', stubBuffer);

    for (let i = 0; i < 8; i += 1) sink.play(cue(), 1);

    expect(started).toHaveLength(2);
  });

  it('keeps magnitude emphasis inside the resolved gain', () => {
    const { context, gains } = fakeContext();
    const sink = new WebAudioSink({ contextFactory: () => context });
    sink.register('impact', stubBuffer);

    sink.play(cue('impact', { magnitude: 400 }), 1);

    // The master gain is created first; the voice gain is the one after it.
    expect(gains.at(-1)?.value).toBeLessThanOrEqual(1);
  });
});

describe('WebAudioSink — autoplay policy', () => {
  it('resumes a suspended context and drops the cue instead of queueing it', () => {
    const { context, started, resume } = fakeContext('suspended');
    const sink = new WebAudioSink({ contextFactory: () => context });
    sink.register('draw', stubBuffer);

    sink.play(cue(), 1);

    expect(resume).toHaveBeenCalled();
    // Nothing is scheduled: unlocking must not replay what the player missed.
    expect(started).toEqual([]);
    expect(errorSpy).not.toHaveBeenCalled();
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it('resumes on the first user gesture', () => {
    const { context, resume } = fakeContext('suspended');
    const sink = new WebAudioSink({ contextFactory: () => context });
    sink.register('draw', stubBuffer);

    sink.play(cue(), 1);
    resume.mockClear();
    window.dispatchEvent(new Event('pointerdown'));

    expect(resume).toHaveBeenCalledTimes(1);
  });

  it('swallows a rejected resume', () => {
    const { context } = fakeContext('suspended');
    (context as unknown as { resume: () => Promise<void> }).resume = () =>
      Promise.reject(new Error('blocked'));
    const sink = new WebAudioSink({ contextFactory: () => context });

    expect(() => sink.play(cue(), 1)).not.toThrow();
  });
});

describe('WebAudioSink — development placeholder tones', () => {
  it('generates a tone for an unregistered category only when tones are wired', () => {
    const { context, started } = fakeContext();
    const sink = new WebAudioSink({ contextFactory: () => context, tones: synthesizeCueTone });

    sink.play(cue('victory'), 1);

    expect(started).toHaveLength(1);
    expect(sink.has('victory')).toBe(true);
  });

  it('produces a finite, bounded waveform', () => {
    const { context } = fakeContext();
    const buffer = synthesizeCueTone(context, 'impact');
    const samples = buffer?.getChannelData(0);

    expect(samples).toBeDefined();
    expect(samples!.length).toBeGreaterThan(0);
    for (const sample of samples!) {
      expect(Number.isFinite(sample)).toBe(true);
      expect(Math.abs(sample)).toBeLessThanOrEqual(1);
    }
  });
});
