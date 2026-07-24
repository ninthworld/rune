/**
 * The sound/haptic hook layer (issue #507): one entry point the scene calls,
 * fire-and-forget, once per presented view transition.
 *
 * `presentAudio` is the whole contract with the rest of the client. It is
 * **synchronous, total, and swallowing**: it awaits nothing, it returns `void`,
 * and any failure anywhere below it — a hostile sink, a browser that throws on
 * a node operation, a settings store that cannot read storage — is caught here
 * so it can never reach the reconciler, the effects layer, or input handling.
 * That is visual-system §9's "never load-bearing" expressed as code.
 *
 * The default sinks are silent in production today: no audio asset ships
 * (ADR 0031), so {@link WebAudioSink} finds no buffer for any category and does
 * nothing, and haptics are off by default. In development the placeholder tone
 * set is wired in so the hooks are audible while working on them; the reference
 * folds away in a production build.
 */
import { getAudioSnapshot, resolveCueGain, resolveHaptic } from '../settings/audioSettings';
import type { GameViewPresentation } from '../live/gameViewPresentation';
import { deriveAudioCues } from './cues';
import { synthesizeCueTone } from './devTones';
import { VibrationHapticSink } from './haptics';
import { WebAudioSink } from './webAudioSink';
import type { AudioChannelSinks, AudioCue } from './types';

/**
 * Whether the development placeholder tones are wired. `import.meta.env.DEV` is
 * a build-time constant, so a production build folds this to `false` and the
 * bundler drops `./devTones` entirely — no placeholder audio code ships.
 */
const DEV_TONES = import.meta.env.DEV;

let sinks: AudioChannelSinks | null = null;

/** Build the production sink pair on first use. */
function defaultSinks(): AudioChannelSinks {
  return {
    audio: new WebAudioSink(DEV_TONES ? { tones: synthesizeCueTone } : {}),
    haptics: new VibrationHapticSink(),
  };
}

/** The live sinks, constructed lazily so importing this module opens nothing. */
function channelSinks(): AudioChannelSinks {
  sinks ??= defaultSinks();
  return sinks;
}

/**
 * Install alternative sinks — the fake audio sink used by tests, or a future
 * alternative backend. Passing `null` restores the production pair.
 */
export function setAudioSinks(next: AudioChannelSinks | null): void {
  sinks = next;
}

/** Drop the current sinks so the next cue rebuilds the production pair. */
export function resetAudioChannel(): void {
  sinks = null;
}

/**
 * Dispatch the sound and haptic cues for one authoritative view transition.
 *
 * Called from the scene's ordinary reconcile path only. A reconnect rebuild and
 * a fast-forward collapse deliberately produce no cues: neither replays the
 * history it skipped visually, and audio must not narrate what the scene did
 * not show.
 */
export function presentAudio(presentation: GameViewPresentation): AudioCue[] {
  try {
    const settings = getAudioSnapshot();
    // Nothing is on: skip the derivation entirely rather than compute and drop.
    if (settings.muted && !settings.haptics) return [];
    const cues = deriveAudioCues(presentation);
    if (cues.length === 0) return cues;
    const { audio, haptics } = channelSinks();
    for (const cue of cues) {
      const gain = resolveCueGain(settings, cue.category);
      if (gain > 0) audio.play(cue, gain);
      if (resolveHaptic(settings, cue.category)) haptics.vibrate(cue);
    }
    return cues;
  } catch {
    // A hook may never break the scene. Silence is always an acceptable outcome.
    return [];
  }
}
