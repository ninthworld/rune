/**
 * The sound and haptics controls inside the display settings overlay
 * (issue #507): every control round-trips through the device-local store, and
 * the defaults the surface opens with are the silent, un-buzzing ones.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { PresentationSettings } from '../PresentationSettings';
import { AUDIO_CUE_CATEGORIES } from '../audio/types';
import { getAudioSnapshot, resetAudioSettings } from './audioSettings';

afterEach(() => {
  cleanup();
  resetAudioSettings();
  localStorage.clear();
});

/** The `aria-checked` state of one switch. */
function checked(testId: string): string | null {
  return screen.getByTestId(testId).getAttribute('aria-checked');
}

describe('PresentationSettings — sound and haptics (issue #507)', () => {
  it('opens muted with haptics off', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    expect(checked('audio-muted')).toBe('false');
    expect(checked('audio-haptics')).toBe('false');
  });

  it('says plainly that no sound assets ship yet', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    expect(screen.getByTestId('audio-asset-note').textContent).toMatch(/no sound assets/i);
  });

  it('round-trips the master mute through the device-local store', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('audio-muted'));
    expect(getAudioSnapshot().muted).toBe(false);
    expect(checked('audio-muted')).toBe('true');

    // A fresh store re-read from storage keeps the choice.
    resetAudioSettings();
    expect(getAudioSnapshot().muted).toBe(false);
  });

  it('round-trips the master volume', () => {
    render(<PresentationSettings onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('audio-muted'));

    fireEvent.change(screen.getByTestId('audio-volume'), { target: { value: '30' } });

    expect(getAudioSnapshot().volume).toBeCloseTo(0.3);
    expect(screen.getByTestId('audio-volume-readout').textContent).toBe('30%');
    resetAudioSettings();
    expect(getAudioSnapshot().volume).toBeCloseTo(0.3);
  });

  it('disables the volume control while muted', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    expect((screen.getByTestId('audio-volume') as HTMLInputElement).disabled).toBe(true);
    fireEvent.click(screen.getByTestId('audio-muted'));
    expect((screen.getByTestId('audio-volume') as HTMLInputElement).disabled).toBe(false);
  });

  it('offers a switch for every taxonomy category, all audible by default', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    for (const category of AUDIO_CUE_CATEGORIES) {
      expect(checked(`audio-category-${category}`)).toBe('true');
    }
  });

  it('round-trips a per-category mute without touching its neighbours', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('audio-category-priority'));

    expect([...getAudioSnapshot().mutedCategories]).toEqual(['priority']);
    expect(checked('audio-category-priority')).toBe('false');
    expect(checked('audio-category-draw')).toBe('true');

    resetAudioSettings();
    expect([...getAudioSnapshot().mutedCategories]).toEqual(['priority']);
  });

  it('round-trips the haptics opt-in', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('audio-haptics'));

    expect(getAudioSnapshot().haptics).toBe(true);
    resetAudioSettings();
    expect(getAudioSnapshot().haptics).toBe(true);
  });

  it('says so when the device reports no vibration support', () => {
    // jsdom exposes no `navigator.vibrate`, which is exactly the unsupported case.
    render(<PresentationSettings onClose={vi.fn()} />);

    expect(screen.getByTestId('haptics-unsupported')).toBeTruthy();
  });

  it('keeps sound independent of the motion preference', () => {
    render(<PresentationSettings onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('motion-reduced'));
    fireEvent.click(screen.getByTestId('audio-muted'));

    // Reduced motion is an animation request, never a request for silence.
    expect(getAudioSnapshot().muted).toBe(false);
  });
});
