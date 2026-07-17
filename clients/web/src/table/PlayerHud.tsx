/**
 * Player HUD surfaces (React DOM, ADR 0003; issue #296).
 *
 * Two purpose-built surfaces replace the old uniform text tiles:
 *
 *  - {@link OpponentHud} — the strip across the top ({@link RegionId} `opponentHud`).
 *    One tile per opponent: identity + life prominent, hand count and statuses
 *    secondary. The strip reflows purely by opponent count — one wide tile at 2p,
 *    condensing toward a compact grid at 8p — via flex grow + wrap, so the local
 *    player is never pulled up from the bottom.
 *  - {@link LocalDock} — the receiver's own surface, docked bottom-left
 *    ({@link RegionId} `localDock`): identity, life, and floating mana (present only
 *    when non-empty, exactly as today).
 *
 * Every value is displayed exactly as the server provides it (life, hand size,
 * statuses); the client derives none. The hidden-zone counts (library / graveyard /
 * exile) live in exactly one home — each player's board lane in
 * {@link './TableGeography'.TableGeography} — so these surfaces never repeat them.
 *
 * Targeting mode (ADR 0009 §Client): when a target slot is active, a player who is a
 * server-listed candidate becomes pickable straight from their HUD surface (ringed, a
 * 44px min target); every non-candidate surface dims and goes inert. The client only
 * makes the server's candidates pickable — it derives no legality. This is exactly the
 * contract the retired `PlayerTiles` carried, preserved here for both surfaces.
 */
import type { EntityId, GameView, PlayerId } from '../protocol';
import type { ReactNode } from 'react';
import { cx } from '../chrome/cx';
import { playerName } from '../playerNames';
import s from './chrome.module.css';

/** The active target slot's player candidates plus the pick handler. */
export interface TargetingTiles {
  /** Entity ids that are legal targets for the active slot (players included). */
  candidates: EntityId[];
  /** Pick a player as the current slot's answer. */
  onPick: (id: EntityId) => void;
}

/**
 * Wrap one player surface's content, applying the shared targeting contract. Outside
 * targeting mode it is a plain labeled `<div>` (`tile-<id>`). Inside it, a candidate
 * becomes a pickable `<button>` (`target-player-<id>`, ringed, ≥44px); a non-candidate
 * stays a `<div>` but dims and is inert. Identical semantics for the opponent strip
 * and the local dock.
 */
function renderSurface(
  playerId: PlayerId,
  name: string,
  className: string,
  content: ReactNode,
  targeting: TargetingTiles | undefined,
  highlighted: boolean,
): ReactNode {
  // A game-log reference can presentationally highlight a player's tile (issue #260):
  // a ring, independent of targeting. Purely display — it makes nothing pickable.
  const highlightClass = highlighted ? s.tileHighlighted : undefined;
  if (!targeting) {
    return (
      <div
        key={playerId}
        data-testid={`tile-${playerId}`}
        className={cx(className, highlightClass)}
        data-highlighted={highlighted || undefined}
      >
        {content}
      </div>
    );
  }
  if (targeting.candidates.includes(playerId)) {
    return (
      <button
        key={playerId}
        type="button"
        data-testid={`target-player-${playerId}`}
        aria-label={`Target player ${name}`}
        onClick={() => targeting.onPick(playerId)}
        className={cx(s.tileButtonReset, className, s.targetTile, highlightClass)}
      >
        {content}
      </button>
    );
  }
  return (
    <div
      key={playerId}
      data-testid={`tile-${playerId}`}
      className={cx(className, s.dimmedTile, highlightClass)}
    >
      {content}
    </div>
  );
}

/** The identity + life + secondary-meta content shared by both surfaces. Life is the
 * prominent value; hand count and statuses are secondary. Values render verbatim. */
function surfaceBody(
  name: string,
  playerId: PlayerId,
  life: number,
  hand: number | undefined,
  statuses: string[] | undefined,
): ReactNode {
  return (
    <>
      <div className={s.hudName} data-testid={`hud-name-${playerId}`}>
        {name}
      </div>
      <div className={s.hudLife}>
        <span className={s.hudLifeLabel}>Life</span>
        <span className={s.hudLifeValue} data-testid={`hud-life-${playerId}`}>
          {life}
        </span>
      </div>
      {hand !== undefined && (
        <div className={s.hudMeta} data-testid={`hud-hand-${playerId}`}>
          Hand {hand}
        </div>
      )}
      {statuses && statuses.length > 0 && (
        <div className={s.hudStatuses} data-testid={`hud-statuses-${playerId}`}>
          {statuses.join(', ')}
        </div>
      )}
    </>
  );
}

interface OpponentHudProps {
  view: GameView;
  /** Present only in targeting mode; makes candidate opponents pickable. */
  targeting?: TargetingTiles;
  /** The player a game-log reference is highlighting, if any (issue #260). */
  highlightedId?: PlayerId | null;
}

/**
 * The opponent HUD strip. One tile per opponent, reflowing by count (wide at 2p →
 * grid at 8p) without moving the local player, who lives in the {@link LocalDock}.
 */
export function OpponentHud({ view, targeting, highlightedId }: OpponentHudProps) {
  return (
    <div data-testid="opponent-hud" className={s.hudStrip}>
      {view.opponents.map((opponent) => {
        const name = playerName(view, opponent.player_id);
        return renderSurface(
          opponent.player_id,
          name,
          s.hudTile,
          surfaceBody(
            name,
            opponent.player_id,
            opponent.life,
            opponent.hand_size,
            opponent.statuses,
          ),
          targeting,
          highlightedId === opponent.player_id,
        );
      })}
    </div>
  );
}

interface LocalDockProps {
  view: GameView;
  /** The receiver's id, when the server named it; falls back to a stable local key. */
  localId?: PlayerId;
  /** Present only in targeting mode; makes the local player pickable when a candidate. */
  targeting?: TargetingTiles;
  /** The player a game-log reference is highlighting, if any (issue #260). */
  highlightedId?: PlayerId | null;
}

/**
 * The local player dock (bottom-left): identity, life, and floating mana (only when
 * the pool is non-empty). The receiver's own statuses are not carried by
 * {@link SelfView}, so the dock shows none — it renders exactly what the view supplies.
 */
export function LocalDock({ view, localId, targeting, highlightedId }: LocalDockProps) {
  const id = localId ?? 'local';
  const name = localId !== undefined ? playerName(view, localId) : 'You';
  return (
    <div data-testid="local-dock" className={s.dock}>
      {renderSurface(
        id,
        name,
        s.hudTile,
        <>
          <div className={s.hudName} data-testid={`hud-name-${id}`}>
            {name} <span className={s.hudYou}>(you)</span>
          </div>
          <div className={s.hudLife}>
            <span className={s.hudLifeLabel}>Life</span>
            <span className={s.hudLifeValue} data-testid={`hud-life-${id}`}>
              {view.me.life}
            </span>
          </div>
          {view.mana_pool.length > 0 && (
            <div className={s.hudMana} data-testid="hud-mana">
              Mana {view.mana_pool.join(' ')}
            </div>
          )}
        </>,
        targeting,
        highlightedId === id,
      )}
    </div>
  );
}
