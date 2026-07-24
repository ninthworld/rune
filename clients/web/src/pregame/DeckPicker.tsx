/**
 * The room's deck column (issue #506; `front-door-and-lobby.md` §5.3, right).
 *
 * The bundled starter decks as selectable tiles plus **Build a deck**. The tiles
 * were already migrated onto the 2.5D visual system by #508 and keep their
 * `--deck-*` scene-token dressing verbatim — this module only relocates them
 * into the room's right column and adds the format's designated-commander line
 * where the advertised rules require one.
 *
 * No card logic: the bundled decklists are static names/ids (`decklists.ts`),
 * the land glyphs are display-only reads of that static data, and the server
 * validates a submitted deck authoritatively behind the unchanged `submit_deck`
 * gate. Saved decks stay inside the builder (ADR 0027, unchanged).
 */
import { cx } from '../chrome/cx';
import { Glyph } from '../chrome/glyphs';
import { deckSceneVars } from '../deck/deckScene';
import { decklistSize, type Decklist } from '../decklists';
import { deckLandGlyphs } from './deckPresentation';
import l from '../screens.module.css';

/**
 * One starter deck as a selectable tile. Selection is the blue ring, carried for
 * assistive tech by `aria-pressed` — never color alone.
 */
export function DeckTile({
  deck,
  selected,
  onSelect,
}: {
  deck: Decklist;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className={cx(l.deckTile, selected && l.deckTileSelected)}
      aria-pressed={selected}
      onClick={() => onSelect(deck.id)}
      data-testid={`deck-tile-${deck.id}`}
    >
      <span className={l.deckTileHead}>
        <span className={l.deckName}>{deck.name}</span>
        <span className={l.deckGlyphs}>
          {deckLandGlyphs(deck).map((land) => (
            <span key={land.glyph} style={{ color: land.hue, display: 'inline-flex' }}>
              <Glyph name={land.glyph} size={16} label={land.name} />
            </span>
          ))}
        </span>
      </span>
      <span className={l.deckSummary}>{deck.summary}</span>
      <span className={l.deckMeta}>{decklistSize(deck)} cards</span>
    </button>
  );
}

/** The starter-deck grid, carrying the #508 scene tokens the tiles read. */
export function DeckGrid({
  decks,
  selectedId,
  onSelect,
  reducedMotion,
  label,
}: {
  decks: readonly Decklist[];
  selectedId: string;
  onSelect: (id: string) => void;
  reducedMotion: boolean;
  label: string;
}) {
  return (
    <div
      className={l.deckGrid}
      role="group"
      aria-label={label}
      style={deckSceneVars(reducedMotion)}
    >
      {decks.map((deck) => (
        <DeckTile key={deck.id} deck={deck} selected={deck.id === selectedId} onSelect={onSelect} />
      ))}
    </div>
  );
}
