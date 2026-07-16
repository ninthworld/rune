import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ShortcutHelp, type Binding } from './ShortcutHelp';

afterEach(cleanup);

const BINDINGS: Binding[] = [
  { id: 'enter', keys: 'Enter', description: 'Activate', available: true },
  { id: 'pass', keys: 'P', description: 'Pass priority', available: false },
];

describe('ShortcutHelp (issue #266)', () => {
  it('lists bindings and flags the ones with no matching action as unavailable', () => {
    render(<ShortcutHelp bindings={BINDINGS} onClose={vi.fn()} />);
    expect(screen.getByTestId('shortcut-help')).toBeDefined();
    // An available binding is marked; an inert one is not.
    expect(screen.getByTestId('shortcut-enter').getAttribute('data-available')).toBe('true');
    expect(screen.getByTestId('shortcut-pass').getAttribute('data-available')).toBeNull();
    expect(screen.getByTestId('shortcut-pass').textContent).toContain('Pass priority');
  });

  it('closes on the backdrop', () => {
    const onClose = vi.fn();
    render(<ShortcutHelp bindings={BINDINGS} onClose={onClose} />);
    fireEvent.click(screen.getByTestId('shortcut-help-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
