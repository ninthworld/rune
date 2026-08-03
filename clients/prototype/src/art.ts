import { useEffect, useState } from "react";

/* Card imagery, fetched by the player's own browser and by nothing else.
   The prototype ships no images and bundles none: a card names the real
   card its pictures come from, and everything else about it stays ours.

   What comes back is cached in localStorage, so flipping between views or
   reloading the page doesn't ask Scryfall the same question twice. */
export type Art = { art: string; full: string };

const KEY = "sage-art-v1";

function stored(): [string, Art][] {
  try {
    return Object.entries(JSON.parse(localStorage.getItem(KEY) ?? "{}"));
  } catch {
    return [];
  }
}

const cache = new Map<string, Art>(stored());
const pending = new Map<string, Promise<Art | null>>();

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(Object.fromEntries(cache)));
  } catch {
    /* a full quota is not worth a broken board */
  }
}

/* What the cache holds, for the settings menu to report. The entries are
   URLs; the pictures themselves live in the browser's own store, which no
   page can measure — so the size is modelled from a typical card's art
   crop and full face rather than claimed as exact. */
const PER_CARD = 236_000;

export function cachedCards() {
  return cache.size;
}

export function estimateBytes(cards: number) {
  return cards * PER_CARD;
}

export function clearArt() {
  cache.clear();
  save();
}

/* Scryfall asks for a gap between requests. One queue keeps that honest
   however many cards come on screen at once. */
let queue: Promise<unknown> = Promise.resolve();
const GAP = 120;

function fetchArt(name: string): Promise<Art | null> {
  const job = queue.then(async (): Promise<Art | null> => {
    await new Promise((done) => setTimeout(done, GAP));
    const res = await fetch(
      `https://api.scryfall.com/cards/named?exact=${encodeURIComponent(name)}`,
    );
    if (!res.ok) return null;
    const data = await res.json();
    /* a two-faced card still only needs a front */
    const uris = data.image_uris ?? data.card_faces?.[0]?.image_uris;
    if (!uris?.art_crop) return null;
    const art: Art = { art: uris.art_crop, full: uris.png ?? uris.normal };
    cache.set(name, art);
    save();
    return art;
  });
  queue = job.catch(() => undefined);
  return job.catch(() => null);
}

export function useArt(name?: string): Art | null {
  const [art, setArt] = useState<Art | null>(() => (name ? cache.get(name) ?? null : null));
  useEffect(() => {
    if (!name) {
      setArt(null);
      return;
    }
    const have = cache.get(name);
    if (have) {
      setArt(have);
      return;
    }
    let live = true;
    let job = pending.get(name);
    if (!job) {
      job = fetchArt(name);
      pending.set(name, job);
    }
    job.then((found) => {
      pending.delete(name);
      if (live) setArt(found);
    });
    return () => {
      live = false;
    };
  }, [name]);
  return art;
}
