import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  detectDefaultQuality,
  getPresentationSnapshot,
  getQualityDetection,
  resetPresentationSettings,
  resolveReducedMotion,
  setDensity,
  setMotion,
  setQuality,
  subscribePresentation,
  type MotionPreference,
} from './presentationSettings';

afterEach(() => {
  resetPresentationSettings();
  localStorage.clear();
});

describe('detectDefaultQuality (issue #505, conservative first run)', () => {
  it('defaults to Standard with no low-capability signal', () => {
    expect(detectDefaultQuality({}).quality).toBe('standard');
    expect(detectDefaultQuality({ deviceMemory: 8, hardwareConcurrency: 8 }).quality).toBe(
      'standard',
    );
  });

  it('never auto-selects High — it is an explicit opt-in', () => {
    // Even a very capable device only ever auto-lands on Standard.
    expect(detectDefaultQuality({ deviceMemory: 32, hardwareConcurrency: 32 }).quality).toBe(
      'standard',
    );
  });

  it('drops to Lite on Data Saver', () => {
    expect(detectDefaultQuality({ saveData: true }).quality).toBe('lite');
  });

  it('drops to Lite on low reported memory', () => {
    expect(detectDefaultQuality({ deviceMemory: 2 }).quality).toBe('lite');
  });

  it('drops to Lite on two or fewer cores', () => {
    expect(detectDefaultQuality({ hardwareConcurrency: 2 }).quality).toBe('lite');
    expect(detectDefaultQuality({ hardwareConcurrency: 1 }).reason).toContain('1 CPU core');
  });
});

describe('resolveReducedMotion (OS × user composition)', () => {
  const cases: { motion: MotionPreference; os: boolean; expected: boolean }[] = [
    { motion: 'system', os: false, expected: false },
    { motion: 'system', os: true, expected: true },
    { motion: 'reduced', os: false, expected: true },
    { motion: 'reduced', os: true, expected: true },
    { motion: 'full', os: false, expected: false },
    // OS reduced-motion is authoritative — `full` cannot override an accessibility setting.
    { motion: 'full', os: true, expected: true },
  ];
  it.each(cases)('motion=$motion os=$os ⇒ $expected', ({ motion, os, expected }) => {
    expect(resolveReducedMotion(os, motion)).toBe(expected);
  });
});

describe('presentation settings store', () => {
  it('starts auto-detected when nothing is stored, and shows the reason', () => {
    const snapshot = getPresentationSnapshot();
    expect(snapshot.qualityAutoDetected).toBe(true);
    expect(getQualityDetection().reason.length).toBeGreaterThan(0);
    // Orthogonal controls have their own defaults.
    expect(snapshot.density).toBe('reduced');
    expect(snapshot.motion).toBe('system');
  });

  it('persists a chosen quality and clears the auto-detected flag (round-trip)', () => {
    setQuality('high');
    expect(getPresentationSnapshot().quality).toBe('high');
    expect(getPresentationSnapshot().qualityAutoDetected).toBe(false);
    // A fresh store reads the persisted choice back — the override sticks.
    resetPresentationSettings();
    const reloaded = getPresentationSnapshot();
    expect(reloaded.quality).toBe('high');
    expect(reloaded.qualityAutoDetected).toBe(false);
  });

  it('persists density and motion across a reload', () => {
    setDensity('full');
    setMotion('reduced');
    resetPresentationSettings();
    const reloaded = getPresentationSnapshot();
    expect(reloaded.density).toBe('full');
    expect(reloaded.motion).toBe('reduced');
  });

  it('publishes to subscribers and hands out a new snapshot reference on change', () => {
    const before = getPresentationSnapshot();
    const listener = vi.fn();
    const unsubscribe = subscribePresentation(listener);
    setMotion('full');
    expect(listener).toHaveBeenCalledTimes(1);
    expect(getPresentationSnapshot()).not.toBe(before);
    // A no-op set neither republishes nor churns the reference.
    const stable = getPresentationSnapshot();
    setMotion('full');
    expect(listener).toHaveBeenCalledTimes(1);
    expect(getPresentationSnapshot()).toBe(stable);
    unsubscribe();
  });
});
