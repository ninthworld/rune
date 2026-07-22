/**
 * Phone-portrait summary tiles + tap-to-focus (issue #400).
 *
 * On a 3–4 player table at phone-portrait geometry the opponents collapse to
 * crest/name/counts **summary tiles**; activating one expands that battlefield in
 * place, collapsing restores the tiles. These tests drive the real `<Table />` at a
 * measured 390×844 viewport (the pure layout resolves the tile composition from the
 * geometry, jsdom needs no real layout) and assert the composition, the pointer- and
 * keyboard-operated expand/collapse with its focus-order survival, the commander
 * chrome the compact composition still owes, that an offered board action is never
 * hidden behind a collapsed tile, and the `prefers-reduced-motion` snap.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useGameStore } from '../store';
import { Table } from './Table';

/** Install a measured phone-portrait viewport + a `matchMedia` (coarse pointer, and
 * a togglable reduced-motion answer) that jsdom otherwise lacks. */
function setPhoneEnvironment(reducedMotion = false): void {
  Object.defineProperty(window, 'innerWidth', { value: 390, configurable: true, writable: true });
  Object.defineProperty(window, 'innerHeight', { value: 844, configurable: true, writable: true });
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: query.includes('coarse') ? true : query.includes('reduce') ? reducedMotion : false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}

interface RawView {
  [key: string]: unknown;
}

/** A 4-player commander frame (receiver + three opponents) — a command zone for the
 * receiver and one opponent, a tax-only opponent, and commander damage dealt around
 * the table. Only the `pass` action is offered, so no board is force-expanded. */
function commanderFourPlayer(extra: RawView = {}): string {
  return JSON.stringify({
    you: 'p1',
    my_hand: [],
    me: { life: 40, library_size: 90 },
    opponents: [
      { player_id: 'p2', hand_size: 4, life: 38, library_size: 88 },
      { player_id: 'p3', hand_size: 2, life: 31, library_size: 80 },
      { player_id: 'p4', hand_size: 7, life: 22, library_size: 70 },
    ],
    command: [
      {
        player_id: 'p1',
        cards: [{ id: 'c1', name: 'My Commander', type_line: 'Legendary Creature' }],
      },
      {
        player_id: 'p2',
        cards: [{ id: 'c2', name: 'Rival Commander', type_line: 'Legendary Creature' }],
      },
    ],
    commander_tax: [{ commander: 'p3', tax: 2 }],
    commander_damage: [
      { commander: 'p2', damaged: 'p1', amount: 7 },
      { commander: 'p3', damaged: 'p2', amount: 12 },
    ],
    battlefield: [
      {
        id: 'p3_crt',
        controller: 'p3',
        owner: 'p3',
        card: {
          id: 'p3_crt',
          name: 'Grizzly Bears',
          type_line: 'Creature — Bear',
          power: '2',
          toughness: '2',
        },
      },
    ],
    seat_order: ['p1', 'p2', 'p3', 'p4'],
    phase: 'precombat_main',
    turn: 5,
    active_player: 'p2',
    priority_player: 'p1',
    valid_actions: [{ id: 'pass', type: 'pass_priority', label: 'Pass' }],
    ...extra,
  });
}

function seed(json: string): void {
  useGameStore.getState().ingest(json);
  useGameStore.setState({ choose: vi.fn() });
}

const originalMatchMedia = window.matchMedia;

afterEach(() => {
  cleanup();
  useGameStore.setState({ view: null });
  window.matchMedia = originalMatchMedia;
});

describe('summary tiles at phone-portrait (issue #400)', () => {
  beforeEach(() => setPhoneEnvironment());

  it('collapses every opponent to a named, counts-bearing tile; keeps the receiver full', () => {
    seed(commanderFourPlayer());
    render(<Table />);
    // Each opponent is a tap-to-focus tile; the receiver keeps a full panel.
    for (const id of ['p2', 'p3', 'p4']) {
      const tile = screen.getByTestId(`tile-focus-${id}`);
      expect(tile.tagName).toBe('BUTTON');
      expect(tile.getAttribute('aria-expanded')).toBe('false');
      // The tile is an interactive element with an accessible name.
      expect(tile.getAttribute('aria-label')).toContain('life');
    }
    // Tile content: crest (life), name, hand + library counts.
    expect(screen.getByTestId('hud-life-p2').textContent).toBe('38');
    expect(screen.getByTestId('hud-name-p3').textContent).toContain('p3');
    expect(screen.getByTestId('hud-hand-p4').textContent).toContain('7');
    expect(screen.getByTestId('tile-library-p2').textContent).toContain('88');
    // The receiver is not a summary tile.
    expect(screen.queryByTestId('tile-focus-p1')).toBeNull();
  });

  it('shows commander tallies and the command zone in the compact composition', () => {
    seed(commanderFourPlayer());
    render(<Table />);
    // Command-zone counts on the tiles: p2 has a commander in the zone, p3 owes tax.
    expect(screen.getByTestId('tile-command-p2').textContent).toContain('1');
    expect(screen.getByTestId('tile-command-p3').textContent).toContain('0');
    // Commander damage tallies where present (p2 has taken 12 from p3's commander).
    expect(screen.getByTestId('cmd-damage-p2')).toBeDefined();
    // The receiver's own command zone + tally stay visible in the compact shell.
    expect(screen.getByTestId('command-pile-p1')).toBeDefined();
    expect(screen.getByTestId('cmd-damage-p1')).toBeDefined();
  });

  it('expands a tile in place on tap and collapses it again (pointer)', () => {
    seed(commanderFourPlayer());
    render(<Table />);
    fireEvent.click(screen.getByTestId('tile-focus-p3'));
    // p3 is now expanded: its tile is gone, a collapse control appears, and its
    // battlefield card is rendered (reachable). The others stay tiled.
    expect(screen.queryByTestId('tile-focus-p3')).toBeNull();
    const collapse = screen.getByTestId('tile-collapse-p3');
    expect(collapse.getAttribute('aria-expanded')).toBe('true');
    expect(collapse.getAttribute('aria-label')).toContain('Collapse');
    expect(screen.getByTestId('inspect-surface-p3_crt')).toBeDefined();
    expect(screen.getByTestId('tile-focus-p2')).toBeDefined();
    // Collapsing restores the tile.
    fireEvent.click(collapse);
    expect(screen.getByTestId('tile-focus-p3')).toBeDefined();
    expect(screen.queryByTestId('tile-collapse-p3')).toBeNull();
  });

  it('expands and collapses by keyboard, and the focus order survives the swap', () => {
    seed(commanderFourPlayer());
    render(<Table />);
    // Focus the tile and activate it with Enter (the shell's select/confirm verb).
    const tile = screen.getByTestId('tile-focus-p3');
    tile.focus();
    fireEvent.keyDown(window, { key: 'Enter' });
    // Focus lands on the newly-mounted collapse control (order survives expansion).
    const collapse = screen.getByTestId('tile-collapse-p3');
    expect(document.activeElement).toBe(collapse);
    // Space collapses it again; focus returns to the restored tile.
    fireEvent.keyDown(window, { key: ' ' });
    const restored = screen.getByTestId('tile-focus-p3');
    expect(document.activeElement).toBe(restored);
  });

  it('auto-expands the opponent whose board an offered action targets — never hidden', () => {
    // A spell in hand whose only target is p3's creature: the requirement candidate
    // lives on p3's board, so p3 is expanded automatically and its card is reachable.
    seed(
      commanderFourPlayer({
        my_hand: [{ id: 'bolt', name: 'Lightning Bolt', type_line: 'Instant' }],
        valid_actions: [
          { id: 'pass', type: 'pass_priority', label: 'Pass' },
          {
            id: 'cast_bolt',
            type: 'cast_spell',
            label: 'Cast Lightning Bolt',
            subject: ['bolt'],
            requirements: [{ slot: 0, prompt: 'Choose target', candidates: ['p3_crt'] }],
          },
        ],
      }),
    );
    render(<Table />);
    // p3 is expanded (no focus tile), its creature is on the canvas and reachable,
    // and — being pinned by the decision — it shows no manual collapse control.
    expect(screen.queryByTestId('tile-focus-p3')).toBeNull();
    expect(screen.getByTestId('inspect-surface-p3_crt')).toBeDefined();
    expect(screen.queryByTestId('tile-collapse-p3')).toBeNull();
    // The other opponents stay collapsed as tiles.
    expect(screen.getByTestId('tile-focus-p2')).toBeDefined();
    expect(screen.getByTestId('tile-focus-p4')).toBeDefined();
  });

  it('a compact duel keeps both battlefields in full — no summary tiles', () => {
    seed(
      JSON.stringify({
        you: 'p1',
        my_hand: [],
        me: { life: 20, library_size: 40 },
        opponents: [{ player_id: 'p2', hand_size: 3, life: 20, library_size: 40 }],
        battlefield: [],
        seat_order: ['p1', 'p2'],
        phase: 'precombat_main',
        valid_actions: [{ id: 'pass', type: 'pass_priority', label: 'Pass' }],
      }),
    );
    render(<Table />);
    expect(screen.queryByTestId('tile-focus-p2')).toBeNull();
  });
});

describe('summary tiles honor prefers-reduced-motion (issue #400)', () => {
  it('snaps the expand/collapse transition when reduced motion is requested', () => {
    setPhoneEnvironment(true);
    seed(commanderFourPlayer());
    render(<Table />);
    expect(screen.getByTestId('panel-chrome').getAttribute('data-animate')).toBe('false');
    expect(screen.getByTestId('tile-focus-p2').getAttribute('data-animate')).toBe('false');
  });

  it('animates by default when reduced motion is not requested', () => {
    setPhoneEnvironment(false);
    seed(commanderFourPlayer());
    render(<Table />);
    expect(screen.getByTestId('panel-chrome').getAttribute('data-animate')).toBe('true');
    expect(screen.getByTestId('tile-focus-p2').getAttribute('data-animate')).toBe('true');
  });
});
