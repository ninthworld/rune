/**
 * The play-affordance half of issue #277: the canvas draws the visible "playable"
 * edge bar, and the overlay's select hotspot must carry an accessible-tree
 * equivalent (a hint naming the offered action) so the affordance survives for a
 * screen-reader or no-color-vision user. Nothing outside `valid_actions[]` may be
 * interactive, so an inert card gets no hotspot at all.
 */
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { EntityOverlay } from './EntityOverlay';
import { buildTableScene, defaultSceneGeometry, type TargetingScene } from './scene';
import { SAMPLE_GAME_VIEW } from '../game-view.fixture';

const noop = () => {};

interface OverlayOpts {
  pointer?: 'fine' | 'coarse';
  targeting?: TargetingScene;
  onPeek?: (id: string | null) => void;
  onPinInspect?: (id: string) => void;
}

function renderOverlay(opts: OverlayOpts = {}) {
  const scene = buildTableScene(
    SAMPLE_GAME_VIEW,
    undefined,
    defaultSceneGeometry(),
    opts.targeting,
  );
  render(
    <EntityOverlay
      scene={scene}
      selectedId={null}
      targeting={opts.targeting !== undefined}
      pointer={opts.pointer ?? 'fine'}
      onSelect={noop}
      onPickTarget={noop}
      onPeek={opts.onPeek ?? noop}
      onPinInspect={opts.onPinInspect ?? noop}
    />,
  );
}

afterEach(cleanup);

describe('EntityOverlay play affordance (issue #277)', () => {
  it('names the offered action on an actionable card so the hint is accessible', () => {
    renderOverlay();
    // perm_xyz carries the "Tap for mana" activate-ability action.
    const hotspot = screen.getByTestId('entity-perm_xyz');
    expect(hotspot.getAttribute('data-actionable')).toBe('true');
    expect(hotspot.getAttribute('aria-label')).toContain('Tap for mana');
  });

  it('gives no select hotspot to a card with no offered action', () => {
    renderOverlay();
    // The inert hand card (Llanowar Elves, c1) carries no subject-action, so it is
    // neither actionable nor selectable — only its transparent inspect surface exists.
    expect(screen.queryByTestId('entity-c1')).toBeNull();
    expect(screen.getByTestId('inspect-surface-c1')).toBeDefined();
  });
});

describe('EntityOverlay inspect gestures (issue #321)', () => {
  it('draws no visible per-card inspect handle — inert cards get a surface instead', () => {
    renderOverlay({ onPeek: noop, onPinInspect: noop });
    // The old always-on "i" handle is gone; the surface is the transparent stand-in.
    expect(screen.queryByTestId('inspect-c1')).toBeNull();
    const surface = screen.getByTestId('inspect-surface-c1');
    expect(surface.tagName).toBe('BUTTON');
  });

  it('hover-dwell opens a peek on a precise pointer, and leaving clears it', () => {
    vi.useFakeTimers();
    try {
      const onPeek = vi.fn();
      renderOverlay({ pointer: 'fine', onPeek });
      const surface = screen.getByTestId('inspect-surface-c1');
      fireEvent.pointerEnter(surface, { pointerType: 'mouse' });
      // Nothing yet — the dwell has not elapsed.
      expect(onPeek).not.toHaveBeenCalled();
      act(() => void vi.advanceTimersByTime(450));
      expect(onPeek).toHaveBeenCalledWith('c1');
      fireEvent.pointerLeave(surface);
      expect(onPeek).toHaveBeenLastCalledWith(null);
    } finally {
      vi.useRealTimers();
    }
  });

  it('long-press opens a peek on touch', () => {
    vi.useFakeTimers();
    try {
      const onPeek = vi.fn();
      renderOverlay({ pointer: 'coarse', onPeek });
      const surface = screen.getByTestId('inspect-surface-c1');
      fireEvent.pointerDown(surface, { pointerType: 'touch' });
      expect(onPeek).not.toHaveBeenCalled();
      act(() => void vi.advanceTimersByTime(550));
      expect(onPeek).toHaveBeenCalledWith('c1');
    } finally {
      vi.useRealTimers();
    }
  });

  it('right-click pins the full inspect panel', () => {
    const onPinInspect = vi.fn();
    renderOverlay({ onPinInspect });
    fireEvent.contextMenu(screen.getByTestId('inspect-surface-c1'));
    expect(onPinInspect).toHaveBeenCalledWith('c1');
  });

  it('suppresses the hover peek during targeting but still allows pinning', () => {
    vi.useFakeTimers();
    try {
      const onPeek = vi.fn();
      const onPinInspect = vi.fn();
      // perm_xyz is a candidate → a target hotspot carries the gestures.
      renderOverlay({ targeting: { candidates: ['perm_xyz'] }, onPeek, onPinInspect });
      const target = screen.getByTestId('target-perm_xyz');
      fireEvent.pointerEnter(target, { pointerType: 'mouse' });
      act(() => void vi.advanceTimersByTime(500));
      // No peek mid-pick…
      expect(onPeek).not.toHaveBeenCalled();
      // …but right-click still pins.
      fireEvent.contextMenu(target);
      expect(onPinInspect).toHaveBeenCalledWith('perm_xyz');
    } finally {
      vi.useRealTimers();
    }
  });
});
