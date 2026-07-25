import { describe, expect, it, vi } from 'vitest';
import type { GameLogEntry, GameView } from '../../protocol';
import {
  ACTIVITY,
  deriveActivity,
  isMeaningful,
  newestSequence,
  turnChangeSequences,
} from './activityFeed';

/** A view carrying only what the activity surface reads. */
function viewWith(log: GameLogEntry[]): Pick<GameView, 'log' | 'player_names'> {
  return { log, player_names: { p1: 'Imogen', p2: 'Sorel' } };
}

function step(sequence: number): GameLogEntry {
  return {
    sequence,
    event: { type: 'step_changed', turn: 1, phase: 'upkeep', active_player: 'p1' },
  };
}

function cast(sequence: number, name: string): GameLogEntry {
  return {
    sequence,
    event: { type: 'spell_cast', player: 'p2', card: { id: `c${sequence}`, name } },
  };
}

describe('isMeaningful', () => {
  it('excludes the repetitive step advance the log already folds', () => {
    expect(isMeaningful(step(1).event)).toBe(false);
  });

  it('includes everything that actually happened', () => {
    expect(isMeaningful(cast(1, 'Shock').event)).toBe(true);
    expect(isMeaningful({ type: 'permanent_died', permanent: { id: 'p', name: 'Bear' } })).toBe(
      true,
    );
  });
});

/**
 * Turn boundaries (issue #455). ADR 0020's settle loop can run a whole opponent
 * turn between two broadcasts; #455 records the consequence verbatim ("the
 * player believes they're still in turn 1; the game is at turn 2"). The eleven
 * intra-turn advances stay folded — the boundary between two turns does not.
 */
describe('turnChangeSequences (issue #455)', () => {
  function stepAt(sequence: number, turn: number): GameLogEntry {
    return {
      sequence,
      event: { type: 'step_changed', turn, phase: 'untap', active_player: 'p1' },
    };
  }

  it('names the step entry whose turn differs from the one before it', () => {
    const entries = [stepAt(1, 1), stepAt(2, 1), stepAt(3, 2), stepAt(4, 2), stepAt(5, 3)];
    expect([...turnChangeSequences(entries)]).toEqual([3, 5]);
  });

  it('never names the window’s first step entry', () => {
    // After a reconnect the window opens mid-turn; calling that a turn change
    // would announce a boundary that never happened.
    expect([...turnChangeSequences([stepAt(9, 4), stepAt(10, 4)])]).toEqual([]);
  });

  it('ignores every other event type', () => {
    expect([...turnChangeSequences([cast(1, 'Shock'), cast(2, 'Bolt')])]).toEqual([]);
  });
});

describe('deriveActivity — presence', () => {
  it('is absent with no log window at all', () => {
    expect(deriveActivity({ player_names: {} }).present).toBe(false);
    expect(deriveActivity(viewWith([])).present).toBe(false);
  });

  it('is present as soon as the window carries anything', () => {
    const model = deriveActivity(viewWith([cast(1, 'Shock')]));
    expect(model.present).toBe(true);
    expect(model.total).toBe(1);
  });
});

describe('deriveActivity — the auto-surfaced ticker', () => {
  it('surfaces meaningful events on its own, newest first', () => {
    const model = deriveActivity(viewWith([cast(1, 'Shock'), cast(2, 'Counterspell')]));
    expect(model.surfaced.map((line) => line.sequence)).toEqual([2, 1]);
  });

  it('never surfaces a step change on its own', () => {
    const model = deriveActivity(viewWith([cast(1, 'Shock'), step(2), step(3)]));
    expect(model.surfaced.map((line) => line.sequence)).toEqual([1]);
  });

  it('caps the ticker rather than becoming the column ADR 0032 removed', () => {
    const entries = Array.from({ length: 10 }, (_, i) => cast(i + 1, `Spell ${i + 1}`));
    expect(deriveActivity(viewWith(entries)).surfaced).toHaveLength(ACTIVITY.surfaceMax);
  });

  it('retires everything at or below the dwell watermark', () => {
    const entries = [cast(1, 'Shock'), cast(2, 'Counterspell')];
    expect(deriveActivity(viewWith(entries), { sinceSequence: 2 }).surfaced).toHaveLength(0);
    expect(
      deriveActivity(viewWith(entries), { sinceSequence: 1 }).surfaced.map((l) => l.sequence),
    ).toEqual([2]);
  });

  it('re-surfaces when something newer than the watermark arrives', () => {
    const model = deriveActivity(viewWith([cast(1, 'Shock'), cast(2, 'Bolt')]), {
      sinceSequence: 1,
    });
    expect(model.surfaced).toHaveLength(1);
  });

  it('carries logComposition’s words, not its own', () => {
    const model = deriveActivity(viewWith([cast(1, 'Shock')]));
    const text = model.surfaced[0].segments
      .map((segment) => (typeof segment === 'string' ? segment : segment.name))
      .join('');
    expect(text).toBe('Sorel cast Shock.');
  });

  it('drops an event kind the composer does not know rather than drawing a blank line', () => {
    const unknown = {
      sequence: 1,
      // A future server event this client has never seen.
      event: { type: 'something_new' },
    } as unknown as GameLogEntry;
    expect(deriveActivity(viewWith([unknown])).surfaced).toHaveLength(0);
    // …but the surface is still present, so the history door stays available.
    expect(deriveActivity(viewWith([unknown])).present).toBe(true);
  });

  it('surfaces a turn change while keeping the intra-turn advances folded', () => {
    // The whole shape #455 reports: an opponent's turn resolving in one
    // broadcast. The boundary line is the one the player must not miss.
    const entries: GameLogEntry[] = [
      { sequence: 1, event: { type: 'step_changed', turn: 1, phase: 'end', active_player: 'p1' } },
      {
        sequence: 2,
        event: { type: 'step_changed', turn: 2, phase: 'untap', active_player: 'p2' },
      },
      {
        sequence: 3,
        event: { type: 'step_changed', turn: 2, phase: 'upkeep', active_player: 'p2' },
      },
    ];
    const model = deriveActivity(viewWith(entries));
    expect(model.surfaced.map((line) => line.sequence)).toEqual([2]);
    const text = model.surfaced[0].segments
      .map((segment) => (typeof segment === 'string' ? segment : segment.name))
      .join('');
    // The words are still logComposition's — this module decides which entries
    // surface, never what they say.
    expect(text).toBe('Turn 2, Untap — Sorel');
  });

  it('marks unseen lines from the shipped unread marker', () => {
    const isUnseen = vi.fn((sequence: number) => sequence === 2);
    const model = deriveActivity(viewWith([cast(1, 'Shock'), cast(2, 'Bolt')]), { isUnseen });
    expect(model.surfaced.find((l) => l.sequence === 2)?.unseen).toBe(true);
    expect(model.surfaced.find((l) => l.sequence === 1)?.unseen).toBe(false);
  });
});

describe('deriveActivity — the badge', () => {
  it('states the unread count, and clamps rather than growing', () => {
    const entries = Array.from({ length: 30 }, (_, i) => cast(i + 1, `S${i}`));
    expect(deriveActivity(viewWith(entries), { unreadCount: 3 }).badgeText).toBe('3');
    expect(deriveActivity(viewWith(entries), { unreadCount: 30 }).badgeText).toBe(
      `${ACTIVITY.badgeMax}+`,
    );
  });

  it('always carries a sentence as its accessible name, never a bare glyph', () => {
    const caught = deriveActivity(viewWith([cast(1, 'Shock')]));
    expect(caught.badgeLabel).toBe('Activity — 1 event. Open the full history.');
    const unread = deriveActivity(viewWith([cast(1, 'Shock')]), { unreadCount: 1 });
    expect(unread.badgeLabel).toBe('Activity — 1 new event. Open the full history.');
  });
});

describe('newestSequence', () => {
  it('reports the newest carried sequence, and -1 for an empty window', () => {
    expect(newestSequence(viewWith([cast(4, 'A'), cast(9, 'B')]))).toBe(9);
    expect(newestSequence(viewWith([]))).toBe(-1);
  });
});
