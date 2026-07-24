/**
 * A **development-only** placeholder tone set (issue #507).
 *
 * No audio file ships with this client. ADR 0031 admits a bundled asset only as
 * an original commissioned work, an original AI-generated work with a recorded
 * prompt/tool, or a permissively licensed third-party work — each with a ledger
 * entry that CI enforces. Rather than manufacture a provenance story for
 * throwaway placeholders (and pay for them in the load budget of #510), the
 * tone set is **generated at runtime from an oscillator's worth of arithmetic**:
 * zero bytes on the wire, no ledger entry, nothing to license. Production audio
 * remains separate asset work.
 *
 * The channel wires this behind `import.meta.env.DEV`, so the reference folds
 * away and the module is dropped from the production bundle entirely.
 *
 * These are deliberately plain: a short decaying sine per category, distinct
 * enough to tell the hooks apart while developing, and nothing anyone would
 * mistake for the shipped sound design.
 */
import type { AudioCueCategory } from './types';

/** Base pitch (Hz) and length (s) per taxonomy category. */
const TONE: Record<AudioCueCategory, { hz: number; seconds: number }> = {
  draw: { hz: 880, seconds: 0.08 },
  play: { hz: 523, seconds: 0.1 },
  tap: { hz: 660, seconds: 0.06 },
  cast: { hz: 740, seconds: 0.14 },
  resolve: { hz: 987, seconds: 0.16 },
  impact: { hz: 196, seconds: 0.12 },
  destroy: { hz: 147, seconds: 0.22 },
  priority: { hz: 1047, seconds: 0.05 },
  phase: { hz: 392, seconds: 0.09 },
  victory: { hz: 587, seconds: 0.4 },
};

/** Peak amplitude of a generated tone — quiet; these are placeholders. */
const PEAK = 0.2;

/**
 * Generate one category's placeholder tone: a sine at the category's pitch with
 * an exponential decay envelope. Returns `undefined` if the context cannot
 * allocate a buffer, which the sink treats as silence like any other absence.
 */
export function synthesizeCueTone(
  context: BaseAudioContext,
  category: AudioCueCategory,
): AudioBuffer | undefined {
  try {
    const { hz, seconds } = TONE[category];
    const rate = context.sampleRate || 44_100;
    const frames = Math.max(1, Math.floor(rate * seconds));
    const buffer = context.createBuffer(1, frames, rate);
    const samples = buffer.getChannelData(0);
    for (let i = 0; i < frames; i += 1) {
      const t = i / rate;
      samples[i] = Math.sin(2 * Math.PI * hz * t) * PEAK * Math.exp(-t / (seconds / 3));
    }
    return buffer;
  } catch {
    return undefined;
  }
}
