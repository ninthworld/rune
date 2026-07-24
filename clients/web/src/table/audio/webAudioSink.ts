/**
 * Pooled `AudioContext` playback for the sound taxonomy (issue #507).
 *
 * Every rule this file exists to keep comes from visual-system §9's "never
 * load-bearing":
 *
 * - **No asset ⇒ complete silence and zero errors.** An unregistered category
 *   returns immediately. There is no fetch, no warning, no console noise, and
 *   the caller cannot tell the difference from a successful play.
 * - **Nothing is awaited.** {@link WebAudioSink.play} returns `void` and every
 *   promise the Web Audio API hands back (`resume`, `decodeAudioData`) is
 *   handled with a swallowing `catch`. The reconciler path never waits.
 * - **Failures are contained.** The whole body is guarded; a browser that
 *   throws on any node operation degrades to silence rather than breaking the
 *   scene or input.
 * - **Autoplay policy is expected, not an error.** A context that starts
 *   `suspended` is not played into; instead a one-shot gesture listener resumes
 *   it on the player's first interaction. Nothing queues up in the meantime, so
 *   unlocking never produces a burst of stale sounds.
 *
 * The sink is also **pooled**: one context, one master gain, and a hard cap on
 * simultaneous voices, so a dense moment can never open unbounded nodes.
 */
import type { AudioCue, AudioCueCategory, AudioSink } from './types';

/** Synthesize a buffer for a category with no registered asset. */
export type ToneSynth = (
  context: BaseAudioContext,
  category: AudioCueCategory,
) => AudioBuffer | undefined;

/** Construction inputs; every one has a safe production default. */
export interface WebAudioSinkOptions {
  /**
   * Build the context. The default returns `null` wherever the Web Audio API is
   * absent (SSR, jsdom, locked-down browsers) — the sink is then a total no-op.
   */
  contextFactory?: () => AudioContext | null;
  /**
   * Optional placeholder tone generator, used only for categories with no
   * registered asset. Wired **development-only** by the channel; production
   * audio is separate asset work under ADR 0031.
   */
  tones?: ToneSynth;
  /** Maximum simultaneous voices. Beyond it, a cue is dropped, never queued. */
  maxVoices?: number;
}

/** The default voice cap: dense moments stay a texture, never a pile-up. */
const DEFAULT_MAX_VOICES = 8;

/** Gestures that satisfy every browser's autoplay unlock requirement. */
const UNLOCK_EVENTS = ['pointerdown', 'keydown', 'touchend'] as const;

/** The Web Audio constructor, including the legacy prefixed spelling. */
function audioContextConstructor(): (new () => AudioContext) | undefined {
  if (typeof globalThis === 'undefined') return undefined;
  const scope = globalThis as unknown as {
    AudioContext?: new () => AudioContext;
    webkitAudioContext?: new () => AudioContext;
  };
  return scope.AudioContext ?? scope.webkitAudioContext;
}

/** The production context factory: a real context, or `null` where impossible. */
export function defaultAudioContextFactory(): AudioContext | null {
  try {
    const Ctor = audioContextConstructor();
    return Ctor === undefined ? null : new Ctor();
  } catch {
    return null;
  }
}

/** Web Audio playback for the sound taxonomy. Silent until assets exist. */
export class WebAudioSink implements AudioSink {
  private readonly contextFactory: () => AudioContext | null;
  private readonly tones: ToneSynth | undefined;
  private readonly maxVoices: number;
  private readonly buffers = new Map<AudioCueCategory, AudioBuffer>();
  private context: AudioContext | null = null;
  private master: GainNode | null = null;
  private constructed = false;
  private unlockArmed = false;
  private voices = 0;

  constructor(options: WebAudioSinkOptions = {}) {
    this.contextFactory = options.contextFactory ?? defaultAudioContextFactory;
    this.tones = options.tones;
    this.maxVoices = options.maxVoices ?? DEFAULT_MAX_VOICES;
  }

  /**
   * Register a decoded buffer for one category. The only way audio becomes
   * audible; with no call to this (the shipped state today) the sink is silent.
   */
  register(category: AudioCueCategory, buffer: AudioBuffer): void {
    this.buffers.set(category, buffer);
  }

  /**
   * Fetch and decode one category's asset, fire-and-forget. A missing file, a
   * 404, an undecodable body, or an absent context all resolve to silence with
   * no error surfaced — the pipeline's procedural-fallback stance, where the
   * fallback for audio is simply nothing.
   *
   * Nothing calls this today: no audio asset ships (ADR 0031). It is the drop-in
   * point for the first ledgered asset, which belongs under the `lazy/` prefix
   * so it stays outside the first-match load budget.
   */
  load(category: AudioCueCategory, url: string): void {
    void (async () => {
      try {
        const context = this.ensureContext();
        if (!context) return;
        const response = await fetch(url);
        if (!response.ok) return;
        const bytes = await response.arrayBuffer();
        this.buffers.set(category, await context.decodeAudioData(bytes));
      } catch {
        // Silence is the specified fallback; a missing sound is never an error.
      }
    })();
  }

  /** Whether a category can currently make a sound (tests and diagnostics). */
  has(category: AudioCueCategory): boolean {
    return this.buffers.has(category) || this.tones !== undefined;
  }

  /** Release the pooled context. Idempotent and never throws. */
  close(): void {
    try {
      void this.context?.close().catch(() => {});
    } catch {
      // A context that refuses to close is not worth reporting.
    }
    this.context = null;
    this.master = null;
    this.constructed = false;
    this.voices = 0;
  }

  /** Create the pooled context once; `null` forever where it is unavailable. */
  private ensureContext(): AudioContext | null {
    if (this.constructed) return this.context;
    this.constructed = true;
    try {
      this.context = this.contextFactory();
      if (this.context) {
        this.master = this.context.createGain();
        this.master.connect(this.context.destination);
      }
    } catch {
      this.context = null;
      this.master = null;
    }
    return this.context;
  }

  /**
   * Arm a one-shot gesture listener that resumes a suspended context. Registered
   * at most once, removed on the first gesture, and entirely silent about
   * failure — an autoplay block is the expected state, not a fault.
   */
  private armUnlock(context: AudioContext): void {
    if (this.unlockArmed || typeof window === 'undefined') return;
    this.unlockArmed = true;
    const unlock = (): void => {
      for (const event of UNLOCK_EVENTS) window.removeEventListener(event, unlock);
      try {
        void context.resume().catch(() => {});
      } catch {
        // Nothing to do: the sink stays silent until a later gesture succeeds.
      }
    };
    for (const event of UNLOCK_EVENTS) {
      window.addEventListener(event, unlock, { once: false, passive: true });
    }
  }

  /** The buffer for a category: a registered asset, else a placeholder tone. */
  private bufferFor(
    context: BaseAudioContext,
    category: AudioCueCategory,
  ): AudioBuffer | undefined {
    const registered = this.buffers.get(category);
    if (registered) return registered;
    if (!this.tones) return undefined;
    const generated = this.tones(context, category);
    if (generated) this.buffers.set(category, generated);
    return generated;
  }

  /** Play one cue. Fire-and-forget: returns immediately and never throws. */
  play(cue: AudioCue, gain: number): void {
    try {
      if (gain <= 0) return;
      const context = this.ensureContext();
      const master = this.master;
      if (!context || !master) return;
      if (context.state === 'suspended') {
        // Autoplay policy. Ask once, arm the gesture, and drop this cue rather
        // than scheduling it — an unlock must not replay what the player missed.
        this.armUnlock(context);
        void context.resume().catch(() => {});
        return;
      }
      if (this.voices >= this.maxVoices) return;
      const buffer = this.bufferFor(context, cue.category);
      if (!buffer) return;

      const source = context.createBufferSource();
      source.buffer = buffer;
      const voice = context.createGain();
      // Magnitude nudges loudness within a narrow band: a bigger hit is a little
      // louder, never a different sound and never past the resolved gain.
      const emphasis = cue.magnitude === undefined ? 1 : Math.min(1.25, 1 + cue.magnitude / 40);
      voice.gain.value = Math.min(1, gain * emphasis);
      source.connect(voice);
      voice.connect(master);
      this.voices += 1;
      source.onended = () => {
        this.voices = Math.max(0, this.voices - 1);
        try {
          source.disconnect();
          voice.disconnect();
        } catch {
          // Already torn down by the context; nothing to release.
        }
      };
      source.start(context.currentTime + Math.max(0, cue.delayMs) / 1000);
    } catch {
      // Playback can never affect presentation or input (visual-system §9).
    }
  }
}
