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
import type { CSSProperties, ReactNode } from 'react';
import type { EntityId, GameView, PlayerId } from '../protocol';
import { cx } from '../chrome/cx';
import { identityAccent } from './identityAccents';
import { playerName } from '../playerNames';
import s from './chrome.module.css';

/**
 * The tile style carrying a player's identity accent (§Identity) as a CSS custom
 * property the chrome classes read (`--identity-accent`): the tile's edge, the
 * nameplate, and the life crest all tint from this one value.
 */
function accentStyle(view: GameView, playerId: PlayerId): CSSProperties {
  return { '--identity-accent': identityAccent(view, playerId) } as CSSProperties;
}

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
  focusLabel?: string,
  style?: CSSProperties,
): ReactNode {
  // A game-log reference can presentationally highlight a player's tile (issue #260):
  // a ring, independent of targeting. Purely display — it makes nothing pickable.
  const highlightClass = highlighted ? s.tileHighlighted : undefined;
  if (!targeting) {
    // Outside targeting the tile is display-only, but keyboard/controller focus must
    // still reach every player area on a multiplayer table (issue #348): a
    // `focusLabel` makes the tile a focusable, screen-reader-labeled navigation
    // anchor (`data-focus-item`, part of the `opponentHud` focus region) that reads
    // the player's public state when focused. It makes nothing *pickable* — there is
    // no action here; picking a player only happens in targeting mode below.
    return (
      <div
        key={playerId}
        data-testid={`tile-${playerId}`}
        className={cx(className, highlightClass)}
        data-highlighted={highlighted || undefined}
        style={style}
        {...(focusLabel !== undefined
          ? { tabIndex: 0, 'data-focus-item': '', role: 'group', 'aria-label': focusLabel }
          : {})}
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
        style={style}
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
      style={style}
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
  underAttack: number,
): ReactNode {
  return (
    <>
      <div className={s.hudName} data-testid={`hud-name-${playerId}`}>
        {name}
      </div>
      {/* Life is the identity moment the tile leads with (§Identity): a rune-framed
          crest in the player's accent, display-face numerals. Value verbatim. */}
      <div className={s.hudLife}>
        <span className={s.lifeCrest}>
          <span className={s.lifeCrestValue} data-testid={`hud-life-${playerId}`}>
            {life}
          </span>
        </span>
        <span className={s.hudLifeLabel}>Life</span>
      </div>
      {hand !== undefined && (
        <div className={s.hudMeta} data-testid={`hud-hand-${playerId}`}>
          Hand {hand}
        </div>
      )}
      {/* Whom the attackers point at (issue #347): a player being attacked shows how
          many attackers are coming, so the attack treatment reads *toward the attacked
          player's HUD tile* and a bystander can tell who is under attack. Reconstructed
          from the view; the client derives no combat. */}
      {underAttack > 0 && (
        <div className={s.hudAttacked} data-testid={`hud-attacked-${playerId}`}>
          Attacked ×{underAttack}
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

/** How many attackers are currently attacking `playerId`, straight from the view
 * (`Permanent.attacking_player`, issue #347). The client counts; it derives no combat. */
function attackersOn(view: GameView, playerId: PlayerId): number {
  return view.battlefield.filter((perm) => perm.attacking_player === playerId).length;
}

/** The screen-reader label a focused opponent tile announces: their public state,
 * and whether they have been eliminated (issue #342/#348). */
function opponentFocusLabel(name: string, opponent: GameView['opponents'][number]): string {
  const parts = [`${name}`, `${opponent.life} life`, `hand ${opponent.hand_size}`];
  if (opponent.eliminated) parts.push('eliminated');
  return parts.join(', ');
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
  // On a multiplayer table there is more than one opponent area to navigate between,
  // so each opponent tile becomes a keyboard/controller focus anchor (issue #348). A
  // two-player duel keeps its single opponent tile as pure quiet display — no focus
  // stop — so the finely-tuned duel focus order is unchanged (the design stance:
  // only offered interactions are focusable; here the extra anchor earns its place
  // only once there are several opponent areas to reach).
  const multiplayer = view.opponents.length > 1;
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
            attackersOn(view, opponent.player_id),
          ),
          targeting,
          highlightedId === opponent.player_id,
          multiplayer ? opponentFocusLabel(name, opponent) : undefined,
          accentStyle(view, opponent.player_id),
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
            <span className={s.lifeCrest}>
              <span className={s.lifeCrestValue} data-testid={`hud-life-${id}`}>
                {view.me.life}
              </span>
            </span>
            <span className={s.hudLifeLabel}>Life</span>
          </div>
          {view.mana_pool.length > 0 && (
            <div className={s.hudMana} data-testid="hud-mana">
              Mana {view.mana_pool.join(' ')}
            </div>
          )}
          {/* The receiver, too, reads when they are the one being attacked (issue #347). */}
          {localId !== undefined && attackersOn(view, localId) > 0 && (
            <div className={s.hudAttacked} data-testid={`hud-attacked-${id}`}>
              Attacked ×{attackersOn(view, localId)}
            </div>
          )}
        </>,
        targeting,
        highlightedId === id,
        undefined,
        localId !== undefined ? accentStyle(view, localId) : undefined,
      )}
    </div>
  );
}
