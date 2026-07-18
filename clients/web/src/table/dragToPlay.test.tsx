/**
 * Drag-to-play (blueprint §Interaction model): the pointer *enhancement* layered
 * over select-then-act. Dragging a playable hand card ghosts it under the pointer
 * and lights the legal drop area — the receiver's battlefield for an untargeted
 * play, the server-listed slot-0 candidates for a targeted spell — and releasing
 * fires exactly the server-offered action. Esc or a release outside cancels.
 *
 * Drops are resolved against SCENE rects (never DOM geometry), so these tests
 * drive the real <Table /> with pointer coordinates computed from the same pure
 * scene the table renders — jsdom reports no layout, and none is needed.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { SAMPLE_GAME_VIEW_JSON, TARGETING_GAME_VIEW_JSON } from '../game-view.fixture';
import type { TargetChoice, ValidAction } from '../protocol';
import { parseGameView } from '../wire';
import { useGameStore } from '../store';
import { layout } from './layout';
import { buildTableScene, type Rect, type TableScene } from './scene';
import { Table } from './Table';

/** A view whose hand holds an untargeted playable land (plus the pass action). */
const LAND_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [{ id: 'l1', name: 'Forest', type_line: 'Basic Land — Forest' }],
  opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
  battlefield: [],
  phase: 'precombat_main',
  valid_actions: [
    { id: 'a1', type: 'pass_priority', label: 'Pass', token: 'h:pass' },
    { id: 'a9', type: 'play_land', label: 'Play Forest', subject: ['l1'], token: 'h:land' },
  ],
});

function seed(json: string): ReturnType<typeof vi.fn> {
  const choose = vi.fn();
  useGameStore.getState().ingest(json);
  useGameStore.setState({ choose });
  return choose;
}

/**
 * The same scene the mounted table computes: the pure build over the same view
 * and the same measured viewport (jsdom's window), so test pointer coordinates
 * and the table's drop hit-testing share one geometry. The overlay root sits at
 * the client origin in jsdom, so scene coordinates ARE client coordinates.
 */
function sceneOf(json: string): TableScene {
  const view = parseGameView(json);
  const viewport = {
    width: window.innerWidth,
    height: window.innerHeight,
    pointer: 'fine' as const,
  };
  return buildTableScene(view, undefined, layout(viewport, 1 + view.opponents.length).scene);
}

const center = (rect: Rect): { clientX: number; clientY: number } => ({
  clientX: rect.x + rect.w / 2,
  clientY: rect.y + rect.h / 2,
});

/**
 * A coordinate-carrying pointer event. jsdom has no `PointerEvent`, and
 * testing-library's fallback (plain `Event`) silently drops `clientX`/`button` —
 * so build a `MouseEvent` (which jsdom implements fully) with the pointer event's
 * type; React and window listeners key on the type, not the constructor.
 */
function pointerEvent(type: string, init: { clientX: number; clientY: number }): MouseEvent {
  return new MouseEvent(type, { ...init, button: 0, bubbles: true, cancelable: true });
}

const pointerDown = (el: Element, at: { clientX: number; clientY: number }): void => {
  fireEvent(el, pointerEvent('pointerdown', at));
};
const pointerMove = (at: { clientX: number; clientY: number }): void => {
  fireEvent(window, pointerEvent('pointermove', at));
};
const pointerUp = (at: { clientX: number; clientY: number }): void => {
  fireEvent(window, pointerEvent('pointerup', at));
};

/** Drag the entity hotspot from its card rect to a destination point. */
function drag(entityId: string, from: Rect, to: { clientX: number; clientY: number }): void {
  pointerDown(screen.getByTestId(`entity-${entityId}`), center(from));
  // Past the travel threshold → the drag goes live (ghost + drop affordances).
  pointerMove({ clientX: center(from).clientX + 12, clientY: center(from).clientY });
  pointerMove(to);
}

afterEach(() => {
  cleanup();
  useGameStore.setState({ view: null });
});

describe('drag-to-play: untargeted play (blueprint §Interaction model)', () => {
  it('lights the battlefield inset, ghosts the card, and fires the play on drop', () => {
    const choose = seed(LAND_VIEW_JSON);
    render(<Table />);
    const scene = sceneOf(LAND_VIEW_JSON);
    const hand = scene.hand[0]!.rect;
    const board = scene.bands.find((b) => b.isLocal)!.rect;

    // The playable hand card advertises the drag affordance.
    expect(screen.getByTestId('entity-l1').getAttribute('data-draggable')).toBe('true');
    // Nothing is lit before a drag.
    expect(screen.queryByTestId('drop-board')).toBeNull();

    drag('l1', hand, center(board));
    // Mid-drag: the ghost rides the pointer and the legal drop area is lit gold.
    expect(screen.getByTestId('drag-ghost').textContent).toBe('Forest');
    expect(screen.getByTestId('drop-board')).toBeDefined();
    expect(choose).not.toHaveBeenCalled();

    pointerUp(center(board));
    // The drop fires exactly the server-offered action — no targets, one call.
    expect(choose).toHaveBeenCalledTimes(1);
    expect((choose.mock.calls[0]![0] as ValidAction).id).toBe('a9');
    expect(choose.mock.calls[0]![1]).toBeUndefined();
    // The drag chrome is gone.
    expect(screen.queryByTestId('drag-ghost')).toBeNull();
    expect(screen.queryByTestId('drop-board')).toBeNull();
  });

  it('cancels on a release outside the lit drop area — nothing fires', async () => {
    const choose = seed(LAND_VIEW_JSON);
    render(<Table />);
    const scene = sceneOf(LAND_VIEW_JSON);
    const hand = scene.hand[0]!.rect;

    // Release over the top bar area (well above the battlefield panel).
    drag('l1', hand, { clientX: 5, clientY: 5 });
    pointerUp({ clientX: 5, clientY: 5 });
    expect(choose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('drag-ghost')).toBeNull();

    // The click-swallow guard self-clears, so a later genuine click still selects.
    await new Promise((resolve) => setTimeout(resolve, 0));
    fireEvent.click(screen.getByTestId('entity-l1'));
    expect(screen.getByTestId('selection-echo')).toBeDefined();
  });

  it('cancels with Escape mid-drag (back to the origin slot)', () => {
    const choose = seed(LAND_VIEW_JSON);
    render(<Table />);
    const scene = sceneOf(LAND_VIEW_JSON);
    const hand = scene.hand[0]!.rect;
    const board = scene.bands.find((b) => b.isLocal)!.rect;

    drag('l1', hand, center(board));
    expect(screen.getByTestId('drag-ghost')).toBeDefined();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('drag-ghost')).toBeNull();
    // A release after the cancel fires nothing.
    pointerUp(center(board));
    expect(choose).not.toHaveBeenCalled();
  });

  it('keeps a short press an ordinary click — drag is layered on, never replacing', () => {
    seed(LAND_VIEW_JSON);
    render(<Table />);
    const scene = sceneOf(LAND_VIEW_JSON);
    const from = center(scene.hand[0]!.rect);

    // Press, wiggle under the threshold, release, click: the select path is intact.
    pointerDown(screen.getByTestId('entity-l1'), from);
    pointerMove({ clientX: from.clientX + 2, clientY: from.clientY });
    pointerUp({ clientX: from.clientX + 2, clientY: from.clientY });
    fireEvent.click(screen.getByTestId('entity-l1'));
    expect(screen.getByTestId('selection-echo')).toBeDefined();
  });

  it('marks only playable hand cards draggable — a battlefield permanent is select-only', () => {
    // SAMPLE: perm_xyz is actionable (an activate ability) but lives on the
    // battlefield — its interaction is select-then-dock, never a drag.
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<Table />);
    expect(screen.getByTestId('entity-perm_xyz').getAttribute('data-draggable')).toBeNull();
  });
});

describe('drag-to-play: targeted spell (orange rings on the slot-0 candidates)', () => {
  it('rings exactly the candidate cards and casts with the dropped target, atomically', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    const scene = sceneOf(TARGETING_GAME_VIEW_JSON);
    const hand = scene.hand.find((c) => c.entityId === 'c3')!.rect;
    const bearRect = scene.bands
      .flatMap((b) => b.cards)
      .find((c) => c.entityId === 'perm_xyz')!.rect;

    drag('c3', hand, center(bearRect));
    // A targeted spell lights its candidates, not the board inset; the player
    // candidate (p2) is not a canvas card, so only the permanent rings.
    expect(screen.getByTestId('drop-target-perm_xyz')).toBeDefined();
    expect(screen.queryByTestId('drop-board')).toBeNull();
    expect(screen.queryByTestId('drop-target-p2')).toBeNull();
    expect(choose).not.toHaveBeenCalled();

    pointerUp(center(bearRect));
    // Cast + target land as ONE atomic answer with the content-binding token.
    expect(choose).toHaveBeenCalledTimes(1);
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action.id).toBe('a3');
    expect(action.token).toBe('h:9f2c');
    expect(targets).toEqual([{ slot: 't0', chosen: ['perm_xyz'] }]);
    // No lingering targeting session: the answer was complete.
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
  });

  it('a release on a non-candidate card cancels — candidates came from the server', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<Table />);
    const scene = sceneOf(TARGETING_GAME_VIEW_JSON);
    const hand = scene.hand.find((c) => c.entityId === 'c3')!.rect;

    // Release over the spell's own hand slot (c3 is not a candidate for itself).
    drag('c3', hand, center(hand));
    pointerUp(center(hand));
    expect(choose).not.toHaveBeenCalled();
    expect(screen.queryByTestId('targeting-prompt')).toBeNull();
  });
});
