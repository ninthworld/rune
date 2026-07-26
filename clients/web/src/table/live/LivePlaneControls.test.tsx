import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GameView } from '../../protocol';
import type { PlaneRegion, StagedPlane } from '../plane';
import { stageSeatCluster } from '../plane';
import { LivePlaneControls, type LivePlaneInteractionProps } from './LivePlaneControls';

const rect = { x: 20, y: 30, w: 72, h: 100 };

/** A staged cluster for a fixture seat — the controls read its label and hit rect. */
function fixtureCluster(seat: string, label: string) {
  return stageSeatCluster({
    seat,
    variant: 'local',
    anchor: { x: 60, y: 60 },
    viewport: { width: 600, height: 400 },
    outboard: 'left',
    facts: {
      label,
      local: true,
      life: 20,
      handCount: 7,
      libraryCount: 40,
      commanderPresent: false,
      statuses: [],
      attackedCount: 0,
      autoPassed: false,
      deadline: false,
      accent: '#4D7EC9',
      eliminated: false,
      priority: false,
      active: false,
      focused: false,
      attacked: false,
    },
  });
}

const region: PlaneRegion = {
  seat: 'p1',
  kind: 'receiver',
  rect: { x: 0, y: 0, w: 600, h: 300 },
  crest: { x: 0, y: 0, w: 52, h: 52 },
  cluster: fixtureCluster('p1', 'You'),
  piles: { x: 540, y: 220, w: 44, h: 62 },
  rack: {
    seat: 'p1',
    indicators: [],
    variant: 'local',
    axis: 'vertical',
    u: 40,
    origin: { x: 552, y: 200 },
    slots: (['library', 'graveyard', 'exile'] as const).map((zone, i) => ({
      zone,
      rect: { x: 540, y: 220 + i * 20, w: 44, h: 44 },
      hitRect: { x: 540, y: 220 + i * 20, w: 44, h: 44 },
      count: 0,
    })),
    bounds: { x: 540, y: 220, w: 44, h: 62 },
    inset: { left: 0, right: 0 },
  },
  zones: { library: 40, graveyard: 0, exile: 0 },
  surface: 'field',
  rung: 2,
  renders: [
    {
      entityId: 'land_a',
      seat: 'p1',
      name: 'Forest',
      row: 'lands',
      tier: 'chip',
      rect,
      hitRect: rect,
      tapped: false,
      memberIds: ['land_a', 'land_b'],
      stackCount: 2,
      candidate: false,
      attacking: false,
      blocking: false,
    },
  ],
  label: 'You',
  life: 20,
  handCount: 0,
  eliminated: false,
  focused: false,
  attacked: false,
  active: true,
  priority: true,
};

const plane: StagedPlane = {
  width: 640,
  height: 480,
  compact: false,
  corridor: { x: 0, y: 300, w: 640, h: 40 },
  receiver: region,
  wings: [],
  tiles: [],
  seats: ['p1'],
};

const view = {
  you: 'p1',
  my_hand: [],
  opponents: [],
  battlefield: [
    {
      id: 'land_a',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'forest', name: 'Forest', type_line: 'Basic Land — Forest' },
    },
    {
      id: 'land_b',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'forest', name: 'Forest', type_line: 'Basic Land — Forest' },
    },
  ],
  phase: 'precombat_main',
  valid_actions: [
    {
      id: 'tap_a',
      type: 'activate_ability',
      label: '{T}: Add {G}.',
      subject: ['land_a'],
      token: 'a',
    },
    {
      id: 'tap_b',
      type: 'activate_ability',
      label: '{T}: Add {G}.',
      subject: ['land_b'],
      token: 'b',
    },
  ],
} as unknown as GameView;

/**
 * The same seat with its rack digested (zone-geography §6): one button, and
 * every zone key resolving to that one rect — which is exactly the staging that
 * made two absolutely-positioned zone controls coincide.
 */
const digestBounds = { x: 540, y: 20, w: 44, h: 62 };
const digestPlane: StagedPlane = {
  ...plane,
  receiver: {
    ...region,
    piles: digestBounds,
    zones: { library: 40, graveyard: 2, exile: 1 },
    rack: {
      ...region.rack,
      variant: 'digest',
      u: 0,
      slots: [
        { zone: 'library', count: 40 },
        { zone: 'graveyard', count: 2 },
        { zone: 'exile', count: 1 },
      ].map(({ zone, count }) => ({
        zone: zone as 'library' | 'graveyard' | 'exile',
        rect: digestBounds,
        hitRect: digestBounds,
        count,
      })),
      bounds: digestBounds,
    },
  },
};

/** A control's staged rect, read back off the inline box the layer positions with. */
function boxOf(el: HTMLElement): { x: number; y: number; w: number; h: number } {
  return {
    x: Number.parseFloat(el.style.left),
    y: Number.parseFloat(el.style.top),
    w: Number.parseFloat(el.style.width),
    h: Number.parseFloat(el.style.height),
  };
}

/** Whether two staged rects share positive area. */
function overlaps(
  a: { x: number; y: number; w: number; h: number },
  b: { x: number; y: number; w: number; h: number },
): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

afterEach(() => document.body.replaceChildren());

function interaction(
  overrides: Partial<LivePlaneInteractionProps> = {},
): LivePlaneInteractionProps {
  return {
    selectedId: null,
    picking: false,
    multiSelect: false,
    candidates: [],
    chosen: [],
    playerCandidates: [],
    onActivateEntity: vi.fn(),
    onPickEntity: vi.fn(),
    onPickPlayer: vi.fn(),
    onInspect: vi.fn(),
    onOpenZone: vi.fn(),
    onFocusSeat: vi.fn(),
    ...overrides,
  };
}

describe('LivePlaneControls', () => {
  it('names the seat control with the whole cluster sentence (seat-identity §9)', () => {
    // The cluster exposes ONE accessible name that reads the seat entire — name,
    // life, hand, library, and every state — so a screen-reader user never has
    // to open anything to know where a seat stands. The verb stays first.
    render(<LivePlaneControls view={view} plane={plane} interaction={interaction()} />);
    const label = screen.getByTestId('focus-seat-p1').getAttribute('aria-label');
    expect(label).toContain('Focus You (you) battlefield');
    expect(label).toContain('You (you), 20 life, 7 in hand, 40 in library');
  });

  it('keeps the §9 targeting phrasing when the seat is a prompt candidate', () => {
    render(
      <LivePlaneControls
        view={view}
        plane={plane}
        interaction={interaction({ picking: true, playerCandidates: ['p1'] })}
      />,
    );
    expect(screen.getByTestId('target-player-p1').getAttribute('aria-label')).toMatch(
      /^Target player You \(you\)\./,
    );
  });

  it('anchors a folded stack to its representative member action', () => {
    const onActivateEntity = vi.fn();
    render(
      <LivePlaneControls
        view={view}
        plane={plane}
        interaction={interaction({ onActivateEntity })}
      />,
    );

    fireEvent.click(screen.getByTestId('entity-land_a'));
    expect(onActivateEntity).toHaveBeenCalledWith('land_a');
  });

  it('anchors a candidate member id explicitly when a prompt pierces a fold', () => {
    const onPickEntity = vi.fn();
    render(
      <LivePlaneControls
        view={view}
        plane={plane}
        interaction={interaction({
          picking: true,
          candidates: ['land_b'],
          onPickEntity,
        })}
      />,
    );

    fireEvent.click(screen.getByTestId('target-land_b'));
    expect(onPickEntity).toHaveBeenCalledWith('land_b');
  });

  it('routes a folded declaration through the eligible member, not the representative', () => {
    const onActivateEntity = vi.fn();
    const declarationView = {
      ...view,
      valid_actions: [
        {
          id: 'attack',
          type: 'declare_attackers',
          label: 'Declare attackers',
          token: 'attack-token',
          requirements: [
            {
              slot: 'attackers',
              candidates: ['land_b'],
            },
          ],
        },
      ],
    } as unknown as GameView;
    render(
      <LivePlaneControls
        view={declarationView}
        plane={plane}
        interaction={interaction({ onActivateEntity })}
      />,
    );

    fireEvent.click(screen.getByTestId('entity-land_b'));
    expect(onActivateEntity).toHaveBeenCalledWith('land_b');
  });

  it('gives a drawn rack one control per public zone, on its own slot rect', () => {
    render(<LivePlaneControls view={view} plane={plane} interaction={interaction()} />);

    expect(screen.queryByTestId('rack-digest-p1')).toBeNull();
    const rects = ['graveyard', 'exile'].map((zone) =>
      boxOf(screen.getByTestId(`table-${zone}-p1`)),
    );
    expect(rects[0]).not.toEqual(rects[1]);
  });

  /**
   * The digest rack (zone-geography §6/§7) resolves every zone key to ONE button
   * rect. Two controls positioned from it therefore overlap exactly, and the
   * later one wins pointer and touch hit-testing — which left the graveyard
   * keyboard-only at the normal 5–6-seat wing treatment and at narrow viewports.
   * §6.2's expansion is the fix: one ≥ 44 px button that opens the seat's zones
   * as separate targets.
   */
  describe('a digest rack expands rather than stacking its zone controls', () => {
    it('offers one ≥ 44 px rack control naming the seat and its counts', () => {
      render(<LivePlaneControls view={view} plane={digestPlane} interaction={interaction()} />);

      const rack = screen.getByTestId('rack-digest-p1');
      expect(rack.getAttribute('aria-label')).toBe(
        'Open You zones: library 40, graveyard 2, exile 1',
      );
      expect(rack.getAttribute('aria-expanded')).toBe('false');
      const box = boxOf(rack);
      expect(box.w).toBeGreaterThanOrEqual(44);
      expect(box.h).toBeGreaterThanOrEqual(44);
      // Collapsed, the seat has no per-zone controls to overlap at all.
      expect(screen.queryByTestId('table-graveyard-p1')).toBeNull();
      expect(screen.queryByTestId('table-exile-p1')).toBeNull();
    });

    it('opens both public zones as separate, non-overlapping ≥ 44 px targets', () => {
      render(<LivePlaneControls view={view} plane={digestPlane} interaction={interaction()} />);

      fireEvent.click(screen.getByTestId('rack-digest-p1'));
      expect(screen.getByTestId('rack-digest-p1').getAttribute('aria-expanded')).toBe('true');

      const boxes = ['rack-digest-p1', 'table-graveyard-p1', 'table-exile-p1'].map((id) =>
        boxOf(screen.getByTestId(id)),
      );
      for (const box of boxes) {
        expect(box.w).toBeGreaterThanOrEqual(44);
        expect(box.h).toBeGreaterThanOrEqual(44);
      }
      // No two controls share a rect, and none of them overlap: the whole point.
      for (let i = 0; i < boxes.length; i += 1) {
        for (let j = i + 1; j < boxes.length; j += 1) {
          expect(boxes[i]).not.toEqual(boxes[j]);
          expect(overlaps(boxes[i], boxes[j])).toBe(false);
        }
      }
    });

    it('routes each opened zone to the existing onOpenZone, by pointer', () => {
      const onOpenZone = vi.fn();
      render(
        <LivePlaneControls
          view={view}
          plane={digestPlane}
          interaction={interaction({ onOpenZone })}
        />,
      );

      fireEvent.click(screen.getByTestId('rack-digest-p1'));
      fireEvent.click(screen.getByTestId('table-graveyard-p1'));
      expect(onOpenZone).toHaveBeenCalledWith('p1', 'graveyard');

      fireEvent.click(screen.getByTestId('rack-digest-p1'));
      fireEvent.click(screen.getByTestId('table-exile-p1'));
      expect(onOpenZone).toHaveBeenCalledWith('p1', 'exile');
    });

    /**
     * Activating a library never browses (§I2) and no wire action names it, so a
     * browse affordance for it would be client-invented. Same for the command
     * slot, whose popover needs commander data the view does not carry.
     */
    it('never offers the library or the command slot as a browse target', () => {
      render(<LivePlaneControls view={view} plane={digestPlane} interaction={interaction()} />);
      fireEvent.click(screen.getByTestId('rack-digest-p1'));

      expect(screen.queryByTestId('table-library-p1')).toBeNull();
      expect(screen.queryByTestId('table-command-p1')).toBeNull();
      for (const button of screen.getAllByRole('button')) {
        expect(button.getAttribute('aria-label')).not.toMatch(/browse .*(library|command)/i);
      }
    });

    it('drops the expansion when a fresh view arrives (presentation state only)', () => {
      const { rerender } = render(
        <LivePlaneControls view={view} plane={digestPlane} interaction={interaction()} />,
      );
      fireEvent.click(screen.getByTestId('rack-digest-p1'));
      expect(screen.queryByTestId('table-graveyard-p1')).not.toBeNull();

      rerender(
        <LivePlaneControls
          view={{ ...view } as GameView}
          plane={digestPlane}
          interaction={interaction()}
        />,
      );
      expect(screen.queryByTestId('table-graveyard-p1')).toBeNull();
    });
  });

  it('turns compact candidate overflow into a focus-and-restage fallback', () => {
    const onFocusSeat = vi.fn();
    const compact: StagedPlane = {
      ...plane,
      compact: true,
      receiver: undefined,
      tiles: [
        {
          seat: 'p2',
          rect: { x: 10, y: 10, w: 220, h: 48 },
          crest: { x: 18, y: 18, w: 32, h: 32 },
          accent: '#B0563F',
          label: 'Opponent',
          life: 20,
          handCount: 4,
          zones: { library: 30, graveyard: 0, exile: 0 },
          candidates: [],
          candidateOverflow: 3,
          eliminated: false,
          attacked: false,
          active: false,
          priority: false,
        },
      ],
      seats: ['p2'],
    };
    render(
      <LivePlaneControls view={view} plane={compact} interaction={interaction({ onFocusSeat })} />,
    );

    const focus = screen.getByTestId('focus-seat-p2');
    expect(focus.getAttribute('aria-label')).toContain('choose 3 more candidates');
    fireEvent.click(focus);
    expect(onFocusSeat).toHaveBeenCalledWith('p2');
  });
});
