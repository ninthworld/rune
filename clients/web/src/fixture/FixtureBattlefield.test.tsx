import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FixtureBattlefield } from './FixtureBattlefield';

vi.mock('../table/EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../table/effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    spawn(): void {}
    trackMotion(): void {}
  },
}));

describe('FixtureBattlefield', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('mounts the isolated harness and publishes its automation hook', () => {
    render(<FixtureBattlefield />);
    expect(screen.getByTestId('fixture-battlefield')).toBeTruthy();
    expect(screen.getByTestId('fixture-plane')).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'RUNE 2.5D battlefield' })).toBeTruthy();
    expect(window.__RUNE_2_5D_FIXTURE__?.ready).toBe(true);
    expect(window.__RUNE_2_5D_FIXTURE__?.report.scenario).toBe('commander4');
  });

  it('switches scenario and frame without entering the play flow', () => {
    render(<FixtureBattlefield />);
    fireEvent.change(screen.getByLabelText('Fixture scenario'), {
      target: { value: 'phone' },
    });
    expect(screen.getByLabelText('Phone portrait: Compact four-player')).toBeTruthy();
    expect(window.__RUNE_2_5D_FIXTURE__?.report.scenario).toBe('phone');
  });
});
