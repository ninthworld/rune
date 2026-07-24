/**
 * Off-focus activity staging on the live plane (issue #501, layout-model
 * §Focus model: "off-focus activity is never silent"). A six-seat table stages
 * its wings at the digest rung — exactly the seats that draw no cards of their
 * own, and must still ping, still wear the attacked ring, and still anchor
 * combat paths. The compact case repeats it against summary tiles. The effects
 * layer is mocked to a call recorder so its traffic is assertable without a
 * WebGL context.
 */
import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GameView } from '../../protocol';
import type { Rect } from '../scene';
import { seatTable } from '../plane.fixture';
import { LivePlane } from './LivePlane';

const effects = vi.hoisted(() => ({
  transients: [] as unknown[][],
  persistent: [] as unknown[][],
  rects: undefined as undefined | ((ref: string) => Rect | undefined),
}));

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    constructor(options: { rects: typeof effects.rects }) {
      effects.rects = options.rects;
    }
    setPersistent(next: unknown[]): void {
      effects.persistent.push(next);
    }
    replaceTransients(next: unknown[]): void {
      effects.transients.push(next);
    }
    trackMotion(): void {}
  },
}));

/** A six-seat table: `p1` receives, `p2`–`p6` are opponents in seat order. */
function sixSeats(perms: Parameters<typeof seatTable>[0]['perms'] = []): GameView {
  return seatTable({ opponents: 5, active: 'p2', perms });
}

/** Stage the plane at a viewport size (the mount reads it from the window). */
function viewport(width: number, height: number): void {
  vi.spyOn(window, 'innerWidth', 'get').mockReturnValue(width);
  vi.spyOn(window, 'innerHeight', 'get').mockReturnValue(height);
}

/** Render one view on the production mount with default presentation settings. */
function mount(view: GameView) {
  return render(
    <LivePlane view={view} quality="standard" density="reduced" reducedMotion={false} />,
  );
}

/** An element's data attribute, or `null` when the element is absent. */
function attr(selector: string, name: string): string | null {
  return document.querySelector(selector)?.getAttribute(name) ?? null;
}

describe('LivePlane off-focus staging', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
    viewport(1600, 900);
    effects.transients = [];
    effects.persistent = [];
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    effects.rects = undefined;
  });

  it('anchors an undrawn digest-wing attacker at its crest so the path still draws', () => {
    mount(sixSeats([{ id: 'p6_atk', controller: 'p6', attacking: true, attacking_player: 'p1' }]));

    // The wing digests: its attacker is not staged as an individual render…
    expect(attr('[data-slot="region"][data-seat="p6"]', 'data-rung')).toBe('4');
    expect(document.querySelector('[data-entity-id="p6_atk"]')).toBeNull();
    // …yet the effects layer still resolves it, folded onto the seat's crest,
    // so the attack path keeps a live source endpoint instead of retiring.
    const crest = effects.rects?.('seat:p6');
    expect(crest).toBeDefined();
    expect(effects.rects?.('p6_atk')).toEqual(crest);
    expect(effects.persistent.at(-1)).toContainEqual(
      expect.objectContaining({
        id: 'attack:p6_atk',
        category: 'attack-path',
        from: { ref: 'p6_atk' },
        to: { ref: 'seat:p1' },
      }),
    );
  });

  it('rings every attacked seat, including the digest wing that lost the focus', () => {
    // Multi-attacker, multi-defender (layout-model §Stress dispositions): the
    // first attacked seat auto-focuses, the other stays a digest wing — and
    // both wear the ring.
    mount(
      sixSeats([
        { id: 'p2_atk', controller: 'p2', attacking: true, attacking_player: 'p5' },
        { id: 'p3_atk', controller: 'p3', attacking: true, attacking_player: 'p6' },
      ]),
    );

    expect(attr('[data-slot="region"][data-seat="p5"]', 'data-kind')).toBe('far');
    expect(attr('[data-slot="crest"][data-seat="p5"]', 'data-attacked')).toBe('true');
    expect(attr('[data-slot="region"][data-seat="p6"]', 'data-rung')).toBe('4');
    expect(attr('[data-slot="crest"][data-seat="p6"]', 'data-attacked')).toBe('true');
    expect(attr('[data-slot="crest"][data-seat="p4"]', 'data-attacked')).toBe('false');
  });

  it('pings a digest wing that acts, alongside the rest of the batch', () => {
    const view = sixSeats();
    const { rerender } = mount(view);
    effects.transients = [];

    const next: GameView = {
      ...view,
      log: [{ sequence: 40, event: { type: 'cards_drawn', player: 'p6', count: 1 } }],
    };
    rerender(<LivePlane view={next} quality="standard" density="reduced" reducedMotion={false} />);

    expect(effects.transients.at(-1)).toContainEqual(
      expect.objectContaining({ category: 'off-focus-ping', target: { ref: 'seat:p6' } }),
    );
    // The seat the plane focused is staged, never pinged.
    expect(effects.transients.at(-1)).not.toContainEqual(
      expect.objectContaining({ category: 'off-focus-ping', target: { ref: 'seat:p2' } }),
    );
  });

  it('lands the same channel on the summary tile at compact geometry', () => {
    viewport(390, 844);
    mount(
      sixSeats([
        { id: 'p2_atk', controller: 'p2', attacking: true, attacking_player: 'p5' },
        { id: 'p3_atk', controller: 'p3', attacking: true, attacking_player: 'p6' },
      ]),
    );

    // Peripheral seats are tiles here; the ping's `seat:` anchor resolves to the
    // tile's mini-crest, and an attacked tile wears the same ring.
    expect(document.querySelector('[data-slot="tile"][data-seat="p6"]')).not.toBeNull();
    expect(effects.rects?.('seat:p6')).toBeDefined();
    expect(attr('[data-slot="tile"][data-seat="p6"]', 'data-attacked')).toBe('true');
    expect(attr('[data-slot="tile"][data-seat="p4"]', 'data-attacked')).toBe('false');
  });
});
