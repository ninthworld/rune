/**
 * Device-local presentation settings (issue #505): the quality level, effect
 * density, and motion preference this player chose on THIS device.
 *
 * These are presentation *preferences*, never game state — so, exactly like the
 * card-art source (ADR 0024) and the saved-deck carve-out (ADR 0027), they live
 * on the device and never touch the protocol. The whole in-game UI must still
 * reconstruct from one `GameView`; nothing here is load-bearing across messages.
 * It only sizes effects, scales motion, and toggles environment animation — the
 * scene (perspective plane, staging, cards, tap/travel motion) is never degraded
 * at any level (`docs/design/presentation-budgets.md` §Quality levels).
 *
 * Storage is guarded (`localStorage`, per the art-source idiom): where it is
 * unavailable — privacy modes, SSR, tests without a stub — the settings degrade
 * to their auto-detected / default values and saving is a no-op.
 *
 * The module is an observable singleton so a change applies WITHOUT a reload:
 * every reader subscribes through {@link subscribePresentation} (the
 * `useSyncExternalStore` contract), and a setter both persists and republishes.
 */
import type { EffectDensity, EffectQuality } from '../effects';
import { DEFAULT_SCENE_THEME, isSceneThemeName, type SceneThemeName } from '../../sceneTokens';

/**
 * The motion preference, composed with the OS `prefers-reduced-motion` query:
 * - `system` — follow the OS setting.
 * - `reduced` — force reduced motion even with the OS setting off.
 * - `full` — prefer full motion when the OS allows it.
 *
 * The OS request is authoritative: an accessibility setting is never overridden
 * by an in-app preference, so `full` still yields to an OS "reduce" (issue #505;
 * **OS-on OR user-on ⇒ reduced**). The reduced-motion contract (snap every
 * animation to its end state with zero layout or state difference) is unchanged;
 * this only chooses *when* it engages.
 */
export type MotionPreference = 'system' | 'reduced' | 'full';

/** The three device-local presentation preferences plus their provenance. */
export interface PresentationSettings {
  /** Effect quality level (sizes the particle pool; never degrades the scene). */
  quality: EffectQuality;
  /** Effect density — a spawn multiplier, independent of the quality level. */
  density: EffectDensity;
  /** Motion preference, composed with the OS reduced-motion query. */
  motion: MotionPreference;
  /**
   * The battlefield environment theme (`docs/design/environment-system.md` §11).
   * A **presentation** preference in exactly the idiom of the three above: per
   * device, per client, never a `GameView` field, never a lobby field, never
   * sent to the server. Two players in one match may see different themes and a
   * spectator a third; the whole UI still rebuilds from one `GameView`, because
   * the theme only selects a palette and a set of plates.
   */
  environmentTheme: SceneThemeName;
  /**
   * Whether `quality` is still the auto-detected first-run default (no user
   * override yet). The settings surface shows the detected level rather than
   * applying it silently.
   */
  qualityAutoDetected: boolean;
}

/** `localStorage` keys for the device-local presentation preferences. */
const QUALITY_KEY = 'rune.presentation.quality';
const DENSITY_KEY = 'rune.presentation.density';
const MOTION_KEY = 'rune.presentation.motion';
const ENVIRONMENT_THEME_KEY = 'rune.presentation.environmentTheme';

function isQuality(value: string | null): value is EffectQuality {
  return value === 'high' || value === 'standard' || value === 'lite';
}

function isDensity(value: string | null): value is EffectDensity {
  return value === 'full' || value === 'reduced' || value === 'minimal';
}

function isMotion(value: string | null): value is MotionPreference {
  return value === 'system' || value === 'reduced' || value === 'full';
}

/** Remove one stored preference; a no-op where storage is unavailable. */
function clearStored(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // Storage unavailable — nothing was stored to begin with.
  }
}

/** Read one stored preference, guarded; `null` when absent or unavailable. */
function readStored<T extends string>(key: string, is: (v: string | null) => v is T): T | null {
  try {
    const stored = localStorage.getItem(key);
    return is(stored) ? stored : null;
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

/**
 * Coarse device-capability signals used for the conservative first-run default.
 * Each is optional because browser support varies; a missing signal is treated
 * as "no evidence of a low-capability device" (biased toward Standard).
 */
export interface DeviceSignals {
  /** `navigator.deviceMemory` (approx. GB), where the browser reports it. */
  deviceMemory?: number;
  /** `navigator.hardwareConcurrency` (logical cores), where reported. */
  hardwareConcurrency?: number;
  /** `navigator.connection.saveData` — the user asked to conserve data. */
  saveData?: boolean;
}

/** The outcome of first-run detection: the level plus the reason shown to the user. */
export interface QualityDetection {
  quality: EffectQuality;
  reason: string;
}

/** Read the device signals from the environment, guarded for SSR/tests. */
export function readDeviceSignals(): DeviceSignals {
  if (typeof navigator === 'undefined') return {};
  const nav = navigator as Navigator & {
    deviceMemory?: number;
    connection?: { saveData?: boolean };
  };
  return {
    deviceMemory: typeof nav.deviceMemory === 'number' ? nav.deviceMemory : undefined,
    hardwareConcurrency:
      typeof nav.hardwareConcurrency === 'number' ? nav.hardwareConcurrency : undefined,
    saveData: nav.connection?.saveData === true ? true : undefined,
  };
}

/**
 * The conservative first-run default (issue #505, budgets §Quality levels):
 * default to **Standard** and only drop to **Lite** on a clear low-capability
 * signal — misdetecting a capable device down to Lite is worse than starting at
 * Standard. **High is never auto-selected**; it is an explicit opt-in. Always
 * overridable, and the chosen level is shown, never applied silently.
 */
export function detectDefaultQuality(
  signals: DeviceSignals = readDeviceSignals(),
): QualityDetection {
  if (signals.saveData === true) {
    return { quality: 'lite', reason: 'Data Saver is on, so effects start at Lite.' };
  }
  if (signals.deviceMemory !== undefined && signals.deviceMemory < 4) {
    return {
      quality: 'lite',
      reason: `This device reports about ${signals.deviceMemory} GB of memory, so effects start at Lite.`,
    };
  }
  if (signals.hardwareConcurrency !== undefined && signals.hardwareConcurrency <= 2) {
    return {
      quality: 'lite',
      reason: `This device reports ${signals.hardwareConcurrency} CPU core${
        signals.hardwareConcurrency === 1 ? '' : 's'
      }, so effects start at Lite.`,
    };
  }
  return { quality: 'standard', reason: 'Defaulted to Standard for this device.' };
}

/**
 * Compose the motion preference with the OS `prefers-reduced-motion` query.
 * The OS request is authoritative — an accessibility setting is never overridden
 * by an in-app preference — so any OS "reduce" yields reduced motion regardless
 * of the preference. `reduced` additionally forces the snap with the OS setting
 * off; `full` and `system` follow the OS. In short, **OS-on OR user-on ⇒
 * reduced** (the budgets' orthogonal reduced-motion control).
 */
export function resolveReducedMotion(osReduced: boolean, motion: MotionPreference): boolean {
  if (osReduced) return true;
  return motion === 'reduced';
}

interface StoreState {
  settings: PresentationSettings;
  detection: QualityDetection;
  listeners: Set<() => void>;
}

let state: StoreState | null = null;

/**
 * Read the stored environment theme, applying the §8.3 stale-key rule: an
 * unknown value (a theme renamed or removed since it was written) resolves to
 * the default **and rewrites the stored key**, so the bad value is corrected on
 * first read rather than re-resolved on every mount.
 */
function loadEnvironmentTheme(): SceneThemeName {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(ENVIRONMENT_THEME_KEY);
  } catch {
    return DEFAULT_SCENE_THEME;
  }
  if (raw === null) return DEFAULT_SCENE_THEME;
  if (isSceneThemeName(raw)) return raw;
  clearStored(ENVIRONMENT_THEME_KEY);
  return DEFAULT_SCENE_THEME;
}

/** Build fresh state from storage, running first-run detection when unset. */
function loadState(): StoreState {
  const density = readStored(DENSITY_KEY, isDensity) ?? 'reduced';
  const motion = readStored(MOTION_KEY, isMotion) ?? 'system';
  const storedQuality = readStored(QUALITY_KEY, isQuality);
  const detection = detectDefaultQuality();
  const settings: PresentationSettings = {
    quality: storedQuality ?? detection.quality,
    density,
    motion,
    environmentTheme: loadEnvironmentTheme(),
    qualityAutoDetected: storedQuality === null,
  };
  return { settings, detection, listeners: new Set() };
}

/** The live store, constructed on first access. */
function store(): StoreState {
  state ??= loadState();
  return state;
}

/** Drop stored effects and re-read from storage — a fresh store (tests). */
export function resetPresentationSettings(): void {
  state = null;
}

/** Notify subscribers that a preference changed. */
function bump(s: StoreState): void {
  for (const listener of s.listeners) listener();
}

/** Subscribe to preference changes; returns the unsubscribe function. */
export function subscribePresentation(listener: () => void): () => void {
  const s = store();
  s.listeners.add(listener);
  return () => s.listeners.delete(listener);
}

/**
 * The current settings snapshot. Referentially stable between changes so it is
 * a safe `useSyncExternalStore` snapshot (a new object is created only when a
 * setter actually changes a value).
 */
export function getPresentationSnapshot(): PresentationSettings {
  return store().settings;
}

/** The first-run auto-detection outcome (the level + the reason shown to the user). */
export function getQualityDetection(): QualityDetection {
  return store().detection;
}

/** Replace the settings object (new reference) and republish. */
function commit(next: PresentationSettings): void {
  const s = store();
  s.settings = next;
  bump(s);
}

/** Choose the effect quality level; persists and clears the auto-detected flag. */
export function setQuality(quality: EffectQuality): void {
  const current = getPresentationSnapshot();
  if (current.quality === quality && !current.qualityAutoDetected) return;
  writeStored(QUALITY_KEY, quality);
  commit({ ...current, quality, qualityAutoDetected: false });
}

/** Choose the effect density; persists and republishes. */
export function setDensity(density: EffectDensity): void {
  const current = getPresentationSnapshot();
  if (current.density === density) return;
  writeStored(DENSITY_KEY, density);
  commit({ ...current, density });
}

/** Choose the motion preference; persists and republishes. */
export function setMotion(motion: MotionPreference): void {
  const current = getPresentationSnapshot();
  if (current.motion === motion) return;
  writeStored(MOTION_KEY, motion);
  commit({ ...current, motion });
}

/**
 * Choose the battlefield environment theme; persists and republishes
 * (`environment-system.md` §11). Permitted mid-match: the manifest re-resolves
 * and the layers cross-fade on the `staging` class, with reduced motion snapping
 * — the match is never interrupted and nothing is re-fetched before it is asked
 * for. An unknown value is ignored rather than stored.
 */
export function setEnvironmentTheme(theme: SceneThemeName): void {
  if (!isSceneThemeName(theme)) return;
  const current = getPresentationSnapshot();
  if (current.environmentTheme === theme) return;
  writeStored(ENVIRONMENT_THEME_KEY, theme);
  commit({ ...current, environmentTheme: theme });
}
