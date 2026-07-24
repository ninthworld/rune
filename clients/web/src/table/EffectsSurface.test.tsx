/**
 * The effects surface mount (issue #482): the passive-overlay contract —
 * `pointer-events: none`, `aria-hidden`, silent where no WebGL exists
 * (jsdom) — and a clean unmount. Effect behavior itself is covered GPU-free
 * in `effectsLayer.test.ts`; this component only hosts the layer.
 */
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { EffectsLayer } from './effects';
import { EffectsSurface } from './EffectsSurface';

afterEach(cleanup);

function makeLayer(): EffectsLayer {
  return new EffectsLayer({
    quality: 'standard',
    density: 'full',
    reducedMotion: false,
    rects: () => undefined,
  });
}

describe('EffectsSurface (passive overlay contract)', () => {
  it('mounts a pointer-transparent, aria-hidden host over the plane', () => {
    render(<EffectsSurface layer={makeLayer()} width={1280} height={800} />);
    const host = screen.getByTestId('effects-surface');
    expect(host.getAttribute('aria-hidden')).toBe('true');
    expect(host.style.pointerEvents).toBe('none');
    expect(host.style.position).toBe('absolute');
  });

  it('stays silent without WebGL (headless): no canvas, no fallback, no throw', () => {
    const layer = makeLayer();
    const { unmount } = render(<EffectsSurface layer={layer} width={640} height={480} />);
    const host = screen.getByTestId('effects-surface');
    expect(host.querySelector('canvas')).toBeNull();
    // The layer stays fully usable headless — effects are decoration only.
    layer.spawn({
      category: 'resolution',
      target: { rect: { x: 0, y: 0, w: 10, h: 10 } },
      accent: '#F2C94C',
    });
    expect(layer.advance(0)).toBe(true);
    unmount();
    expect(layer.wake).toBeUndefined();
  });
});
