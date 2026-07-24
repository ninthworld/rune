import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { registerTableTestHooks, seed } from '../table-test-support';
import { resetPresentationSettings, setMotion, setQuality } from '../settings/presentationSettings';
import { LiveMatchTable } from './LiveMatchTable';

/** Capture the presentation props LiveMatchTable pushes into the scene layer. */
const captured = vi.hoisted(() => ({
  props: [] as { quality: string; density: string; reducedMotion: boolean }[],
}));

vi.mock('./LivePlane', () => ({
  LivePlane: (props: { quality: string; density: string; reducedMotion: boolean }) => {
    captured.props.push({
      quality: props.quality,
      density: props.density,
      reducedMotion: props.reducedMotion,
    });
    return <div data-testid="live-2-5d-plane" />;
  },
}));

registerTableTestHooks();

afterEach(() => {
  resetPresentationSettings();
  localStorage.clear();
});

beforeEach(() => {
  captured.props = [];
});

/** The last set of props the (mocked) scene layer received. */
function latest() {
  return captured.props[captured.props.length - 1]!;
}

describe('LiveMatchTable presentation wiring (issue #505)', () => {
  it('feeds the device-local settings straight into the scene layer', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    // jsdom has no reduced-motion media query, and default motion is `system`.
    expect(latest().reducedMotion).toBe(false);
    expect(latest().density).toBe('reduced');
  });

  it('applies a quality change immediately, without a reload', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    act(() => setQuality('high'));
    expect(latest().quality).toBe('high');
  });

  it('collapses motion when the user selects reduced motion', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    expect(latest().reducedMotion).toBe(false);
    act(() => setMotion('reduced'));
    expect(latest().reducedMotion).toBe(true);
  });

  it('opens the display settings surface from the in-match game menu', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    fireEvent.click(screen.getByTestId('game-menu-button'));
    fireEvent.click(screen.getByTestId('menu-settings'));
    expect(screen.getByTestId('presentation-settings')).toBeTruthy();
  });
});
