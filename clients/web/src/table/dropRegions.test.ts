import { describe, expect, it } from 'vitest';
import { parseGameView } from '../wire';
import { dropRegionsFor, dropRegionsForEntity, regionMatches } from './dropRegions';
// The canonical action-contract fixture (issue #554), owned by the `rune-protocol`
// crate and round-tripped by its Rust test.
import CONTRACT_FIXTURE_ACTIONS from '@protocol-fixtures/gameview-actions.json';

describe('drop regions (issue #554)', () => {
  const view = parseGameView(JSON.stringify(CONTRACT_FIXTURE_ACTIONS));
  const action = (id: string) => {
    const found = view.valid_actions.find((a) => a.id === id);
    if (found === undefined) throw new Error(`no action ${id}`);
    return found;
  };

  it('produces a drop region solely from the server-named destination', () => {
    expect(dropRegionsFor(action('a1'))).toEqual([
      {
        actionId: 'a1',
        token: 't0000000000000a1',
        kind: 'zone',
        target: 'battlefield',
        label: 'Battlefield',
      },
    ]);
    expect(dropRegionsFor(action('a2'))[0]).toMatchObject({ kind: 'zone', target: 'stack' });
  });

  it('produces no drop target when the action names no destination', () => {
    // Fail closed, in all three flavours: a global action, a mana ability that never
    // uses the stack, and an older server that predates the field entirely.
    expect(dropRegionsFor(action('a0'))).toEqual([]);
    expect(dropRegionsFor(action('a3'))).toEqual([]);
    expect(dropRegionsFor({ id: 'x', type: 'cast_spell', label: 'Cast' })).toEqual([]);
  });

  it('ignores a destination kind it cannot render rather than guessing', () => {
    // A newer server may name a surface this client cannot draw; the region is simply
    // absent, never approximated. A destination with no id is likewise dropped.
    const regions = dropRegionsFor({
      id: 'a9',
      type: 'cast_spell',
      label: 'Cast',
      destinations: [
        { type: 'quadrant', id: 'north' },
        { type: 'zone', id: '' },
        { type: 'player', id: 'p1' },
      ],
    });
    expect(regions).toEqual([{ actionId: 'a9', token: '', kind: 'player', target: 'p1' }]);
  });

  it('collects the regions for one dragged entity from its own actions', () => {
    // Subjects are the server's statement of what an action belongs to (ADR 0004).
    expect(dropRegionsForEntity(view.valid_actions, 'c_forest').map((r) => r.target)).toEqual([
      'battlefield',
    ]);
    expect(dropRegionsForEntity(view.valid_actions, 'c_fireball').map((r) => r.target)).toEqual([
      'stack',
    ]);
    // The permanent's only action is a mana ability, which names no destination.
    expect(dropRegionsForEntity(view.valid_actions, 'perm_elves')).toEqual([]);
    // An entity no action names lights nothing up.
    expect(dropRegionsForEntity(view.valid_actions, 'perm_nothing')).toEqual([]);
  });

  it('scopes an owner-qualified region to that seat only', () => {
    const [region] = dropRegionsFor({
      id: 'a4',
      type: 'return_commander_to_command_zone',
      label: 'Move to the command zone',
      destinations: [{ type: 'zone', id: 'command', owner: 'p1', label: 'Command zone' }],
    });
    expect(region).toBeDefined();
    expect(regionMatches(region!, 'zone', 'command', 'p1')).toBe(true);
    expect(regionMatches(region!, 'zone', 'command', 'p0')).toBe(false);
    expect(regionMatches(region!, 'zone', 'stack', 'p1')).toBe(false);
    // A shared zone has no owner and matches regardless of who is asked about.
    const shared = dropRegionsFor(action('a2'))[0]!;
    expect(regionMatches(shared, 'zone', 'stack', 'p0')).toBe(true);
    expect(regionMatches(shared, 'zone', 'stack', undefined)).toBe(true);
  });
});
