/**
 * The sound and haptic hook layer (issue #507), keyed to the shared effect
 * taxonomy of `docs/design/visual-system.md` §9 and
 * `docs/design/asset-pipeline.md`. Optional, independently muted, and never
 * load-bearing: with no assets registered the whole layer is silence.
 */
export {
  AUDIO_CUE_CATEGORIES,
  AUDIO_CUE_LABELS,
  SILENT_AUDIO_SINK,
  SILENT_HAPTIC_SINK,
  type AudioChannelSinks,
  type AudioCue,
  type AudioCueCategory,
  type AudioSink,
  type HapticSink,
} from './types';
export { deriveAudioCues, MOTION_CUE_CATEGORY } from './cues';
export { presentAudio, resetAudioChannel, setAudioSinks } from './channel';
export { defaultAudioContextFactory, WebAudioSink, type WebAudioSinkOptions } from './webAudioSink';
export { hapticsSupported, HAPTIC_PATTERN, VibrationHapticSink } from './haptics';
export { synthesizeCueTone } from './devTones';
