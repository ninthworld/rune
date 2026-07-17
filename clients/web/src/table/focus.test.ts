import { afterEach, describe, expect, it } from 'vitest';
import type { Rect } from './scene';
import { collectFocusRegions, nextFocus, type FocusRegion } from './focus';

/**
 * The spatial focus engine (issue #301) is exercised as pure logic here: DOM
 * discovery from tagged region containers, plus region-aware navigation over a set
 * of regions with layout geometry. jsdom reports no box geometry, which is exactly
 * why the engine keys off the layout rects passed in — so these assertions are
 * deterministic without a real layout pass.
 */

afterEach(() => {
  document.body.innerHTML = '';
});

/** The shell's rough region rects (top chrome over a central board, rail on the
 * right, decision cluster at the bottom) — a realistic geometry to navigate. */
const GEOMETRY = new Map<string, Rect>([
  ['indicator', { x: 0, y: 0, w: 1280, h: 48 }],
  ['opponentHud', { x: 0, y: 48, w: 1280, h: 76 }],
  ['battlefield', { x: 0, y: 124, w: 1024, h: 600 }],
  ['rail', { x: 1024, y: 124, w: 256, h: 600 }],
  ['tray', { x: 268, y: 640, w: 756, h: 60 }],
]);

/** Build a region container with `n` buttons, tagged `data-focus-region`. */
function region(id: string, n: number): HTMLElement {
  const div = document.createElement('div');
  div.setAttribute('data-focus-region', id);
  for (let i = 0; i < n; i += 1) {
    const button = document.createElement('button');
    button.type = 'button';
    button.dataset.testid = `${id}-${i}`;
    div.appendChild(button);
  }
  document.body.appendChild(div);
  return div;
}

/** A synthetic FocusRegion (bypasses DOM discovery) for the navigation unit tests. */
function fake(id: string, itemCount: number): FocusRegion {
  const items: HTMLElement[] = [];
  for (let i = 0; i < itemCount; i += 1) {
    const el = document.createElement('button');
    el.dataset.testid = `${id}-${i}`;
    items.push(el);
  }
  return { id, rect: GEOMETRY.get(id)!, items };
}

describe('collectFocusRegions', () => {
  it('discovers tagged regions with their focusable items and orders them by geometry', () => {
    // Append out of reading order to prove geometry — not DOM order — sorts them.
    region('tray', 2);
    region('indicator', 0); // no items → dropped
    region('battlefield', 3);
    region('opponentHud', 1);

    const regions = collectFocusRegions(document, GEOMETRY);
    expect(regions.map((r) => r.id)).toEqual(['opponentHud', 'battlefield', 'tray']);
    // The empty indicator is dropped so navigation never lands on nothing.
    expect(regions.find((r) => r.id === 'indicator')).toBeUndefined();
    expect(regions[1].items).toHaveLength(3);
  });

  it('excludes disabled controls (a binding stays inert with no action)', () => {
    const div = region('tray', 0);
    const enabled = document.createElement('button');
    const disabled = document.createElement('button');
    disabled.disabled = true;
    div.append(enabled, disabled);

    const regions = collectFocusRegions(document, GEOMETRY);
    expect(regions).toHaveLength(1);
    expect(regions[0].items).toEqual([enabled]);
  });
});

describe('nextFocus — within a region (along-axis)', () => {
  const hud = fake('opponentHud', 3); // a wide row: Left/Right steps its items
  const regions = [hud];

  it('steps to the next item on Right and the previous on Left', () => {
    expect(nextFocus(regions, hud.items[0], 'right')).toBe(hud.items[1]);
    expect(nextFocus(regions, hud.items[2], 'left')).toBe(hud.items[1]);
  });

  it('enters the first region on the first Right press when nothing is focused', () => {
    expect(nextFocus(regions, null, 'right')).toBe(hud.items[0]);
    expect(nextFocus(regions, null, 'left')).toBe(hud.items[2]);
  });
});

describe('nextFocus — a vertical region (rail) walks on Up/Down', () => {
  const rail = fake('rail', 3); // a tall column: Up/Down steps its items
  const regions = [rail];

  it('steps items with Down/Up (its main axis), not Left/Right', () => {
    expect(nextFocus(regions, rail.items[0], 'down')).toBe(rail.items[1]);
    expect(nextFocus(regions, rail.items[1], 'up')).toBe(rail.items[0]);
  });
});

describe('nextFocus — between regions', () => {
  const hud = fake('opponentHud', 2);
  const battlefield = fake('battlefield', 2);
  const rail = fake('rail', 2);
  const tray = fake('tray', 2);
  const regions = [hud, battlefield, rail, tray];

  it('jumps cross-axis to the spatially nearest region (Down from HUD → battlefield)', () => {
    expect(nextFocus(regions, hud.items[0], 'down')).toBe(battlefield.items[0]);
  });

  it('jumps from the board to the rail on Right and back on Left', () => {
    // Board is a row, so Left/Right is its along-axis; stepping off the right end
    // continues in reading order — battlefield → rail (the next region).
    expect(nextFocus(regions, battlefield.items[1], 'right')).toBe(rail.items[0]);
    // The rail is a column, so Left is a cross-axis jump back to the board; arriving
    // from the right lands on the board's near (last) item.
    expect(nextFocus(regions, rail.items[0], 'left')).toBe(battlefield.items[1]);
  });

  it('reaches every item through region navigation (the core acceptance property)', () => {
    // Explore the focus graph from the first item across all four directions: every
    // focusable item in every region must be reachable by keyboard, with focus never
    // trapped. A region's own axis walks its items; either axis moves between regions.
    const all = regions.flatMap((r) => r.items);
    const start = all[0];
    const seen = new Set<HTMLElement>([start]);
    const queue: HTMLElement[] = [start];
    while (queue.length > 0) {
      const active = queue.shift()!;
      for (const dir of ['up', 'down', 'left', 'right'] as const) {
        const next = nextFocus(regions, active, dir);
        if (next && !seen.has(next)) {
          seen.add(next);
          queue.push(next);
        }
      }
    }
    expect(seen.size).toBe(all.length);
  });
});

describe('nextFocus — degenerate cases', () => {
  it('returns null with no regions', () => {
    expect(nextFocus([], null, 'right')).toBeNull();
  });
});
