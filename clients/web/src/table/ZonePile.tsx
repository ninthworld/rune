/**
 * A card-shaped **zone pile** — library, graveyard, or exile — parked in a
 * consistent corner of a player's board region (issue #319, React DOM per ADR 0003).
 *
 * Zones used to read as text chips in the lane header; this makes each a findable
 * card-shaped object a player can point at, identified by its zone glyph (#317) and
 * a count. Graveyard and exile piles open the existing zone browser (issue #262) via
 * pointer, touch, or keyboard focus; the library pile is **count-only** — the client
 * never holds the library beyond counts and server-revealed subsets.
 *
 * A pile is a *place where a card can be shown*: the {@link ZonePileProps.faceUp}
 * slot renders a face-up card in the pile's frame (a future server-revealed library
 * top, `docs/design/ui-design-notes.md` §Zone piles). No protocol reveal exists
 * today, so the slot is normally empty — but the layout can already host it, without
 * change, which the component fixture demonstrates.
 */
import type { ReactNode } from 'react';
import { Glyph, type GlyphName } from '../chrome/glyphs';
import s from './chrome.module.css';

/** The three board-region zones, each with a glyph and a count home. */
export type PileZone = 'library' | 'graveyard' | 'exile';

/** The glyph that identifies each pile. */
const ZONE_GLYPH: Record<PileZone, GlyphName> = {
  library: 'zone-library',
  graveyard: 'zone-graveyard',
  exile: 'zone-exile',
};

/** Human labels for the accessible name. */
const ZONE_NAME: Record<PileZone, string> = {
  library: 'library',
  graveyard: 'graveyard',
  exile: 'exile',
};

interface ZonePileProps {
  /** Which zone this pile represents. */
  zone: PileZone;
  /** The controller's display label, for the accessible name (e.g. `"p1 (you)"`). */
  playerLabel: string;
  /** The pile's card count, shown in its one and only home. */
  count: number;
  /**
   * Open this pile's browser (graveyard/exile only). When provided the pile is a
   * focusable button; absent ⇒ a static, non-interactive pile (the library, always;
   * graveyard/exile on a read-only board).
   */
  onOpen?: () => void;
  /**
   * A face-up card to render in the pile's frame — a server-revealed top card. Absent
   * today (no reveal in the protocol); the slot proves the layout can host one.
   */
  faceUp?: ReactNode;
  /** Test id for the pile element. */
  testId?: string;
}

/** The pile's frame contents: a revealed face-up card, or the zone glyph. */
function PileFace({ zone, faceUp }: { zone: PileZone; faceUp?: ReactNode }) {
  return (
    <span className={s.zonePileFace} data-zone={zone}>
      {faceUp ?? <Glyph name={ZONE_GLYPH[zone]} size={20} />}
    </span>
  );
}

export function ZonePile({ zone, playerLabel, count, onOpen, faceUp, testId }: ZonePileProps) {
  const label = `${playerLabel} ${ZONE_NAME[zone]} (${count})`;

  if (onOpen) {
    return (
      <button
        type="button"
        data-testid={testId}
        aria-label={`Browse ${label}`}
        className={s.zonePile}
        onClick={onOpen}
      >
        <PileFace zone={zone} faceUp={faceUp} />
        <span className={s.zonePileCount}>{count}</span>
      </button>
    );
  }

  return (
    <span data-testid={testId} className={s.zonePile} role="img" aria-label={label}>
      <PileFace zone={zone} faceUp={faceUp} />
      <span className={s.zonePileCount}>{count}</span>
    </span>
  );
}
