import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { useGameStore } from '../../store';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    trackMotion(): void {}
  },
}));

registerTableTestHooks();

describe('LiveMatchTable', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('composes the real view through scene, effects, hand, and screen chrome', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    expect(screen.getByTestId('live-match-table')).toBeTruthy();
    expect(screen.getByTestId('live-2-5d-plane')).toBeTruthy();
    expect(screen.getByTestId('effects-surface')).toBeTruthy();
    expect(screen.getByTestId('top-bar')).toBeTruthy();
    expect(screen.getByTestId('rail')).toBeTruthy();
    expect(screen.getByTestId('prompt-banner')).toBeTruthy();
    expect(screen.getByTestId('action-bar')).toBeTruthy();
    expect(screen.getByTestId('live-hand-card-c1')).toBeTruthy();
    expect(document.querySelector('canvas')).toBeNull();
  });

  it('echoes only an offered global action through the existing dock', () => {
    const choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByRole('button', { name: /^Pass/ }));
    expect(choose).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'a1', type: 'pass_priority' }),
    );
  });

  it('drops ephemeral highlights when a new authoritative view arrives', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const logReference = screen.getByTestId('log-ref-perm_xyz');
    fireEvent.click(logReference);
    expect(
      document.querySelector('[data-entity-id="perm_xyz"] [data-selected="true"]'),
    ).not.toBeNull();

    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(document.querySelector('[data-entity-id="perm_xyz"] [data-selected="true"]')).toBeNull();
  });
});
