/**
 * The device-local sound/haptic settings store (issue #507): silent defaults,
 * a persistence round trip for every control, and the gain/haptic resolution
 * the hook layer depends on.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AUDIO_CUE_CATEGORIES } from '../audio/types';
import {
  DEFAULT_AUDIO_VOLUME,
  clampVolume,
  getAudioSnapshot,
  isCategoryEnabled,
  resetAudioSettings,
  resolveCueGain,
  resolveHaptic,
  setAudioMuted,
  setAudioVolume,
  setCategoryMuted,
  setHapticsEnabled,
  subscribeAudio,
} from './audioSettings';

afterEach(() => {
  resetAudioSettings();
  localStorage.clear();
});

describe('audio settings defaults', () => {
  it('ships muted, un-buzzing, and with every category available', () => {
    const settings = getAudioSnapshot();

    expect(settings.muted).toBe(true);
    expect(settings.haptics).toBe(false);
    expect(settings.volume).toBe(DEFAULT_AUDIO_VOLUME);
    expect(settings.mutedCategories.size).toBe(0);
    for (const category of AUDIO_CUE_CATEGORIES) {
      expect(isCategoryEnabled(settings, category)).toBe(true);
    }
  });

  it('resolves a zero gain for every category while muted', () => {
    for (const category of AUDIO_CUE_CATEGORIES) {
      expect(resolveCueGain(getAudioSnapshot(), category)).toBe(0);
    }
  });
});

describe('audio settings round trip', () => {
  it('persists the master mute across a reload', () => {
    setAudioMuted(false);
    resetAudioSettings();
    expect(getAudioSnapshot().muted).toBe(false);

    setAudioMuted(true);
    resetAudioSettings();
    expect(getAudioSnapshot().muted).toBe(true);
  });

  it('persists the master volume across a reload', () => {
    setAudioMuted(false);
    setAudioVolume(0.25);
    resetAudioSettings();

    expect(getAudioSnapshot().volume).toBe(0.25);
  });

  it('persists per-category mutes across a reload', () => {
    setCategoryMuted('priority', true);
    setCategoryMuted('phase', true);
    resetAudioSettings();

    const settings = getAudioSnapshot();
    expect([...settings.mutedCategories].sort()).toEqual(['phase', 'priority']);
    expect(isCategoryEnabled(settings, 'draw')).toBe(true);
  });

  it('persists the haptics opt-in across a reload', () => {
    setHapticsEnabled(true);
    resetAudioSettings();

    expect(getAudioSnapshot().haptics).toBe(true);
  });

  it('un-mutes a category back to audible', () => {
    setCategoryMuted('draw', true);
    setCategoryMuted('draw', false);
    resetAudioSettings();

    expect(getAudioSnapshot().mutedCategories.size).toBe(0);
  });

  it('drops an unrecognized stored category rather than trusting it', () => {
    localStorage.setItem('rune.audio.muted-categories', 'draw,not-a-category');
    resetAudioSettings();

    expect([...getAudioSnapshot().mutedCategories]).toEqual(['draw']);
  });

  it('degrades to the defaults when storage is unavailable', () => {
    const getItem = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('storage disabled');
    });
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('storage disabled');
    });
    resetAudioSettings();

    expect(getAudioSnapshot().muted).toBe(true);
    // Saving is a no-op, but the choice still applies for this session.
    expect(() => setAudioMuted(false)).not.toThrow();
    expect(getAudioSnapshot().muted).toBe(false);

    getItem.mockRestore();
    setItem.mockRestore();
  });
});

describe('audio settings observability', () => {
  it('republishes to subscribers and keeps the snapshot stable otherwise', () => {
    const listener = vi.fn();
    const unsubscribe = subscribeAudio(listener);
    const before = getAudioSnapshot();

    setAudioMuted(false);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(getAudioSnapshot()).not.toBe(before);

    // A no-op set neither republishes nor churns the snapshot reference.
    const after = getAudioSnapshot();
    setAudioMuted(false);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(getAudioSnapshot()).toBe(after);

    unsubscribe();
    setAudioMuted(true);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe('gain and haptic resolution', () => {
  it('clamps a volume into 0–1', () => {
    expect(clampVolume(-3)).toBe(0);
    expect(clampVolume(9)).toBe(1);
    expect(clampVolume(Number.NaN)).toBe(DEFAULT_AUDIO_VOLUME);
  });

  it('returns the master volume for an audible category', () => {
    setAudioMuted(false);
    setAudioVolume(0.8);

    expect(resolveCueGain(getAudioSnapshot(), 'cast')).toBe(0.8);
  });

  it('returns zero for a muted category even when unmuted overall', () => {
    setAudioMuted(false);
    setCategoryMuted('cast', true);

    expect(resolveCueGain(getAudioSnapshot(), 'cast')).toBe(0);
    expect(resolveCueGain(getAudioSnapshot(), 'draw')).toBeGreaterThan(0);
  });

  it('gates haptics on the opt-in and the same per-category control', () => {
    expect(resolveHaptic(getAudioSnapshot(), 'impact')).toBe(false);

    setHapticsEnabled(true);
    expect(resolveHaptic(getAudioSnapshot(), 'impact')).toBe(true);

    setCategoryMuted('impact', true);
    expect(resolveHaptic(getAudioSnapshot(), 'impact')).toBe(false);
  });

  it('leaves haptics independent of the master sound mute', () => {
    setHapticsEnabled(true);

    expect(getAudioSnapshot().muted).toBe(true);
    expect(resolveHaptic(getAudioSnapshot(), 'destroy')).toBe(true);
  });
});
