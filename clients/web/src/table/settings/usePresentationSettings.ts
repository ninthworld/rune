/**
 * Live subscription to the device-local presentation settings (issue #505).
 *
 * `useSyncExternalStore` over the {@link presentationSettings} observable so any
 * change — quality, density, or motion — re-renders every consumer immediately,
 * applying the choice without a reload. The snapshot is referentially stable
 * between changes, so unrelated re-renders never churn the scene.
 */
import { useSyncExternalStore } from 'react';
import {
  getPresentationSnapshot,
  subscribePresentation,
  type PresentationSettings,
} from './presentationSettings';

export function usePresentationSettings(): PresentationSettings {
  return useSyncExternalStore(
    subscribePresentation,
    getPresentationSnapshot,
    getPresentationSnapshot,
  );
}
