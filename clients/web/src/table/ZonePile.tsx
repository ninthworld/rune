/**
 * A card-shaped **zone pile** — library, graveyard, or exile — parked in the
 * reserved pile column of a player's board region (issue #319 + the pile-column
 * redesign, React DOM per ADR 0003).
 *
 * Zones used to read as small header icons; each is now a card-proportioned pile
 * on the table itself — the library as a stacked card back, the graveyard showing
 * its public top card face-up in place, exile by its glyph — with the count worn
 * as a corner badge (its one and only home). Graveyard and exile piles open the
 * existing zone browser (issue #262) via pointer, touch, or keyboard focus; the
 * library pile is **count-only** — the client never holds the library beyond
 * counts and server-revealed subsets.
 *
 * A pile is a *place where a card can be shown*: the {@link ZonePileProps.faceUp}
 * slot renders a face-up card in the pile's frame. The graveyard fills it today
 * (its contents are public in the view); the library's stays reserved for a future
 * server reveal (`docs/design/ui-design-notes.md` §Zone piles — a protocol change).
 */
import type { ReactNode } from 'react';
import { Glyph, type GlyphName } from '../chrome/glyphs';
import { PALETTE, type ColorIdentity } from '../tokens';
import s from './chrome.module.css';

/** The board-region zones, each with a glyph and a count home. The command zone
 * (issue #372) joins the library/graveyard/exile piles wherever a player has one. */
export type PileZone = 'library' | 'graveyard' | 'exile' | 'command';

/** The glyph that identifies each pile. */
const ZONE_GLYPH: Record<PileZone, GlyphName> = {
  library: 'zone-library',
  graveyard: 'zone-graveyard',
  exile: 'zone-exile',
  command: 'zone-command',
};

/** Human labels for the accessible name. */
const ZONE_NAME: Record<PileZone, string> = {
  library: 'library',
  graveyard: 'graveyard',
  exile: 'exile',
  command: 'command',
};

interface ZonePileProps {
  /** Which zone this pile represents. */
  zone: PileZone;
  /** The controller's display label, for the accessible name (e.g. `"Rowan (you)"`). */
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
   * A face-up card to render in the pile's frame — the graveyard's public top card,
   * or a future server-revealed library top. See {@link PileTopCard}.
   */
  faceUp?: ReactNode;
  /** Test id for the pile element. */
  testId?: string;
}

/**
 * A face-up card shown in a pile's frame: the minimal honest face — frame color
 * (game information, from the shared card tokens) and name. Sized by the pile.
 */
export function PileTopCard({
  name,
  colorIdentity,
}: {
  name: string;
  colorIdentity: ColorIdentity;
}) {
  return (
    <span
      className={s.pileTopCard}
      data-testid="pile-top-card"
      style={{ borderColor: PALETTE[colorIdentity] }}
    >
      <span className={s.pileTopCardBand} style={{ background: PALETTE[colorIdentity] }} />
      <span className={s.pileTopCardName}>{name}</span>
    </span>
  );
}

/** The pile's frame contents: a revealed face-up card, or the zone glyph. */
function PileFace({ zone, count, faceUp }: { zone: PileZone; count: number; faceUp?: ReactNode }) {
  return (
    <span
      className={s.zonePileFace}
      data-zone={zone}
      data-empty={count === 0 || undefined}
      data-stacked={count > 1 || undefined}
    >
      {faceUp ?? <Glyph name={ZONE_GLYPH[zone]} size={22} />}
      <span className={s.zonePileCount} data-testid="pile-count">
        {count}
      </span>
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
        <PileFace zone={zone} count={count} faceUp={faceUp} />
        <span className={s.zonePileName}>{ZONE_NAME[zone]}</span>
      </button>
    );
  }

  return (
    <span data-testid={testId} className={s.zonePile} role="img" aria-label={label}>
      <PileFace zone={zone} count={count} faceUp={faceUp} />
      <span className={s.zonePileName}>{ZONE_NAME[zone]}</span>
    </span>
  );
}
