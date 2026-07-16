/**
 * Player tiles (React DOM, ADR 0003).
 *
 * Opponent tiles show exactly what the redacted {@link OpponentView} exposes
 * (life, hidden-zone counts, status labels). The local player's tile shows what
 * the GameView carries for the receiver — own life and library size (from
 * {@link SelfView} `me`), hand size, graveyard size, and floating mana. Every
 * value is displayed exactly as the server provides it; the client invents none.
 *
 * In **targeting mode** a player can be a legal target (ADR 0009 §Client /
 * `docs/brief.md` "Targeting Mode": click the player portrait). Each tile whose
 * `player_id` is a server-listed candidate becomes pickable; every other tile is
 * dimmed and inert. The client only makes the server's candidates pickable — it
 * derives no legality.
 */
import type { EntityId, GameView, PlayerId } from '../protocol';
import type { CSSProperties, ReactNode } from 'react';
import {
  dimmedTile,
  localTile,
  targetTile,
  tile,
  tileButtonReset,
  tileName,
  tiles,
} from './styles';

/** The active target slot's player candidates plus the pick handler. */
interface TargetingTiles {
  /** Entity ids that are legal targets for the active slot (players included). */
  candidates: EntityId[];
  /** Pick a player as the current slot's answer. */
  onPick: (id: EntityId) => void;
}

interface Props {
  view: GameView;
  localId?: PlayerId;
  /** Present only in targeting mode; makes candidate players pickable. */
  targeting?: TargetingTiles;
}

export function PlayerTiles({ view, localId, targeting }: Props) {
  const localGraveyard = view.graveyards.find((pile) => pile.player_id === localId);
  const candidateSet = targeting ? new Set(targeting.candidates) : null;

  /**
   * Wrap a tile's content. Outside targeting mode it is a plain `<div>`. Inside
   * it, a candidate player becomes a `<button>` (pickable, ringed); a non-candidate
   * is dimmed and inert.
   */
  const renderTile = (playerId: PlayerId, style: CSSProperties, content: ReactNode): ReactNode => {
    const testId = `tile-${playerId}`;
    if (candidateSet === null) {
      return (
        <div key={playerId} data-testid={testId} style={style}>
          {content}
        </div>
      );
    }
    if (candidateSet.has(playerId) && targeting) {
      return (
        <button
          key={playerId}
          type="button"
          data-testid={`target-player-${playerId}`}
          aria-label={`Target player ${playerId}`}
          onClick={() => targeting.onPick(playerId)}
          style={{ ...tileButtonReset, ...style, ...targetTile }}
        >
          {content}
        </button>
      );
    }
    return (
      <div key={playerId} data-testid={testId} style={{ ...style, ...dimmedTile }}>
        {content}
      </div>
    );
  };

  return (
    <div data-testid="player-tiles" style={tiles}>
      {view.opponents.map((opponent) =>
        renderTile(
          opponent.player_id,
          tile,
          <>
            <div style={tileName}>{opponent.player_id}</div>
            <div>Life {opponent.life}</div>
            <div>Hand {opponent.hand_size}</div>
            <div>Library {opponent.library_size}</div>
            <div>Graveyard {opponent.graveyard_size}</div>
            {opponent.statuses && opponent.statuses.length > 0 && (
              <div>{opponent.statuses.join(', ')}</div>
            )}
          </>,
        ),
      )}

      {renderTile(
        localId ?? 'local',
        { ...tile, ...localTile },
        <>
          <div style={tileName}>{localId ?? 'You'} (you)</div>
          <div>Life {view.me.life}</div>
          <div>Hand {view.my_hand.length}</div>
          <div>Library {view.me.library_size}</div>
          <div>Graveyard {localGraveyard?.cards.length ?? 0}</div>
          {view.mana_pool.length > 0 && <div>Mana {view.mana_pool.join(' ')}</div>}
        </>,
      )}
    </div>
  );
}
