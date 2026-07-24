/**
 * Device-local sound and haptic settings (issue #507), on the same surface and
 * the same idiom as the presentation settings of issue #505.
 *
 * These are device *preferences* — never game state, never protocol — exactly
 * like the card-art source (ADR 0024), the saved-deck carve-out (ADR 0027), and
 * the quality/density/motion trio next door. Storage is guarded: where
 * `localStorage` is unavailable the settings degrade to their defaults and
 * saving is a no-op. The module is an observable singleton so a change applies
 * without a reload (the `useSyncExternalStore` contract).
 *
 * **Defaults ship silent and un-buzzing.** The project bundles no production
 * audio (ADR 0031 — the ledger and its CI gate land with the first real asset),
 * so an unmuted default would promise a sound that does not exist, and browser
 * autoplay policy would suppress the first one anyway. Haptics are opt-in on
 * top of that, per the visual-system §9 rule that every hook is optional.
 */
import { AUDIO_CUE_CATEGORIES, type AudioCueCategory } from '../audio/types';

/** The device-local sound and haptic preferences. */
export interface AudioSettings {
  /** Master mute. Defaults to `true`; nothing is audible until it is cleared. */
  muted: boolean;
  /** Master volume, 0–1. Applies to every category. */
  volume: number;
  /** Categories the player silenced individually. Absent ⇒ audible. */
  mutedCategories: ReadonlySet<AudioCueCategory>;
  /** Vibration API haptics; **off by default**, and gated per category too. */
  haptics: boolean;
}

/** `localStorage` keys for the device-local sound preferences. */
const MUTED_KEY = 'rune.audio.muted';
const VOLUME_KEY = 'rune.audio.volume';
const CATEGORIES_KEY = 'rune.audio.muted-categories';
const HAPTICS_KEY = 'rune.audio.haptics';

/** The default master volume — a moderate level for the first un-mute. */
export const DEFAULT_AUDIO_VOLUME = 0.6;

/** Read one stored string, guarded; `null` when absent or unavailable. */
function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Persist one preference; a no-op where storage is unavailable. */
function writeStored(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage unavailable — the choice simply doesn't survive a reload.
  }
}

/** Clamp an arbitrary number into the 0–1 volume range. */
export function clampVolume(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_AUDIO_VOLUME;
  return Math.min(1, Math.max(0, value));
}

/** Parse the stored muted-category list, dropping anything unrecognized. */
function parseCategories(stored: string | null): Set<AudioCueCategory> {
  const known = new Set<string>(AUDIO_CUE_CATEGORIES);
  const parsed = (stored ?? '')
    .split(',')
    .map((entry) => entry.trim())
    .filter((entry): entry is AudioCueCategory => known.has(entry));
  return new Set(parsed);
}

/** Serialize the muted-category set in taxonomy order (stable round trip). */
function serializeCategories(categories: ReadonlySet<AudioCueCategory>): string {
  return AUDIO_CUE_CATEGORIES.filter((category) => categories.has(category)).join(',');
}

interface StoreState {
  settings: AudioSettings;
  listeners: Set<() => void>;
}

let state: StoreState | null = null;

/** Build fresh settings from storage, falling back to the silent defaults. */
function loadState(): StoreState {
  const storedVolume = Number.parseFloat(readStored(VOLUME_KEY) ?? '');
  return {
    settings: {
      muted: readStored(MUTED_KEY) !== 'false',
      volume: Number.isFinite(storedVolume) ? clampVolume(storedVolume) : DEFAULT_AUDIO_VOLUME,
      mutedCategories: parseCategories(readStored(CATEGORIES_KEY)),
      haptics: readStored(HAPTICS_KEY) === 'true',
    },
    listeners: new Set(),
  };
}

/** The live store, constructed on first access. */
function store(): StoreState {
  state ??= loadState();
  return state;
}

/** Drop the stored settings and re-read from storage — a fresh store (tests). */
export function resetAudioSettings(): void {
  state = null;
}

/** Subscribe to preference changes; returns the unsubscribe function. */
export function subscribeAudio(listener: () => void): () => void {
  const s = store();
  s.listeners.add(listener);
  return () => s.listeners.delete(listener);
}

/** The current snapshot — referentially stable between changes. */
export function getAudioSnapshot(): AudioSettings {
  return store().settings;
}

/** Replace the settings object (new reference) and republish. */
function commit(next: AudioSettings): void {
  const s = store();
  s.settings = next;
  for (const listener of s.listeners) listener();
}

/** Set the master mute; persists and republishes. */
export function setAudioMuted(muted: boolean): void {
  const current = getAudioSnapshot();
  if (current.muted === muted) return;
  writeStored(MUTED_KEY, String(muted));
  commit({ ...current, muted });
}

/** Set the master volume (clamped to 0–1); persists and republishes. */
export function setAudioVolume(volume: number): void {
  const current = getAudioSnapshot();
  const next = clampVolume(volume);
  if (current.volume === next) return;
  writeStored(VOLUME_KEY, String(next));
  commit({ ...current, volume: next });
}

/** Mute or unmute one taxonomy category; persists and republishes. */
export function setCategoryMuted(category: AudioCueCategory, muted: boolean): void {
  const current = getAudioSnapshot();
  if (current.mutedCategories.has(category) === muted) return;
  const next = new Set(current.mutedCategories);
  if (muted) next.add(category);
  else next.delete(category);
  writeStored(CATEGORIES_KEY, serializeCategories(next));
  commit({ ...current, mutedCategories: next });
}

/** Turn haptics on or off; persists and republishes. */
export function setHapticsEnabled(haptics: boolean): void {
  const current = getAudioSnapshot();
  if (current.haptics === haptics) return;
  writeStored(HAPTICS_KEY, String(haptics));
  commit({ ...current, haptics });
}

/**
 * Whether a category passes the per-category control. Independent of the master
 * mute so the same switch gates audio and haptics — visual-system §9's
 * "independently muted" applies to both channels.
 */
export function isCategoryEnabled(settings: AudioSettings, category: AudioCueCategory): boolean {
  return !settings.mutedCategories.has(category);
}

/**
 * The linear gain one cue category plays at: `0` whenever the master mute, the
 * category mute, or a zero volume silences it. A zero never reaches a sink.
 */
export function resolveCueGain(settings: AudioSettings, category: AudioCueCategory): number {
  if (settings.muted || !isCategoryEnabled(settings, category)) return 0;
  return clampVolume(settings.volume);
}

/** Whether a cue category may buzz: haptics on **and** the category enabled. */
export function resolveHaptic(settings: AudioSettings, category: AudioCueCategory): boolean {
  return settings.haptics && isCategoryEnabled(settings, category);
}
