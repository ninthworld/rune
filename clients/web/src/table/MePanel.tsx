/**
 * The bottom shell's identity panel (ADR 0023): the receiver's crest (life),
 * nameplate, a quiet status line, floating mana, and — on the full composition —
 * **your piles** at the largest pile tier (the blueprint moves the receiver's
 * library/graveyard/exile off the board panel and into this panel). On the
 * compact composition it condenses to a one-line identity strip (name · mana ·
 * life), matching the phone mock's dock strip.
 *
 * Every value renders exactly as the view supplies it. Targeting mode keeps the
 * shared player-pick contract: when the receiver is a server-listed candidate the
 * panel becomes the pick surface; a non-candidate dims.
 */
import type { CSSProperties, ReactNode } from 'react';
import type { GameView, PlayerId } from '../protocol';
import {
  CommanderDamageTally,
  CommandPile,
  type BrowsableZone,
  type PlayerTargeting,
} from './PanelChrome';
import { cx } from '../chrome/cx';
import { identityAccent } from './identityAccents';
import { playerName } from '../playerNames';
import { zoneCountsOf } from './scene';
import { PileTopCard, ZonePile } from './ZonePile';
import s from './chrome.module.css';

interface Props {
  view: GameView;
  /** The receiver's id, when the server named it. */
  localId?: PlayerId;
  /** Condensed one-line variant (compact composition). */
  condensed?: boolean;
  /** Open the receiver's graveyard/exile browser (issue #262). */
  onOpenZone?: (playerId: PlayerId, zone: BrowsableZone) => void;
  /** Present only in targeting mode; makes the local player pickable when a candidate. */
  targeting?: PlayerTargeting;
  /** The player a game-log reference is highlighting, if any (issue #260). */
  highlightedId?: PlayerId | null;
}

/** How many attackers currently attack the receiver (issue #347), verbatim. */
function attackersOnMe(view: GameView, localId: PlayerId | undefined): number {
  if (localId === undefined) return 0;
  return view.battlefield.filter((perm) => perm.attacking_player === localId).length;
}

/** A quiet one-line reading of whose turn it is / who holds priority. */
function statusLine(view: GameView, localId: PlayerId | undefined): string {
  if (view.priority_player !== undefined && view.priority_player === localId) {
    return 'Priority — respond or pass';
  }
  if (view.active_player !== '' && view.active_player === localId) return 'Your turn';
  if (view.active_player !== '') return `${playerName(view, view.active_player)}'s turn`;
  return '';
}

export function MePanel({ view, localId, condensed, onOpenZone, targeting, highlightedId }: Props) {
  const id = localId ?? 'local';
  const name = localId !== undefined ? playerName(view, localId) : 'You';
  const accent = localId !== undefined ? identityAccent(view, localId) : undefined;
  const style = (accent ? { '--identity-accent': accent } : {}) as CSSProperties;
  const attacked = attackersOnMe(view, localId);
  const zones = localId !== undefined ? zoneCountsOf(view, localId, true) : undefined;

  const body: ReactNode = (
    <>
      <div className={s.meTop}>
        <span className={cx(s.lifeCrest, !condensed && s.lifeCrestBig)} title="Life">
          <span className={s.lifeCrestValue} data-testid={`hud-life-${id}`}>
            {view.me.life}
          </span>
        </span>
        <div className={s.meIdentity}>
          <div className={s.hudName} data-testid={`hud-name-${id}`}>
            {name} <span className={s.hudYou}>(you)</span>
          </div>
          {!condensed && <div className={s.meStatus}>{statusLine(view, localId)}</div>}
          {attacked > 0 && (
            <div className={s.hudAttacked} data-testid={`hud-attacked-${id}`}>
              Attacked ×{attacked}
            </div>
          )}
          {/* Commander damage the receiver has taken (issue #372), near their crest. */}
          {localId !== undefined && <CommanderDamageTally view={view} playerId={localId} />}
        </div>
      </div>
      {view.mana_pool.length > 0 && (
        <div className={s.hudMana} data-testid="hud-mana">
          Mana {view.mana_pool.join(' ')}
        </div>
      )}
      {/* Your piles, at the largest pile tier — the bottom shell owns them on the
          full composition (blueprint §Screen anatomy). */}
      {!condensed && zones && localId !== undefined && (
        <div className={s.mePiles} data-testid="me-piles">
          <ZonePile
            zone="library"
            playerLabel={name}
            count={zones.library}
            testId={`library-pile-${localId}`}
          />
          <ZonePile
            zone="graveyard"
            playerLabel={name}
            count={zones.graveyard}
            onOpen={onOpenZone ? () => onOpenZone(localId, 'graveyard') : undefined}
            faceUp={
              zones.graveyardTop && (
                <PileTopCard
                  name={zones.graveyardTop.name}
                  colorIdentity={zones.graveyardTop.colorIdentity}
                />
              )
            }
            testId={onOpenZone ? `table-graveyard-${localId}` : `graveyard-pile-${localId}`}
          />
          <ZonePile
            zone="exile"
            playerLabel={name}
            count={zones.exile}
            onOpen={onOpenZone ? () => onOpenZone(localId, 'exile') : undefined}
            testId={onOpenZone ? `table-exile-${localId}` : `exile-pile-${localId}`}
          />
          <CommandPile
            view={view}
            playerId={localId}
            playerLabel={name}
            count={zones.command ?? 0}
          />
        </div>
      )}
    </>
  );

  const highlightClass = highlightedId === id ? s.tileHighlighted : undefined;
  const className = cx(s.mePanel, condensed && s.mePanelCondensed, highlightClass);

  // The shared player-pick contract (ADR 0009 §Client): a candidate receiver is
  // pickable from their own panel; a non-candidate dims during a player pick.
  if (targeting?.candidates.includes(id)) {
    return (
      <button
        type="button"
        data-testid={`target-player-${id}`}
        aria-label={`Target player ${name}`}
        onClick={() => targeting.onPick(id)}
        className={cx(s.tileButtonReset, className, s.targetTile)}
        style={style}
      >
        {body}
      </button>
    );
  }
  return (
    <div
      data-testid={`tile-${id}`}
      data-highlighted={highlightedId === id || undefined}
      className={cx(className, targeting && s.dimmedTile)}
      style={style}
    >
      {body}
    </div>
  );
}
