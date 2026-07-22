import { describe, expect, it } from 'vitest';
import type { GameLogEntry, GameLogEvent } from '../protocol';
import { describeEvent, groupEntries, isRef, phaseLabel, type LogSegment } from './logComposition';

/** A naming view exposing the display-name map the composer reads. */
const NAMES = { player_names: { p1: 'Alice', p2: 'Bob' } };
/** A naming view with no chosen names, so players fall back to their opaque ids. */
const NO_NAMES = { player_names: {} };

/** Flatten a segment list to its plain rendered text (refs contribute their name). */
function text(segments: LogSegment[]): string {
  return segments.map((seg) => (isRef(seg) ? seg.name : seg)).join('');
}

/** The clickable references in a composed line, in order. */
function refs(segments: LogSegment[]) {
  return segments.filter(isRef);
}

describe('phaseLabel', () => {
  it('title-cases a snake_case phase', () => {
    expect(phaseLabel('precombat_main')).toBe('Precombat Main');
    expect(phaseLabel('upkeep')).toBe('Upkeep');
    expect(phaseLabel('declare_attackers')).toBe('Declare Attackers');
  });
});

describe('describeEvent (client-composed prose)', () => {
  it('composes a spell cast with a clickable caster and card', () => {
    const event: GameLogEvent = {
      type: 'spell_cast',
      player: 'p2',
      card: { id: 's1', name: 'Lightning Bolt' },
    };
    const segments = describeEvent(event, NAMES);
    expect(text(segments)).toBe('Bob cast Lightning Bolt.');
    expect(refs(segments)).toEqual([
      { kind: 'player', id: 'p2', name: 'Bob' },
      { kind: 'entity', id: 's1', name: 'Lightning Bolt' },
    ]);
  });

  it('falls back to the opaque id when a player has no chosen name', () => {
    const event: GameLogEvent = { type: 'mulligan', player: 'p1' };
    expect(text(describeEvent(event, NO_NAMES))).toBe('p1 mulligans.');
  });

  it('renders life gain and loss from the amount sign', () => {
    expect(text(describeEvent({ type: 'life_changed', player: 'p1', amount: 3 }, NAMES))).toBe(
      'Alice gains 3 life.',
    );
    expect(text(describeEvent({ type: 'life_changed', player: 'p1', amount: -2 }, NAMES))).toBe(
      'Alice loses 2 life.',
    );
  });

  it('distinguishes damage to a player from damage to a permanent', () => {
    expect(
      text(
        describeEvent(
          { type: 'damage_dealt', target: { kind: 'player', player: 'p2' }, amount: 3 },
          NAMES,
        ),
      ),
    ).toBe('Bob takes 3 damage.');
    const toPerm = describeEvent(
      {
        type: 'damage_dealt',
        target: { kind: 'permanent', permanent: { id: 'perm_1', name: 'Grizzly Bears' } },
        amount: 2,
      },
      NAMES,
    );
    expect(text(toPerm)).toBe('Grizzly Bears takes 2 damage.');
    expect(refs(toPerm)).toEqual([{ kind: 'entity', id: 'perm_1', name: 'Grizzly Bears' }]);
  });

  it('pluralizes card draws by count', () => {
    expect(text(describeEvent({ type: 'cards_drawn', player: 'p1', count: 1 }, NAMES))).toBe(
      'Alice draws 1 card.',
    );
    expect(text(describeEvent({ type: 'cards_drawn', player: 'p1', count: 3 }, NAMES))).toBe(
      'Alice draws 3 cards.',
    );
  });

  it('lists declared attackers as clickable references', () => {
    const event: GameLogEvent = {
      type: 'attackers_declared',
      player: 'p1',
      attackers: [
        { id: 'a1', name: 'Bear' },
        { id: 'a2', name: 'Elf' },
      ],
    };
    const segments = describeEvent(event, NAMES);
    expect(text(segments)).toBe('Alice attacks with Bear and Elf.');
    expect(refs(segments).map((r) => r.id)).toEqual(['p1', 'a1', 'a2']);
  });

  it('composes each blocker→attacker pairing', () => {
    const event: GameLogEvent = {
      type: 'blockers_declared',
      player: 'p2',
      blocks: [
        { blocker: { id: 'b1', name: 'Wall' }, attacker: { id: 'a1', name: 'Bear' } },
        { blocker: { id: 'b2', name: 'Golem' }, attacker: { id: 'a2', name: 'Elf' } },
      ],
    };
    expect(text(describeEvent(event, NAMES))).toBe(
      'Bob blocks: Wall blocks Bear; Golem blocks Elf.',
    );
  });

  it('names the step change with turn, phase, and active player', () => {
    const event: GameLogEvent = {
      type: 'step_changed',
      turn: 5,
      active_player: 'p1',
      phase: 'draw',
    };
    expect(text(describeEvent(event, NAMES))).toBe('Turn 5, Draw — Alice');
  });

  it('composes a decisive game-over and a draw', () => {
    expect(
      text(
        describeEvent(
          { type: 'game_over', result: { winner: 'p1', losers: ['p2'], reason: 'life_zero' } },
          NAMES,
        ),
      ),
    ).toBe('Game over — Alice wins (life total reached zero).');
    expect(
      text(
        describeEvent(
          { type: 'game_over', result: { losers: ['p1', 'p2'], reason: 'concede' } },
          NAMES,
        ),
      ),
    ).toBe('Game over — draw (conceded).');
  });

  it('degrades an unknown future event to nothing rather than throwing', () => {
    // A forward-compatible frame could carry an event kind this client does not know.
    const unknown = { type: 'teleported', player: 'p1' } as unknown as GameLogEvent;
    expect(describeEvent(unknown, NAMES)).toEqual([]);
  });
});

describe('groupEntries (collapse repetitive step runs)', () => {
  const step = (sequence: number): GameLogEntry => ({
    sequence,
    event: { type: 'step_changed', turn: 1, active_player: 'p1', phase: 'upkeep' },
  });
  const draw = (sequence: number): GameLogEntry => ({
    sequence,
    event: { type: 'cards_drawn', player: 'p1', count: 1 },
  });

  it('folds a run of two or more consecutive step changes into one group', () => {
    const groups = groupEntries([step(1), step(2), step(3), draw(4)]);
    expect(groups).toHaveLength(2);
    expect(groups[0]).toEqual({ kind: 'steps', entries: [step(1), step(2), step(3)] });
    expect(groups[1]).toEqual({ kind: 'entry', entry: draw(4) });
  });

  it('keeps a lone step change as its own entry', () => {
    const groups = groupEntries([draw(1), step(2), draw(3)]);
    expect(groups.map((g) => g.kind)).toEqual(['entry', 'entry', 'entry']);
  });

  it('separates step runs split by a non-step event', () => {
    const groups = groupEntries([step(1), step(2), draw(3), step(4), step(5)]);
    expect(groups.map((g) => g.kind)).toEqual(['steps', 'entry', 'steps']);
  });

  it('is a no-op on an empty window', () => {
    expect(groupEntries([])).toEqual([]);
  });
});
