import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { CardView } from '../protocol';
import { ZoneBrowser } from './ZoneBrowser';

afterEach(cleanup);

const CARDS: CardView[] = [
  { id: 'g1', name: 'Llanowar Elves', type_line: 'Creature — Elf Druid' },
  { id: 'g2', name: 'Giant Growth', type_line: 'Instant' },
  { id: 'g3', name: 'Forest', type_line: 'Basic Land — Forest' },
];

describe('ZoneBrowser (issue #262)', () => {
  it('lists the pile in wire order with a titled count', () => {
    render(
      <ZoneBrowser title="p1 — Graveyard" cards={CARDS} onInspect={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByTestId('zone-browser-title').textContent).toBe('p1 — Graveyard (3)');
    const rows = within(screen.getByTestId('zone-browser')).getAllByRole('listitem');
    // Wire order preserved (top of the pile is last on the wire).
    expect(rows[0].textContent).toContain('Llanowar Elves');
    expect(rows[2].textContent).toContain('Forest');
  });

  it('opens inspect for a card and reports its id', () => {
    const onInspect = vi.fn();
    render(
      <ZoneBrowser title="p1 — Graveyard" cards={CARDS} onInspect={onInspect} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId('browser-card-g2'));
    expect(onInspect).toHaveBeenCalledWith('g2');
  });

  it('renders an empty-zone placeholder', () => {
    render(<ZoneBrowser title="p1 — Exile" cards={[]} onInspect={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByTestId('zone-browser-title').textContent).toBe('p1 — Exile (0)');
    expect(screen.getByTestId('zone-browser-empty')).toBeDefined();
    expect(screen.queryByTestId('browser-card-g1')).toBeNull();
  });

  it('closes on the close control and on a backdrop click', () => {
    const onClose = vi.fn();
    render(<ZoneBrowser title="p1 — Exile" cards={[]} onInspect={vi.fn()} onClose={onClose} />);
    fireEvent.click(screen.getByTestId('zone-browser-close'));
    fireEvent.click(screen.getByTestId('zone-browser-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
