import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW_JSON } from './game-view.fixture';

vi.mock('./table/EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('./table/effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
  vi.resetModules();
});

describe('App live 2.5D match gate', () => {
  it('selects the production 2.5D composition only when explicitly enabled', async () => {
    vi.stubEnv('VITE_RUNE_2_5D_MATCH', 'true');
    vi.resetModules();
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
    const { useGameStore } = await import('./store');
    useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON);
    const { App } = await import('./App');

    render(<App />);

    expect(screen.getByTestId('live-match-table')).toBeTruthy();
    expect(screen.getByTestId('live-2-5d-plane')).toBeTruthy();
  });
});
