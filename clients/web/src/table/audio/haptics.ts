/**
 * Haptic hooks over the Vibration API (issue #507).
 *
 * Same taxonomy, same per-category controls, and the same "never load-bearing"
 * rule as audio: no support ⇒ nothing happens, silently. Haptics are **off by
 * default** — a device that buzzes without being asked is worse than one that
 * never does — and the browser itself may refuse without a prior user gesture,
 * which is expected, not an error.
 *
 * Patterns are deliberately short. The longest is the verdict; everything else
 * is a tap the player feels rather than notices.
 */
import type { AudioCue, AudioCueCategory, HapticSink } from './types';

/**
 * The buzz for each category, in ms (a single pulse, or an on/off pattern).
 * Every taxonomy category has one so the per-category controls mean the same
 * thing for both channels.
 */
export const HAPTIC_PATTERN: Record<AudioCueCategory, number | number[]> = {
  draw: 6,
  play: 10,
  tap: 8,
  cast: 12,
  resolve: 14,
  impact: 18,
  destroy: 24,
  priority: 6,
  phase: 8,
  victory: [30, 40, 30],
};

/** The Vibration API, where the platform exposes it. */
function vibrateFn(): ((pattern: number | number[]) => boolean) | undefined {
  if (typeof navigator === 'undefined') return undefined;
  const vibrate = (navigator as Navigator & { vibrate?: unknown }).vibrate;
  return typeof vibrate === 'function'
    ? (vibrate as (pattern: number | number[]) => boolean).bind(navigator)
    : undefined;
}

/** Whether this device exposes the Vibration API (drives the settings copy). */
export function hapticsSupported(): boolean {
  return vibrateFn() !== undefined;
}

/** The production haptic sink: one short buzz per cue, or nothing at all. */
export class VibrationHapticSink implements HapticSink {
  vibrate(cue: AudioCue): void {
    try {
      const vibrate = vibrateFn();
      if (vibrate === undefined) return;
      vibrate(HAPTIC_PATTERN[cue.category]);
    } catch {
      // A refused or unsupported vibration never affects presentation or input.
    }
  }
}
