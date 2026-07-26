import { describe, expect, it } from 'vitest';
import { normalizeGameView, normalizeSpectatorView } from '../wire';
import { isCommanderMatch, seatPresentation, spectatorSeatPresentation } from './seatPresentation';
// The canonical Commander fixture (issue #553), owned by the `rune-protocol` crate
// and round-tripped by its Rust test. Its command zones are all empty on purpose.
import CONTRACT_FIXTURE_COMMANDER from '@protocol-fixtures/gameview-commander.json';

describe('seat presentation (issue #553)', () => {
  const view = normalizeGameView(CONTRACT_FIXTURE_COMMANDER);

  it('reports a Commander match from the format signal, not from zone contents', () => {
    // Every commander has left the command zone, so the frame carries no `command`
    // pile at all — the exact state in which a zone-shaped guess is wrong.
    expect(view.command).toEqual([]);
    expect(isCommanderMatch(view)).toBe(true);
    expect(view.format?.id).toBe('commander');

    // A frame with no format reads as "not Commander", the pre-#553 behaviour.
    expect(isCommanderMatch(normalizeGameView({ phase: 'upkeep' }))).toBe(false);
  });

  it('renders a disconnected seat and an AI seat from authoritative state', () => {
    expect(seatPresentation(view, 'p1').connected).toBe(false);
    expect(seatPresentation(view, 'p1').ai).toBe(false);
    expect(seatPresentation(view, 'p2').connected).toBe(true);
    expect(seatPresentation(view, 'p2').ai).toBe(true);
  });

  it('treats an absent connection flag as connected, never as falsy-disconnected', () => {
    // The inversion this module exists to get right: `connected` rides the wire only
    // as `false`, so an older server's silence must not read as "disconnected".
    const legacy = normalizeGameView({
      phase: 'upkeep',
      you: 'p0',
      me: { life: 20, library_size: 53 },
      opponents: [{ player_id: 'p1', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 }],
    });
    expect(legacy.opponents[0].connected).toBeUndefined();
    expect(seatPresentation(legacy, 'p1').connected).toBe(true);
    expect(seatPresentation(legacy, 'p0').connected).toBe(true);
    expect(seatPresentation(legacy, 'p0').eliminated).toBe(false);
    expect(seatPresentation(legacy, 'p0').ai).toBe(false);
    // A seat the view never mentions falls back to the documented defaults.
    expect(seatPresentation(legacy, 'p9')).toEqual({
      connected: true,
      eliminated: false,
      ai: false,
      commanderName: null,
      colorIdentity: [],
    });
  });

  it('reports the local seat eliminated while the game is still live', () => {
    // `result` is absent (two seats remain), so `me.eliminated` is the only source.
    expect(view.result).toBeUndefined();
    expect(seatPresentation(view, 'p0').eliminated).toBe(true);
    expect(seatPresentation(view, 'p1').eliminated).toBe(false);
  });

  it('keeps a commander name and colour identity that outlive the command zone', () => {
    expect(seatPresentation(view, 'p1')).toMatchObject({
      commanderName: 'Jedit Ojanen',
      colorIdentity: ['G'],
    });
    expect(seatPresentation(view, 'p2')).toMatchObject({
      commanderName: 'Thraximundar',
      colorIdentity: ['U', 'B', 'R'],
    });
    // A colourless commander has a name but no gems — an empty identity is a value.
    expect(seatPresentation(view, 'p0')).toMatchObject({
      commanderName: 'Karn, Silver Golem',
      colorIdentity: [],
    });
  });

  it('survives the commander changing zones', () => {
    // Emptying the command zone changes nothing: the identity is keyed to the
    // designation, so the nameplate and gems do not flicker with the card's location.
    const withZone = normalizeGameView({
      ...CONTRACT_FIXTURE_COMMANDER,
      command: [{ player_id: 'p1', cards: [] }],
    });
    expect(seatPresentation(withZone, 'p1')).toEqual(seatPresentation(view, 'p1'));
  });

  it('gives a spectator the same public seat state with no hidden information', () => {
    const spectator = normalizeSpectatorView({
      phase: 'precombat_main',
      players: [
        { player_id: 'p0', hand_size: 4, life: 0, library_size: 31, graveyard_size: 9 },
        {
          player_id: 'p1',
          hand_size: 5,
          life: 34,
          library_size: 71,
          graveyard_size: 3,
          connected: false,
        },
      ],
      format: { id: 'commander', commander: true },
      commander_identity: [{ commander: 'p1', name: 'Jedit Ojanen', color_identity: ['G'] }],
    });
    expect(isCommanderMatch(spectator)).toBe(true);
    expect(spectatorSeatPresentation(spectator, 'p1')).toEqual({
      connected: false,
      eliminated: false,
      ai: false,
      commanderName: 'Jedit Ojanen',
      colorIdentity: ['G'],
    });
    expect(spectatorSeatPresentation(spectator, 'p0').connected).toBe(true);
    // Structural redaction is untouched: no receiver field exists to read.
    expect('my_hand' in spectator).toBe(false);
    expect('valid_actions' in spectator).toBe(false);
  });
});
