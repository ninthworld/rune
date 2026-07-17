import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { RejectionToast } from './RejectionToast';

// The toast is timer-driven (auto-dismiss), so drive time deterministically.
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe('RejectionToast (issue #265)', () => {
  it('shows nothing until a new rejection arrives', () => {
    render(<RejectionToast nonce={0} />);
    expect(screen.queryByTestId('rejection-toast')).toBeNull();
  });

  it('treats the mount value as baseline: a pre-mount rejection is stale, shows nothing', () => {
    // Mounting at a non-zero nonce means the rejection happened before this component
    // existed; it must not resurrect a stale notice.
    render(<RejectionToast nonce={5} />);
    expect(screen.queryByTestId('rejection-toast')).toBeNull();
  });

  it('appears when the nonce increments and carries a non-blaming message', () => {
    const { rerender } = render(<RejectionToast nonce={0} />);
    rerender(<RejectionToast nonce={1} />);
    const toast = screen.getByTestId('rejection-toast');
    expect(toast).toBeTruthy();
    // Informational "the game moved on" tone — never blames the player.
    expect(toast.textContent).toBe('The game moved on — that action was no longer available.');
    // Announced politely for assistive tech, and never a blocking dialog.
    expect(screen.getByRole('status')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('auto-dismisses after the duration and never blocks input', () => {
    const { rerender } = render(<RejectionToast nonce={0} durationMs={4000} />);
    rerender(<RejectionToast nonce={1} durationMs={4000} />);
    expect(screen.getByTestId('rejection-toast')).toBeTruthy();

    // Still up just before the deadline…
    act(() => void vi.advanceTimersByTime(3999));
    expect(screen.queryByTestId('rejection-toast')).toBeTruthy();
    // …and gone once it elapses, leaving no lingering element to swallow clicks.
    act(() => void vi.advanceTimersByTime(1));
    expect(screen.queryByTestId('rejection-toast')).toBeNull();
  });

  it('re-fires on a second rejection and resets the auto-dismiss window', () => {
    const { rerender } = render(<RejectionToast nonce={0} durationMs={4000} />);
    rerender(<RejectionToast nonce={1} durationMs={4000} />);
    // Part-way through the first toast's life, a second rejection arrives.
    act(() => void vi.advanceTimersByTime(3000));
    rerender(<RejectionToast nonce={2} durationMs={4000} />);
    // The window resets: past the first toast's original deadline, still visible.
    act(() => void vi.advanceTimersByTime(3000));
    expect(screen.queryByTestId('rejection-toast')).toBeTruthy();
    // A fresh full duration from the second rejection then dismisses it.
    act(() => void vi.advanceTimersByTime(1000));
    expect(screen.queryByTestId('rejection-toast')).toBeNull();
  });

  it('does not re-fire when the nonce is unchanged across a resync re-render', () => {
    const { rerender } = render(<RejectionToast nonce={0} durationMs={4000} />);
    rerender(<RejectionToast nonce={1} durationMs={4000} />);
    act(() => void vi.advanceTimersByTime(4000));
    expect(screen.queryByTestId('rejection-toast')).toBeNull();
    // A subsequent normal view (same nonce, flag cleared) must not resurrect the toast.
    rerender(<RejectionToast nonce={1} durationMs={4000} />);
    expect(screen.queryByTestId('rejection-toast')).toBeNull();
  });
});
