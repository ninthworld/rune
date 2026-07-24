import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import { LivePlane } from './LivePlane';

const effectsMock = vi.hoisted(() => ({
  rects: undefined as
    undefined | ((ref: string) => { x: number; y: number; w: number; h: number } | undefined),
}));

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    constructor(options: { rects: typeof effectsMock.rects }) {
      effectsMock.rects = options.rects;
    }
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

describe('LivePlane', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    effectsMock.rects = undefined;
  });

  it('maps every folded member id to its representative visual rect', () => {
    const first = { ...SAMPLE_GAME_VIEW.battlefield[0]!, tapped: false };
    const copies = Array.from({ length: 20 }, (_, index) => {
      const id = index === 0 ? 'perm_xyz' : `perm_folded_${index}`;
      return { ...first, id, card: { ...first.card, id } };
    });
    render(
      <LivePlane
        view={{
          ...SAMPLE_GAME_VIEW,
          battlefield: copies,
          valid_actions: [],
        }}
        quality="standard"
        density="reduced"
        reducedMotion
      />,
    );

    expect(effectsMock.rects?.('perm_xyz')).toBeDefined();
    expect(effectsMock.rects?.('perm_folded_19')).toEqual(effectsMock.rects?.('perm_xyz'));
  });

  it('mounts a complete production plane from one GameView without entrance motion', () => {
    const onPlane = vi.fn();
    render(
      <LivePlane
        view={SAMPLE_GAME_VIEW}
        quality="standard"
        density="reduced"
        reducedMotion={false}
        onPlane={onPlane}
      />,
    );

    expect(screen.getByTestId('live-2-5d-plane')).toBeTruthy();
    expect(screen.getByTestId('effects-surface')).toBeTruthy();
    expect(document.querySelectorAll('[data-slot="region"]')).toHaveLength(2);
    expect(
      document.querySelector('[data-slot="region"][data-seat="p1"]')?.getAttribute('data-life'),
    ).toBe('18');
    expect(
      document.querySelector('[data-slot="region"][data-seat="p2"]')?.getAttribute('data-hand'),
    ).toBe('7');
    expect(document.querySelectorAll('[data-ghost]')).toHaveLength(0);
    expect(onPlane).toHaveBeenCalled();
  });

  it('reconciles a newer view in place by entity id', () => {
    const { rerender } = render(
      <LivePlane view={SAMPLE_GAME_VIEW} quality="standard" density="reduced" reducedMotion />,
    );
    const wrapper = document.querySelector<HTMLElement>('[data-entity-id="perm_xyz"]');
    expect(wrapper).not.toBeNull();
    expect(wrapper?.querySelector('[data-tapped="true"]')).not.toBeNull();

    const next = {
      ...SAMPLE_GAME_VIEW,
      battlefield: SAMPLE_GAME_VIEW.battlefield.map((permanent) => ({
        ...permanent,
        tapped: false,
      })),
    };
    rerender(<LivePlane view={next} quality="standard" density="reduced" reducedMotion />);

    expect(document.querySelector('[data-entity-id="perm_xyz"]')).toBe(wrapper);
    expect(wrapper?.querySelector('[data-tapped="true"]')).toBeNull();
  });
});
