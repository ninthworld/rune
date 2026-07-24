import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { PresentationSettings } from './PresentationSettings';
import {
  getPresentationSnapshot,
  resetPresentationSettings,
  setQuality,
} from './settings/presentationSettings';

afterEach(() => {
  cleanup();
  resetPresentationSettings();
  localStorage.clear();
});

describe('PresentationSettings (issue #505)', () => {
  it('offers quality, density, and motion with the current values checked', () => {
    render(<PresentationSettings onClose={vi.fn()} />);
    // Defaults: auto-detected quality (Standard in jsdom), reduced density, system motion.
    expect(screen.getByTestId('density-reduced').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('motion-system').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('quality-lite')).toBeTruthy();
    expect(screen.getByTestId('quality-high')).toBeTruthy();
  });

  it('shows the auto-detected level rather than applying it silently', () => {
    render(<PresentationSettings onClose={vi.fn()} />);
    expect(screen.getByTestId('quality-autodetected')).toBeTruthy();
  });

  it('applies a quality choice immediately and persists it (round-trip)', () => {
    render(<PresentationSettings onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('quality-high'));
    expect(getPresentationSnapshot().quality).toBe('high');
    expect(screen.getByTestId('quality-high').getAttribute('aria-checked')).toBe('true');
    // Choosing clears the auto-detected note (it is now a user override).
    expect(screen.queryByTestId('quality-autodetected')).toBeNull();
    // The override sticks across a fresh store (device-local persistence).
    resetPresentationSettings();
    expect(getPresentationSnapshot().quality).toBe('high');
  });

  it('reflects an external settings change without a remount (live subscription)', () => {
    render(<PresentationSettings onClose={vi.fn()} />);
    expect(screen.getByTestId('quality-lite').getAttribute('aria-checked')).toBe('false');
    fireEvent.click(screen.getByTestId('motion-reduced'));
    expect(getPresentationSnapshot().motion).toBe('reduced');
    // A store change from elsewhere re-renders the open surface.
    act(() => setQuality('lite'));
    expect(screen.getByTestId('quality-lite').getAttribute('aria-checked')).toBe('true');
  });

  it('closes on the backdrop, Done, and Escape', () => {
    const onClose = vi.fn();
    const { rerender } = render(<PresentationSettings onClose={onClose} />);
    fireEvent.click(screen.getByTestId('presentation-settings-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(1);
    rerender(<PresentationSettings onClose={onClose} />);
    fireEvent.click(screen.getByTestId('presentation-settings-close'));
    expect(onClose).toHaveBeenCalledTimes(2);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(3);
  });
});
