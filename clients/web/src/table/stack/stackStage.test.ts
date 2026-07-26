import { describe, expect, it } from 'vitest';
import type { GameView, Permanent, StackItem } from '../../protocol';
import { parseGameView } from '../../wire';
import {
  CLUSTER_CLEARANCE,
  STACK_STAGE,
  deriveStackStage,
  railViewportHeight,
  stackStyleVars,
  tierHeight,
} from './stackStage';

/** A minimal live view carrying just the fields the stage derivation reads. */
function viewWith(stack: StackItem[], battlefield: Permanent[] = []): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [{ player_id: 'p2', life: 20, library_size: 40, hand_size: 7, graveyard_size: 0 }],
    battlefield,
    stack,
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 1,
    active_player: 'p1',
    seat_order: ['p1', 'p2'],
    mana_pool: [],
    valid_actions: [],
    player_names: { p1: 'Imogen', p2: 'Sorel' },
    commander_damage: [],
  };
}

/** `n` spells, bottom-first on the wire (`s1` at the bottom). */
function spells(n: number, controller = 'p2'): StackItem[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `s${i + 1}`,
    controller,
    description: `Spell ${i + 1}`,
  }));
}

function permanent(id: string, name: string): Permanent {
  return { id, controller: 'p1', owner: 'p1', card: { id, name, type_line: 'Creature' } };
}

describe('deriveStackStage — presence (§1.2)', () => {
  it('is absent for an empty stack, reserving nothing', () => {
    const model = deriveStackStage(viewWith([]));
    expect(model.present).toBe(false);
    expect(model.entries).toHaveLength(0);
    expect(model.count).toBe(0);
  });

  it('surfaces automatically the moment the stack is non-empty', () => {
    expect(deriveStackStage(viewWith(spells(1))).present).toBe(true);
  });
});

describe('deriveStackStage — order (§3.3)', () => {
  it('reverses the bottom-first wire order so index 1 is the top of the stack', () => {
    const model = deriveStackStage(viewWith(spells(3)));
    expect(model.entries.map((e) => e.id)).toEqual(['s3', 's2', 's1']);
    expect(model.entries.map((e) => e.index)).toEqual([1, 2, 3]);
    expect(model.entries[0].isTop).toBe(true);
    expect(model.entries[1].isTop).toBe(false);
    expect(model.entries.every((e) => e.total === 3)).toBe(true);
  });

  it('states N in the header verbatim, so tiers printing only n stay readable', () => {
    expect(deriveStackStage(viewWith(spells(5))).header).toBe('STACK (5) — TOP RESOLVES FIRST');
    expect(deriveStackStage(viewWith(spells(5))).ariaLabel).toBe(
      'Stack, 5 objects, top resolves first',
    );
  });

  it('paints the top of the stack at the front of the pile', () => {
    const model = deriveStackStage(viewWith(spells(3)));
    expect(model.entries[0].zOrder).toBeGreaterThan(model.entries[1].zOrder);
    expect(model.entries[1].zOrder).toBeGreaterThan(model.entries[2].zOrder);
  });
});

describe('deriveStackStage — the depth ladder (§3.1, D4)', () => {
  it('is a pile of one Expanded entry at depth 1', () => {
    const model = deriveStackStage(viewWith(spells(1)));
    expect(model.layout).toBe('pile');
    expect(model.entries[0].tier).toBe('expanded');
  });

  it('recedes Mini entries behind the Expanded top at depth 2–5', () => {
    const model = deriveStackStage(viewWith(spells(5)));
    expect(model.layout).toBe('pile');
    expect(model.entries.map((e) => e.tier)).toEqual(['expanded', 'mini', 'mini', 'mini', 'mini']);
    // One constant offset per step, with the recession scale compounding.
    expect(model.entries[1].offsetX).toBe(STACK_STAGE.splayDx);
    expect(model.entries[2].offsetX).toBe(2 * STACK_STAGE.splayDx);
    expect(model.entries[1].scale).toBeCloseTo(STACK_STAGE.splayScale);
    expect(model.entries[2].scale).toBeCloseTo(STACK_STAGE.splayScale ** 2);
    expect(model.entries[0].offsetX).toBe(0);
    expect(model.entries[0].scale).toBe(1);
  });

  it('floors the recession scale so a deep pile never vanishes', () => {
    const model = deriveStackStage(viewWith(spells(5)));
    expect(Math.min(...model.entries.map((e) => e.scale))).toBeGreaterThanOrEqual(
      STACK_STAGE.splayScaleFloor,
    );
  });

  it('changes kind to a row rail at the depth-6 collapse point', () => {
    const model = deriveStackStage(viewWith(spells(6)));
    expect(model.layout).toBe('rail');
    expect(model.entries.map((e) => e.tier)).toEqual([
      'expanded',
      'row',
      'row',
      'row',
      'row',
      'row',
    ]);
    // Recession STOPS at the collapse point — it is a change of kind, not a shrink.
    expect(model.entries.every((e) => e.offsetX === 0 && e.scale === 1)).toBe(true);
  });

  it('does not scroll at depth 8 — the whole rail fits', () => {
    const model = deriveStackStage(viewWith(spells(8)));
    expect(model.scrolls).toBe(false);
    expect(model.hiddenCount).toBe(0);
  });

  it('scrolls beyond depth 8 and states the hidden count', () => {
    const model = deriveStackStage(viewWith(spells(12)));
    expect(model.scrolls).toBe(true);
    expect(model.hiddenCount).toBe(4);
    expect(model.entries).toHaveLength(12);
  });

  it('pins the top three entries out of the scroll (D5)', () => {
    const model = deriveStackStage(viewWith(spells(12)));
    expect(model.entries.slice(0, STACK_STAGE.stickyHead).every((e) => e.sticky === 'head')).toBe(
      true,
    );
    expect(model.entries[STACK_STAGE.stickyHead].sticky).toBeNull();
  });

  it('pins the bottom-most entry too past depth 10, so both ends stay visible', () => {
    expect(deriveStackStage(viewWith(spells(12))).entries[11].sticky).toBe('foot');
    // At depth 9 the foot is not pinned; only the head is.
    expect(deriveStackStage(viewWith(spells(9))).entries[8].sticky).toBeNull();
  });
});

describe('deriveStackStage — compact geometry (§10.4)', () => {
  it('changes kind to a sheet of Row entries, readable at depth 8', () => {
    const model = deriveStackStage(viewWith(spells(8)), { compact: true });
    expect(model.layout).toBe('sheet');
    expect(model.entries.slice(1).every((e) => e.tier === 'row')).toBe(true);
    // Every row clears the 44 px interactive floor by construction.
    expect(tierHeight('row')).toBeGreaterThanOrEqual(STACK_STAGE.hit);
  });

  it('stays a sheet at depth 1 rather than falling back to the flank pile', () => {
    expect(deriveStackStage(viewWith(spells(1)), { compact: true }).layout).toBe('sheet');
  });
});

describe('deriveStackStage — focus promotes exactly one entry (§2.1, §9.3)', () => {
  it('promotes the focused entry and demotes the top, keeping exactly one Expanded', () => {
    const model = deriveStackStage(viewWith(spells(4)), { focusId: 's2' });
    const expanded = model.entries.filter((e) => e.tier === 'expanded');
    expect(expanded).toHaveLength(1);
    expect(expanded[0].id).toBe('s2');
    expect(model.entries[0].tier).toBe('mini');
  });

  it('ignores a focus id that is no longer on the stack', () => {
    const model = deriveStackStage(viewWith(spells(3)), { focusId: 'gone' });
    expect(model.entries[0].tier).toBe('expanded');
    expect(model.entries.filter((e) => e.tier === 'expanded')).toHaveLength(1);
  });
});

describe('deriveStackStage — the four channels that never degrade (§2.4 rule 2)', () => {
  it('carries controller stripe, order index, kind marker on every entry at every tier', () => {
    for (const depth of [1, 5, 8, 12]) {
      for (const entry of deriveStackStage(viewWith(spells(depth))).entries) {
        expect(entry.accent).toMatch(/^#/);
        expect(entry.index).toBeGreaterThan(0);
        expect(entry.glyph.length).toBeGreaterThan(0);
        expect(entry.subtitle.length).toBeGreaterThan(0);
      }
    }
  });

  it('gives spells and abilities different kind markers', () => {
    const view = viewWith(
      [
        { id: 'a1', controller: 'p1', description: 'Tap: add {G}', source: 'perm1' },
        { id: 's1', controller: 'p2', description: 'Counterspell' },
      ],
      [permanent('perm1', 'Ridge Wolf')],
    );
    const model = deriveStackStage(view);
    const spell = model.entries.find((e) => e.id === 's1');
    const ability = model.entries.find((e) => e.id === 'a1');
    expect(spell?.kind).toBe('spell');
    expect(spell?.glyph).toBe('C');
    expect(ability?.kind).toBe('ability');
    expect(ability?.glyph).not.toBe(spell?.glyph);
  });

  it('takes the kind the server states, falling back to `source` only without one', () => {
    // Issue #550: the discriminator is server-stated. The `source`-presence reading
    // survives only for an entry from a server that predates the field — it is a
    // fallback, never a second opinion the client weighs against the server's.
    const model = deriveStackStage(
      viewWith([
        { id: 'stated', controller: 'p1', description: 'Trigger', kind: 'ability' },
        { id: 'legacy', controller: 'p1', description: 'Tap: add {G}', source: 'perm1' },
        { id: 'bare', controller: 'p2', description: 'Counterspell' },
      ]),
    );
    // An ability with no source permanent (the server said so) is still an ability.
    expect(model.entries.find((e) => e.id === 'stated')?.kind).toBe('ability');
    expect(model.entries.find((e) => e.id === 'legacy')?.kind).toBe('ability');
    expect(model.entries.find((e) => e.id === 'bare')?.kind).toBe('spell');
  });

  it('carries the activated/triggered provenance the server states (issue #579)', () => {
    // Gap G2 closed: the engine records how an ability got onto the stack, so the
    // stage carries it as `origin` — the data source §2.3's trigger caret reads —
    // and speaks it in the accessible name (§9.2's "Triggered ability from …").
    // Both finer kinds still draw the *ability* plate: `kind` is the plate category.
    const model = deriveStackStage(
      viewWith(
        [
          {
            id: 'act',
            controller: 'p1',
            description: 'Tap target creature.',
            source: 'perm1',
            kind: 'activated',
          },
          {
            id: 'trg',
            controller: 'p1',
            description: 'Tap target creature.',
            source: 'perm1',
            kind: 'triggered',
          },
          {
            id: 'coarse',
            controller: 'p1',
            description: 'Tap target creature.',
            source: 'perm1',
            kind: 'ability',
          },
        ],
        [permanent('perm1', 'Dawn Herald')],
      ),
    );
    const entry = (id: string) => model.entries.find((e) => e.id === id);
    expect(entry('act')?.origin).toBe('activated');
    expect(entry('trg')?.origin).toBe('triggered');
    expect(entry('act')?.kind).toBe('ability');
    expect(entry('trg')?.kind).toBe('ability');
    expect(entry('trg')?.label).toContain('Triggered ability from Dawn Herald');
    expect(entry('act')?.label).toContain('Activated ability from Dawn Herald');

    // A server that states only the coarse `ability` gets the generic reading: the
    // client leaves it unclassified rather than picking one.
    expect(entry('coarse')?.origin).toBeUndefined();
    expect(entry('coarse')?.label).toContain('Ability from Dawn Herald');
    expect(entry('coarse')?.label).not.toContain('Triggered');
  });

  it('leaves an unrecognized kind unclassified end to end, never falling back to `source`', () => {
    // The forward-compatibility path, driven through the *real* normalizer rather
    // than a hand-built model: a server states `copy` (gap G3), which this build does
    // not know. `normalizeStackItem` refuses to carry it as a `kind` — nothing may
    // mistake it for a known one — but records that a kind *was* stated, so the
    // legacy `source`-presence fallback stays switched off. Without that, an entry
    // the server explicitly classified as something else would be silently redrawn
    // as an ability, which is the guess the contract forbids.
    const wire = JSON.stringify({
      you: 'p1',
      phase: 'precombat_main',
      player_names: { p1: 'Imogen' },
      battlefield: [
        { id: 'perm1', controller: 'p1', owner: 'p1', card: { id: 'perm1', name: 'Dawn Herald' } },
      ],
      stack: [
        { id: 'future', controller: 'p1', description: 'Copy of Shock', kind: 'copy' },
        {
          id: 'sourced',
          controller: 'p1',
          description: 'Copy of a trigger',
          kind: 'copy',
          source: 'perm1',
        },
        { id: 'legacy', controller: 'p1', description: 'Tap: add {G}', source: 'perm1' },
      ],
    });
    const view = parseGameView(wire);
    expect(view.stack[0]!.kind).toBeUndefined();
    expect(view.stack[0]!.kindUnknown).toBe(true);
    // The pre-#550 entry states no kind at all, so it carries no marker and keeps
    // the fallback it is entitled to.
    expect(view.stack[2]!.kindUnknown).toBeUndefined();

    const entry = (id: string) => deriveStackStage(view).entries.find((e) => e.id === id);
    expect(entry('future')?.kind).toBe('unclassified');
    expect(entry('future')?.origin).toBeUndefined();
    // The one that would otherwise have been coerced: it names a source, and under
    // the legacy fallback alone that presence would have made it an ability.
    expect(entry('sourced')?.kind).toBe('unclassified');
    expect(entry('legacy')?.kind).toBe('ability');

    // The entry is still fully rendered — unclassified is a state, not a hole. It
    // reads from `description` and says plainly that it is unrecognized, and the
    // source tether (a server fact, independent of kind) still survives.
    expect(entry('future')?.description).toBe('Copy of Shock');
    expect(entry('future')?.glyph).toBe('?');
    expect(entry('future')?.subtitle).toBe('unrecognized kind · You');
    expect(entry('future')?.label).toContain('Object of an unrecognized kind, controlled by you.');
    expect(entry('sourced')?.label).toContain('unrecognized kind from Dawn Herald');
  });

  it('names an ability source from the battlefield, and says so when it is gone (C5)', () => {
    const resolved = deriveStackStage(
      viewWith(
        [{ id: 'a1', controller: 'p1', description: 'Trigger', source: 'perm1' }],
        [permanent('perm1', 'Dawn Herald')],
      ),
    ).entries[0];
    expect(resolved.sourceResolved).toBe(true);
    expect(resolved.sourceName).toBe('Dawn Herald');
    expect(resolved.subtitle).toBe('ability — Dawn Herald · You');

    const gone = deriveStackStage(
      viewWith([{ id: 'a1', controller: 'p1', description: 'Trigger', source: 'perm1' }]),
    ).entries[0];
    expect(gone.sourceResolved).toBe(false);
    // Never the raw opaque entity id — a player reads that as a card name.
    expect(gone.subtitle).not.toContain('perm1');
    expect(gone.label).not.toContain('perm1');
  });

  it('gives different controllers different seat accents', () => {
    const view = viewWith([
      { id: 's1', controller: 'p1', description: 'Mine' },
      { id: 's2', controller: 'p2', description: 'Theirs' },
    ]);
    const model = deriveStackStage(view);
    expect(model.entries[0].accent).not.toBe(model.entries[1].accent);
  });
});

describe('deriveStackStage — text is the server’s (§2.4 rule 3, §9.2)', () => {
  it('prints the description verbatim and never composes prose', () => {
    const view = viewWith([{ id: 's1', controller: 'p2', description: 'Deal 3 damage to X' }]);
    expect(deriveStackStage(view).entries[0].description).toBe('Deal 3 damage to X');
  });

  it('assembles the accessible name in §9.2’s fixed order', () => {
    const view = viewWith(
      [
        { id: 's1', controller: 'p2', description: 'Grizzly Bears' },
        { id: 'a1', controller: 'p1', description: 'Tap: add {G}', source: 'perm1' },
      ],
      [permanent('perm1', 'Ridge Wolf')],
    );
    const model = deriveStackStage(view);
    expect(model.entries[0].label).toBe(
      // The label is a pure-text context, so the description's symbol notation
      // is spoken rather than braced (issue #462).
      '1 of 2. Resolves next. Ability from Ridge Wolf, controlled by you. Tap: add green mana',
    );
    expect(model.entries[1].label).toBe('2 of 2. Spell, controlled by Sorel. Grizzly Bears');
  });
});

describe('stage placement constants', () => {
  it('shares the control cluster’s column and margin (control-language §4.4/D7)', () => {
    const vars = stackStyleVars() as Record<string, string>;
    expect(vars['--stack-width']).toBe(`${STACK_STAGE.width}px`);
    expect(vars['--stack-margin']).toBe(`${STACK_STAGE.margin}px`);
    // §1.2's own clamp band, which the cluster width has to sit inside.
    expect(STACK_STAGE.width).toBeGreaterThanOrEqual(232);
    expect(STACK_STAGE.width).toBeLessThanOrEqual(300);
  });

  it('leaves the flank’s foot clear for the control cluster', () => {
    expect(CLUSTER_CLEARANCE).toBeGreaterThan(STACK_STAGE.expandedH / 2);
    expect((stackStyleVars() as Record<string, string>)['--stack-clearance']).toBe(
      `${CLUSTER_CLEARANCE}px`,
    );
  });

  it('caps the scrolling rail at exactly the depth-8 window', () => {
    expect(railViewportHeight()).toBe(
      STACK_STAGE.visibleRows * (STACK_STAGE.rowH + STACK_STAGE.rowGap),
    );
  });

  it('publishes the sheet ceiling as §10.4’s fraction of the viewport', () => {
    expect((stackStyleVars() as Record<string, string>)['--stack-sheet-max-h']).toBe('42%');
  });
});
