/**
 * Regression tests for the canvas that vanished / failed to paint in the dev
 * client (issue #276). The load-bearing invariant: Pixi owns its own canvas
 * inside a React-owned host `<div>`, so a StrictMode mount→cleanup→mount cycle
 * disposes the first canvas (and its GL context) and mounts a *fresh* one — never
 * reusing a context-less canvas (which is what made the board fail to paint).
 *
 * Pixi and the reconciler are mocked so the test is deterministic in jsdom: the
 * fake `Application` creates its own canvas (as Pixi does when no `view` is
 * passed) and `destroy(removeView=true)` removes that canvas from the DOM.
 */
import { StrictMode } from 'react';
import { render, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import type { TableScene } from './scene';

const createdViews: HTMLCanvasElement[] = [];
const destroyCalls: (boolean | undefined)[] = [];
let constructShouldThrow = false;

vi.mock('pixi.js', () => ({
  // Mirrors Pixi v7 with no `view` passed: the Application creates its own canvas,
  // and destroy(removeView=true) removes that canvas from the DOM.
  Application: class {
    view: HTMLCanvasElement;
    stage = { addChild: () => {} };
    renderer = { resize: () => {} };
    constructor() {
      if (constructShouldThrow) throw new Error('no GL');
      this.view = document.createElement('canvas');
      this.view.width = 200;
      createdViews.push(this.view);
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
  createdViews.length = 0;
  destroyCalls.length = 0;
  constructShouldThrow = false;
  vi.restoreAllMocks();
});

describe('BattlefieldCanvas', () => {
  it('mounts a fresh canvas after a StrictMode mount→cleanup→mount cycle', () => {
    const { container } = render(
      <StrictMode>
        <BattlefieldCanvas scene={makeScene()} />
      </StrictMode>,
    );

    // The React-owned host survives the cycle and holds exactly one canvas.
    const host = container.querySelector<HTMLElement>('[data-testid="battlefield-canvas-host"]');
    expect(host).not.toBeNull();
    const canvases = host!.querySelectorAll('canvas');
    expect(canvases).toHaveLength(1);

    // That canvas is the *most recently* created one — the stale first canvas was
    // destroyed with its context, not reused (the #276 failure mode).
    expect(canvases[0]).toBe(createdViews.at(-1));
    expect(createdViews[0]?.isConnected).toBe(false);
    // Cleanup disposed the app's own canvas (removeView = true).
    expect(destroyCalls).toContain(true);
  });

  it('disposes the Pixi app and its canvas on unmount', () => {
    const { container, unmount } = render(<BattlefieldCanvas scene={makeScene()} />);
    const host = container.querySelector('[data-testid="battlefield-canvas-host"]')!;
    expect(host.querySelector('canvas')).not.toBeNull();
    unmount();
    expect(destroyCalls).toContain(true);
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
