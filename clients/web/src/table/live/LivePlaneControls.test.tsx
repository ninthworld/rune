import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GameView } from '../../protocol';
import type { PlaneRegion, StagedPlane } from '../plane';
import { LivePlaneControls, type LivePlaneInteractionProps } from './LivePlaneControls';

const rect = { x: 20, y: 30, w: 72, h: 100 };
const region: PlaneRegion = {
  seat: 'p1',
  kind: 'receiver',
  rect: { x: 0, y: 0, w: 600, h: 300 },
  crest: { x: 0, y: 0, w: 52, h: 52 },
  piles: { x: 540, y: 220, w: 44, h: 62 },
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
