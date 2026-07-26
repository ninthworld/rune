import { describe, expect, it } from 'vitest';
import type { GameView, Permanent, StackItem } from '../../protocol';
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
