/**
 * Live subscription to the device-local sound/haptic settings (issue #507).
 *
 * `useSyncExternalStore` over the {@link audioSettings} observable, exactly like
 * {@link usePresentationSettings}: a mute, a volume change, or a haptics toggle
 * re-renders every consumer immediately, with no reload. The snapshot is
 * referentially stable between changes.
 */
import { useSyncExternalStore } from 'react';
import { getAudioSnapshot, subscribeAudio, type AudioSettings } from './audioSettings';

export function useAudioSettings(): AudioSettings {
  return useSyncExternalStore(subscribeAudio, getAudioSnapshot, getAudioSnapshot);
}
