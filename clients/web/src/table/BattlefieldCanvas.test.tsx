/**
 * Regression tests for the two invariants that made the board vanish in dev
 * (issue #276): the React-owned `<canvas>` must survive a StrictMode
 * mount→cleanup→mount cycle, and a GL-capable client that still fails must show
 * a visible fallback rather than a silently blank board.
 *
 * Pixi and the reconciler are mocked so the test is deterministic in jsdom: the
 * fake `Application` mirrors Pixi v7's `destroy(removeView)` contract — it
 * detaches its view from the DOM iff `removeView` is truthy — which is exactly
 * the behavior the fix must avoid triggering.
 */
import { StrictMode } from 'react';
import { render, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import type { TableScene } from './scene';

const destroyCalls: (boolean | undefined)[] = [];
let constructShouldThrow = false;

vi.mock('pixi.js', () => ({
  // Mirrors Pixi v7: destroy(removeView=true) removes the canvas from the DOM.
  Application: class {
    view: HTMLCanvasElement;
    stage = { addChild: () => {} };
    renderer = { resize: () => {} };
    constructor(opts: { view: HTMLCanvasElement }) {
      if (constructShouldThrow) throw new Error('no GL');
      this.view = opts.view;
    }
    destroy(removeView?: boolean) {
      destroyCalls.push(removeView);
      if (removeView) this.view.remove();
    }
  },
  Container: class {
    addChild() {}
  },
}));

vi.mock('./sceneReconciler', () => ({
  SceneReconciler: class {
    reconcile() {}
  },
}));

function makeScene(overrides: Partial<TableScene> = {}): TableScene {
  return {
    width: 200,
    height: 200,
    bands: [],
    hand: [],
    handRegion: { rect: { x: 0, y: 0, w: 200, h: 24 }, label: 'Your hand' },
    ...overrides,
  };
}

/** Force `webglSupported()` to a given answer by stubbing the probe context. */
function stubWebgl(supported: boolean) {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(() =>
    supported ? ({} as unknown as RenderingContext) : null,
  );
}

afterEach(() => {
  cleanup();
  destroyCalls.length = 0;
  constructShouldThrow = false;
  vi.restoreAllMocks();
});

describe('BattlefieldCanvas', () => {
  it('keeps its canvas attached across a StrictMode mount→cleanup→mount cycle', () => {
    const { container } = render(
      <StrictMode>
        <BattlefieldCanvas scene={makeScene()} />
      </StrictMode>,
    );

    // The element React owns must still be in the DOM after StrictMode's
    // double-invoke ran a full cleanup between the two mounts.
    expect(container.querySelector('canvas')).not.toBeNull();
    // And cleanup must never have asked Pixi to remove the view.
    expect(destroyCalls.length).toBeGreaterThan(0);
    expect(destroyCalls.every((removeView) => removeView === false)).toBe(true);
  });

  it('passes removeView=false to destroy on unmount', () => {
    const { unmount } = render(<BattlefieldCanvas scene={makeScene()} />);
    unmount();
    expect(destroyCalls).toContain(false);
    expect(destroyCalls).not.toContain(true);
  });

  it('shows a visible fallback when a GL-capable client cannot start the canvas', () => {
    stubWebgl(true);
    constructShouldThrow = true;
    const scene = makeScene({
      hand: [{ name: 'Emberfang Raider' } as TableScene['hand'][number]],
    });

    const { queryByTestId, getByRole } = render(<BattlefieldCanvas scene={scene} />);

    const fallback = queryByTestId('board-render-fallback');
    expect(fallback).not.toBeNull();
    expect(getByRole('alert').textContent).toContain('Board rendering failed');
    expect(fallback?.textContent).toContain('Emberfang Raider');
  });

  it('stays silently blank (no fallback) where WebGL is unavailable', () => {
    stubWebgl(false);
    constructShouldThrow = true;

    const { queryByTestId } = render(<BattlefieldCanvas scene={makeScene()} />);

    expect(queryByTestId('board-render-fallback')).toBeNull();
  });
});
