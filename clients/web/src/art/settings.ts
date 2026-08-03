/**
 * Where a player's card art comes from, as a device preference.
 *
 * ADR 0012 is the whole of the policy this implements, and two of its rules are the reason this
 * is a *preference* rather than a feature:
 *
 * - **The project ships imageless.** Nothing is bundled, nothing is served, nothing is proxied.
 *   What the third-party source means is that the *player's own browser* fetches images, at that
 *   player's own request, and caches them on that player's own device.
 * - **It is opt-in, and off until chosen.** `procedural` is the default and the only value a
 *   fresh device has. Turning on a source is a deliberate act taken in front of a plain
 *   description of what it does — which is what the ADR's consent step is, and why nothing here
 *   infers consent from anything else.
 *
 * The preference is stored per device and is not game state. It survives nothing being sent to
 * any server, it is never part of a `GameView`, and a client whose storage is empty or unreadable
 * simply draws procedurally — which is the same screen, with a different picture in one window.
 */

/** Which source resolves an illustration. `procedural` fetches nothing. */
export type ArtSourceId = 'procedural' | 'scryfall'

/**
 * How much of a card's face is ours (`docs/client-design.md` §6, §9.6).
 *
 * - `frame`: the card is entirely SAGE's and nothing is fetched at all. The art window is a
 *   field tinted by the printed cost — which is what the design settled on in place of the
 *   procedural composition an earlier draft drew there.
 * - `window`: only the illustration is drawn, inside SAGE's own frame. The name band, the pips,
 *   the type line, and the stat are still SAGE's.
 * - `full`: the whole card image is the face. The printed text is suppressed because it is on
 *   the image — but every server-computed overlay stays on top of it, so a buffed 4/4 never
 *   reads as its printed 2/2.
 *
 * The style and the source govern each other, and the rule is §9.6's: turning the source off
 * returns the style to `frame`, because the two faces that need pictures cannot be drawn without
 * one.
 */
export type ArtStyle = 'frame' | 'window' | 'full'

export interface ArtPreference {
  source: ArtSourceId
  style: ArtStyle
  /**
   * Whether a `full` card face also wears SAGE's **name band** — the card's name and its mana
   * cost as pips — over the printed one.
   *
   * Off by default, because the whole point of `full` is that the printed card is the face and a
   * second name over the first is a duplicate. It is offered because the printed band is the
   * part of a fetched image that suffers most: at a board card's size an official title bar is
   * unreadable, and the cost is a row of symbols this client draws far larger than the print. A
   * player who wants their board scannable turns it on and gets the one band that is worth
   * repeating.
   *
   * It governs only the band. Everything the *server computed* — a buffed 4/4, counters, damage,
   * markers — is drawn over a full face unconditionally and is not a preference, because a
   * printed 2/2 standing in for a 4/4 is a wrong board rather than a plainer one.
   */
  fullArtBand: boolean
}

/** What a device with no stored preference has: nothing downloads, and offline play is normal. */
export const DEFAULT_ART: ArtPreference = {
  source: 'procedural',
  style: 'frame',
  fullArtBand: false,
}

const KEY = 'sage.art.preference.v1'

/**
 * Read the stored preference, or the default.
 *
 * Every failure lands on the default: no storage (a private window with it disabled), unparseable
 * contents, a value from a build that offered a source this one does not. Art is cache and never
 * state, so falling back is always correct and never loses anything a player cannot re-choose.
 */
export function readArtPreference(storage: Storage | undefined): ArtPreference {
  if (!storage) return DEFAULT_ART
  try {
    const raw: unknown = JSON.parse(storage.getItem(KEY) ?? 'null')
    if (typeof raw !== 'object' || raw === null) return DEFAULT_ART
    const stored = raw as Partial<ArtPreference>
    return {
      source: stored.source === 'scryfall' ? 'scryfall' : 'procedural',
      style: stored.style === 'full' ? 'full' : stored.style === 'window' ? 'window' : 'frame',
      // A device stored before this option existed simply has the default, which is off.
      fullArtBand: stored.fullArtBand === true,
    }
  } catch {
    return DEFAULT_ART
  }
}

/** Store the preference, or carry on without storing it. A device that cannot remember still plays. */
export function writeArtPreference(storage: Storage | undefined, preference: ArtPreference): void {
  try {
    storage?.setItem(KEY, JSON.stringify(preference))
  } catch {
    // Full, disabled, or denied. The preference still applies to this session.
  }
}
