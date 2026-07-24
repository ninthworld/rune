/**
 * The sound and haptic event taxonomy (issue #507).
 *
 * `docs/design/visual-system.md` §9 names the taxonomy —
 * draw/play/tap/cast/resolve/impact/destroy/priority/phase/victory — and binds
 * three rules to it: every hook is **optional**, **independently muted**, and
 * **never load-bearing** (the visual + log channels always stand alone). The
 * asset pipeline (`docs/design/asset-pipeline.md`) puts audio on exactly the
 * same generic effect taxonomy the visual channel already uses: a category plus
 * parameters the client already has.
 *
 * Nothing in this module does I/O, and nothing here may ever be awaited on the
 * reconciler path. A sink is free to do nothing at all — that is the specified
 * behavior when no asset is present.
 */
import type { PlayerId } from '../../protocol';

/**
 * Every sound/haptic category, in the taxonomy's own order. This is the single
 * source of truth: the {@link AudioCueCategory} union is derived from it, so a
 * category added here can never drift out of the type, the settings surface, or
 * the mapping table.
 */
export const AUDIO_CUE_CATEGORIES = [
  'draw',
  'play',
  'tap',
  'cast',
  'resolve',
  'impact',
  'destroy',
  'priority',
  'phase',
  'victory',
] as const;

/** One category of the visual-system §9 sound/haptic taxonomy. */
export type AudioCueCategory = (typeof AUDIO_CUE_CATEGORIES)[number];

/** Human labels for the settings surface, one per category. */
export const AUDIO_CUE_LABELS: Record<AudioCueCategory, string> = {
  draw: 'Draw',
  play: 'Play and zone travel',
  tap: 'Tap and untap',
  cast: 'Cast',
  resolve: 'Resolve, counter, fizzle',
  impact: 'Damage, life, counters',
  destroy: 'Destruction',
  priority: 'Priority',
  phase: 'Phase and turn',
  victory: 'Victory',
};

/**
 * One fire-and-forget sound/haptic request for a single view transition.
 *
 * A cue is already **collapsed**: the derivation emits at most one cue per
 * category per batch window, mirroring the visual stagger budget, so a
 * thirty-token swarm is one sound rather than thirty (issue #507). It carries
 * only parameters the client already has — no rules claim is made anywhere.
 */
export interface AudioCue {
  /** The taxonomy category this cue belongs to. */
  category: AudioCueCategory;
  /**
   * The leading edge of the batch window, in ms, taken from the smallest
   * stagger delay among the collapsed intents. The sound lands with the first
   * item of the batch, not the last.
   */
  delayMs: number;
  /** Largest display magnitude among the collapsed intents, when there is one. */
  magnitude?: number;
  /** The seat the first collapsed intent was credited to, when there is one. */
  seat?: PlayerId;
  /** How many presentation intents collapsed into this cue (always ≥ 1). */
  count: number;
}

/**
 * A playback sink. Implementations must be **fire-and-forget**: `play` returns
 * `void`, never throws out to the caller, and never blocks. The default
 * production sink is silent whenever no asset is registered for a category.
 */
export interface AudioSink {
  /**
   * Play one cue at the resolved linear gain (0–1). A gain of 0 never reaches
   * a sink — the channel drops it first.
   */
  play(cue: AudioCue, gain: number): void;
}

/** A haptic sink over the Vibration API (or a fake, in tests). */
export interface HapticSink {
  /** Buzz for one cue. Fire-and-forget, exactly like {@link AudioSink.play}. */
  vibrate(cue: AudioCue): void;
}

/** A sink pair; the channel dispatches every cue to both. */
export interface AudioChannelSinks {
  audio: AudioSink;
  haptics: HapticSink;
}

/** A sink that does nothing — the honest default where playback is impossible. */
export const SILENT_AUDIO_SINK: AudioSink = { play: () => {} };

/** A haptic sink that does nothing (no Vibration API, or haptics off). */
export const SILENT_HAPTIC_SINK: HapticSink = { vibrate: () => {} };
