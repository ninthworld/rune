/**
 * The art pipeline, as React sees it.
 *
 * One provider around the whole app, because the preference is a *device* preference and the
 * registry is a *device* cache: a lobby, a deck builder, and a game all draw the same card and
 * must draw it the same way. Nothing here is game state — it is never sent, never received, and
 * never part of a view.
 *
 * `useCardArt` is the only thing a component needs. It answers with a texture or with nothing,
 * and nothing is the normal answer: no source chosen, a card not yet resolved, a card the source
 * does not have, a request that failed. Every one of those falls back to the procedural face, per
 * card, which is what ADR 0012 means by an unavailable illustration never blocking play.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react'

import type { CardFace } from './../card-face'
import { ArtStore } from './../art/store'
import { SCRYFALL, type ArtSource } from './../art/source'
import {
  DEFAULT_ART,
  readArtPreference,
  writeArtPreference,
  type ArtPreference,
  type ArtStyle,
} from './../art/settings'

/** The source a preference names, or none — which is the default and fetches nothing. */
const sourceFor = (preference: ArtPreference): ArtSource | undefined =>
  preference.source === 'scryfall' ? SCRYFALL : undefined

/** Absent in a browser that has storage disabled, which is a normal way to run. */
const deviceStorage = (): Storage | undefined => {
  try {
    return globalThis.localStorage
  } catch {
    return undefined
  }
}

interface ArtContextValue {
  preference: ArtPreference
  setPreference(preference: ArtPreference): void
  store: ArtStore
}

// The default is a live store with no source, so a component rendered outside the provider —
// a test, a fixture harness — behaves exactly as a device that has chosen nothing.
const ArtContext = createContext<ArtContextValue>({
  preference: DEFAULT_ART,
  setPreference: () => {},
  store: new ArtStore(),
})

export function ArtProvider({ children }: { children: ReactNode }) {
  const [preference, setStored] = useState(() => readArtPreference(deviceStorage()))
  const [store] = useState(
    () =>
      new ArtStore({
        storage: deviceStorage(),
        source: sourceFor(readArtPreference(deviceStorage())),
      }),
  )

  const setPreference = useCallback(
    (next: ArtPreference) => {
      writeArtPreference(deviceStorage(), next)
      setStored(next)
      // Turning a source off drops everything it resolved, which is the point: a player who
      // switched back to procedural has said they want no third-party images on screen.
      store.setSource(sourceFor(next))
    },
    [store],
  )

  const value = useMemo(
    () => ({ preference, setPreference, store }),
    [preference, setPreference, store],
  )
  return <ArtContext.Provider value={value}>{children}</ArtContext.Provider>
}

/** The preference and the way to change it, for the settings surface. */
export function useArtPreference(): {
  preference: ArtPreference
  setPreference(preference: ArtPreference): void
  clear(): void
} {
  const { preference, setPreference, store } = useContext(ArtContext)
  const clear = useCallback(() => store.clear(), [store])
  return { preference, setPreference, clear }
}

/** A texture for one card, and how it should be presented. `undefined` means draw procedurally. */
export interface CardArtTexture {
  url: string
  style: ArtStyle
}

export function useCardArt(face: CardFace): CardArtTexture | undefined {
  const { preference, store } = useContext(ArtContext)
  // A token has no card identity (CR 111), so there is nothing to key a cache by and nothing to
  // look up. It keeps its procedural face, which is the only stable one it can have.
  const key = face.artKey

  const subscribe = useCallback((listener: () => void) => store.subscribe(listener), [store])
  const entry = useSyncExternalStore(subscribe, () =>
    key === undefined ? undefined : store.get(key),
  )

  // Asked for in an effect rather than during render: a request is a side effect, and a render
  // that started one would fire it again on every re-render React chose to make.
  useEffect(() => {
    if (key !== undefined) store.request(key, face.name)
  }, [store, key, face.name, preference.source])

  if (entry?.status !== 'ready') return undefined

  // The full-card image is the only thing that can carry the `full` style, because that style
  // suppresses SAGE's own printed text on the grounds that it is on the image. With only an
  // illustration resolved there is no such image, so it is drawn in the frame instead — a
  // fallback per card, never a mode the player is silently moved into.
  if (preference.style === 'full' && entry.images.full) {
    return { url: entry.images.full, style: 'full' }
  }
  const window = entry.images.window ?? entry.images.full
  return window ? { url: window, style: 'window' } : undefined
}
