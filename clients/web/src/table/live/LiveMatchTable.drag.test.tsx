/**
 * The hand drag **presentation** (issue #569, absorbing #568).
 *
 * The behaviour of drag — the threshold, the drop resolution, `Escape`, the
 * click suppression, and the fact that a drop only ever sends a server-issued
 * `action_id` — is covered by `LiveMatchTable.test.tsx` and stays covered there.
 * What this file pins is what the player *sees*: the real resolved card face
 * follows the pointer instead of a name box, and the slot the card came out of
 * is visibly held open.
 *
 * **What jsdom cannot show.** No CSS module is applied and no layout is
 * computed, so nothing here proves the dashed origin outline is drawn, the tilt
 * angle, or that the proxy is under the cursor. Those are the maintainer's
 * browser check. jsdom's `PointerEvent` also drops `clientX`/`clientY`
 * entirely, so the 6 px arming threshold and the proxy's tracking of the
 * pointer cannot be exercised here at all and are stated rather than asserted.
 * What IS proven is the tree the stylesheet is handed: which component renders
 * the proxy, what marks the vacated slot, and that neither is reachable by hit
 * testing or by assistive technology.
 */
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

registerTableTestHooks();

/** The sample view with exactly one playable hand card (`c1`). */
function playableView(): string {
  const raw = JSON.parse(SAMPLE_GAME_VIEW_JSON) as Record<string, unknown>;
  raw.valid_actions = [
    {
      id: 'play-c1',
      type: 'cast_spell',
      label: 'Cast Llanowar Elves',
      subject: ['c1'],
      token: 'h:play',
    },
  ];
  return JSON.stringify(raw);
}

/** Arm and start a drag on hand card `c1`. */
function startDrag(): void {
  const hand = screen.getByTestId('live-hand-card-c1');
  fireEvent.pointerDown(hand, { button: 0, clientX: 40, clientY: 500 });
  fireEvent.pointerMove(window, { clientX: 90, clientY: 400 });
}

describe('hand drag presentation (issue #569)', () => {
  it('drags the real resolved card face, not a name box', () => {
    seed(playableView());
    render(<LiveMatchTable />);
    startDrag();

    const proxy = screen.getByTestId('drag-ghost');
    // The one card renderer, at the hand tier, held at the drag elevation —
    // the same component and the same tier the card wears in the fan.
    const face = proxy.querySelector('[data-tier]');
    expect(face?.getAttribute('data-tier')).toBe('hand');
    expect(face?.getAttribute('data-kind')).toBe('card');
    expect(face?.getAttribute('data-elevation')).toBe('held');
    expect(face?.getAttribute('aria-label')).toBe('Llanowar Elves');
    // It is the card, not a caption: the frame's own bands came with it.
    expect(face?.textContent).toContain('Creature — Elf Druid');
    expect(proxy.getAttribute('data-entity-drag')).toBe('c1');
  });

  it('keeps the proxy out of hit testing and out of the accessibility tree', () => {
    seed(playableView());
    render(<LiveMatchTable />);
    startDrag();

    const proxy = screen.getByTestId('drag-ghost');
    // `aria-hidden` because the origin button keeps the accessible name and
    // every keyboard path; the proxy is pure decoration for the pointer.
    expect(proxy.getAttribute('aria-hidden')).toBe('true');
    // And it carries no `data-entity`, so the drop's `elementFromPoint` walk can
    // never resolve the dragged card as its own drop target.
    expect(proxy.querySelector('[data-entity]')).toBeNull();
  });

  it('holds the origin slot open while the card is in flight', () => {
    seed(playableView());
    render(<LiveMatchTable />);
    const slot = screen.getByTestId('live-hand-card-c1');
    expect(slot.getAttribute('data-vacated')).toBeNull();

    startDrag();
    // The slot the card was lifted out of is marked vacated — the stylesheet's
    // dashed origin outline (`control-language.md` §6.2 stage 2a) selects on it.
    expect(screen.getByTestId('live-hand-card-c1').getAttribute('data-vacated')).toBe('true');
    // Only that slot: the rest of the fan is untouched.
    for (const other of ['c2', 'c3']) {
      const el = screen.queryByTestId(`live-hand-card-${other}`);
      if (el) expect(el.getAttribute('data-vacated')).toBeNull();
    }
    // The button itself stays in the tree, named and operable, so a cancelled
    // drag restores nothing — there is nothing to restore.
    expect(slot.isConnected).toBe(true);
    expect(slot.getAttribute('aria-label')).toContain('Llanowar Elves');
  });

  it('restores the slot when Escape cancels the drag', () => {
    seed(playableView());
    render(<LiveMatchTable />);
    startDrag();
    expect(screen.getByTestId('drag-ghost')).toBeTruthy();

    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByTestId('drag-ghost')).toBeNull();
    expect(screen.getByTestId('live-hand-card-c1').getAttribute('data-vacated')).toBeNull();
    // The card is still in the fan, drawn as it always was.
    expect(
      within(screen.getByTestId('live-hand-card-c1')).getByRole('img', { name: 'Llanowar Elves' }),
    ).toBeDefined();
  });
});
