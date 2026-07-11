/**
 * Player tiles (React DOM, ADR 0003).
 *
 * Opponent tiles show exactly what the redacted {@link OpponentView} exposes
 * (life, hidden-zone counts, status labels). The local player's tile shows only
 * what the GameView carries for the receiver — hand size, graveyard size, and
 * floating mana. The server never sends the receiver's own life total, so the
 * client does not invent one (values are displayed exactly as provided).
 */
import type { GameView, PlayerId } from '../protocol';
import { localTile, tile, tileName, tiles } from './styles';

interface Props {
  view: GameView;
  localId?: PlayerId;
}

export function PlayerTiles({ view, localId }: Props) {
  const localGraveyard = view.graveyards.find((pile) => pile.player_id === localId);

  return (
    <div data-testid="player-tiles" style={tiles}>
      {view.opponents.map((opponent) => (
        <div key={opponent.player_id} data-testid={`tile-${opponent.player_id}`} style={tile}>
          <div style={tileName}>{opponent.player_id}</div>
          <div>Life {opponent.life}</div>
          <div>Hand {opponent.hand_size}</div>
          <div>Library {opponent.library_size}</div>
          <div>Graveyard {opponent.graveyard_size}</div>
          {opponent.statuses && opponent.statuses.length > 0 && (
            <div>{opponent.statuses.join(', ')}</div>
          )}
        </div>
      ))}

      <div data-testid={`tile-${localId ?? 'local'}`} style={{ ...tile, ...localTile }}>
        <div style={tileName}>{localId ?? 'You'} (you)</div>
        <div>Hand {view.my_hand.length}</div>
        <div>Graveyard {localGraveyard?.cards.length ?? 0}</div>
        {view.mana_pool.length > 0 && <div>Mana {view.mana_pool.join(' ')}</div>}
      </div>
    </div>
  );
}
