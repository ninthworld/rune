/**
 * The phase plaque's contract (`docs/design/control-language.md` §5, issue #534).
 *
 * Two things are being guarded here. The first is that the plaque **carries the
 * shipped indicator's semantics** across the move to the cluster: the same step
 * names, the same five groups, the same twelve-step list, and the same
 * "send the full new set" answer to `set_stops`. The second is that the chevron
 * stays a **disclosure** — D4 and GAP-3 both say there is no advance action to
 * wire it to, and a plaque that ever sends a game action is the failure this file
 * exists to catch.
 */
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GameView, Phase } from '../../protocol';
import { PHASES } from '../../protocol';
import { PhasePlaque } from './PhasePlaque';
import { PHASE_GROUPS, STEP_NAME, pipStates } from './phaseSteps';

afterEach(cleanup);

/** A minimal live view carrying just the fields the plaque reads. */
function viewWith(overrides: Partial<GameView> = {}): GameView {
  return {
    you: 'p1',
    my_hand: [],
    me: { life: 20, library_size: 40 },
    opponents: [],
    battlefield: [],
    stack: [],
    graveyards: [],
    exile: [],
    phase: 'precombat_main',
    turn: 3,
    active_player: 'p1',
    seat_order: ['p1', 'p2'],
    mana_pool: [],
    valid_actions: [],
    player_names: {},
    commander_damage: [],
    ...overrides,
  };
}

describe('PhasePlaque title and ownership (§5)', () => {
  it('prints the shipped step name for the current phase', () => {
    render(<PhasePlaque view={viewWith({ phase: 'declare_attackers' })} />);
    expect(screen.getByTestId('plaque-step').textContent).toBe('Declare Attackers');
  });

  it('reads "Your turn" on the receiver\'s own turn', () => {
    render(<PhasePlaque view={viewWith({ active_player: 'p1' })} />);
    expect(screen.getByTestId('plaque-ownership').textContent).toBe('Your turn');
  });

  it('reads "Priority" when the receiver holds priority on another turn', () => {
    // The more useful of the two facts there: it says the game is waiting on you.
    render(<PhasePlaque view={viewWith({ active_player: 'p2', priority_player: 'p1' })} />);
    expect(screen.getByTestId('plaque-ownership').textContent).toBe('Priority');
  });

  it("names the active player on an opponent's turn", () => {
    render(
      <PhasePlaque
        view={viewWith({
          active_player: 'p2',
          priority_player: 'p2',
          player_names: { p2: 'Veyra' },
        })}
      />,
    );
    expect(screen.getByTestId('plaque-ownership').textContent).toBe("Veyra's turn");
  });

  it('reads "Waiting" for rule 8 while still naming the step to assistive tech', () => {
    render(<PhasePlaque view={viewWith({ phase: 'upkeep' })} waiting />);
    expect(screen.getByTestId('plaque-step').textContent).toBe('Waiting');
    // The turn has not stopped, only the player's part in it: the step name is
    // still reachable through the list behind the chevron.
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    expect(screen.getByTestId('plaque-step-upkeep').textContent).toContain('Upkeep');
  });

  it("drops the ownership line in panel 6b's compact form, keeping the pips", () => {
    render(<PhasePlaque view={viewWith()} compact />);
    expect(screen.queryByTestId('plaque-ownership')).toBeNull();
    expect(screen.getByTestId('plaque-pips')).toBeDefined();
    expect(screen.getByTestId('plaque-chevron')).toBeDefined();
  });
});

describe('step pips (§5.1, D3)', () => {
  it('renders the five shipped phase groups, not the four the panel draws', () => {
    render(<PhasePlaque view={viewWith({ phase: 'precombat_main' })} />);
    const pips = screen.getByTestId('plaque-pips');
    expect(pips.children.length).toBe(5);
    expect(PHASE_GROUPS.length).toBe(5);
  });

  it('marks exactly one pip current, with everything before it passed', () => {
    render(<PhasePlaque view={viewWith({ phase: 'declare_attackers' })} />);
    const states = Array.from(screen.getByTestId('plaque-pips').children).map((pip) =>
      pip.getAttribute('data-state'),
    );
    expect(states).toEqual(['passed', 'passed', 'current', 'upcoming', 'upcoming']);
  });

  it('hides the pip row from assistive tech; the <ol> carries the sequence', () => {
    render(<PhasePlaque view={viewWith()} />);
    expect(screen.getByTestId('plaque-pips').getAttribute('aria-hidden')).toBe('true');
  });

  it('classifies every shipped phase into exactly one group', () => {
    // The regression: a phase added to PHASES but not to PHASE_GROUPS would
    // silently render five upcoming pips and claim no progress at all.
    for (const phase of PHASES) {
      const states = pipStates(phase);
      expect(states.filter((state) => state === 'current').length, phase).toBe(1);
    }
  });
});

describe('the chevron is a disclosure, never a game action (§5.2, D4, GAP-3)', () => {
  it('opens and closes the twelve-step list, reporting its state', () => {
    render(<PhasePlaque view={viewWith()} />);
    const chevron = screen.getByTestId('plaque-chevron');
    expect(chevron.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByTestId('plaque-steps')).toBeNull();

    fireEvent.click(chevron);
    expect(chevron.getAttribute('aria-expanded')).toBe('true');
    const list = screen.getByTestId('plaque-steps');
    expect(chevron.getAttribute('aria-controls')).toBe(list.getAttribute('id'));
    expect(list.children.length).toBe(PHASES.length);

    fireEvent.click(chevron);
    expect(screen.queryByTestId('plaque-steps')).toBeNull();
  });

  it('starts closed on every fresh mount (nothing load-bearing across messages)', () => {
    const { unmount } = render(<PhasePlaque view={viewWith()} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    expect(screen.getByTestId('plaque-steps')).toBeDefined();
    unmount();

    render(<PhasePlaque view={viewWith()} />);
    expect(screen.queryByTestId('plaque-steps')).toBeNull();
  });

  it('carries an accessible name, so the glyph is never the only label', () => {
    render(<PhasePlaque view={viewWith()} />);
    expect(screen.getByRole('button', { name: 'Turn steps and stops' })).toBeDefined();
  });

  it('marks the current step in the list', () => {
    render(<PhasePlaque view={viewWith({ phase: 'end' })} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    expect(screen.getByTestId('plaque-step-end').getAttribute('aria-current')).toBe('step');
    expect(screen.getByTestId('plaque-step-upkeep').getAttribute('aria-current')).toBeNull();
  });
});

describe('per-step stops answer set_stops (ADR 0020, issue #455)', () => {
  it('sends the FULL new preference — both lists — when a step is switched on', () => {
    // The message carries the whole preference, not a delta: `view.stops` and
    // `view.own_turn_stops` are the only source of truth and the client stores
    // nothing. One click off "Auto" lands on the narrower own-turn setting.
    const onSetStops = vi.fn();
    render(<PhasePlaque view={viewWith({ stops: ['end'] })} onSetStops={onSetStops} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    fireEvent.click(screen.getByTestId('plaque-stop-upkeep'));
    expect(onSetStops).toHaveBeenCalledWith<[Phase[], Phase[]]>(['end'], ['upkeep']);
  });

  it('cycles Auto → your turn → every turn → Auto, moving the step between lists', () => {
    // Three settings because the server has three answers. Each click sends the
    // preference the *next* setting implies; a step is never on both lists.
    const onSetStops = vi.fn();
    const { rerender } = render(
      <PhasePlaque view={viewWith({ stops: [] })} onSetStops={onSetStops} />,
    );
    fireEvent.click(screen.getByTestId('plaque-chevron'));

    fireEvent.click(screen.getByTestId('plaque-stop-upkeep'));
    expect(onSetStops).toHaveBeenLastCalledWith<[Phase[], Phase[]]>([], ['upkeep']);

    rerender(
      <PhasePlaque view={viewWith({ own_turn_stops: ['upkeep'] })} onSetStops={onSetStops} />,
    );
    fireEvent.click(screen.getByTestId('plaque-stop-upkeep'));
    expect(onSetStops).toHaveBeenLastCalledWith<[Phase[], Phase[]]>(['upkeep'], []);

    rerender(<PhasePlaque view={viewWith({ stops: ['upkeep'] })} onSetStops={onSetStops} />);
    fireEvent.click(screen.getByTestId('plaque-stop-upkeep'));
    expect(onSetStops).toHaveBeenLastCalledWith<[Phase[], Phase[]]>([], []);
  });

  it('clears a seeded main-phase default by sending two empty lists', () => {
    // The default stops (#455) arrive as `own_turn_stops` the player never set, so
    // the *only* way the UI can retire them is a message that states the whole new
    // preference. Cycling the seeded step twice lands on "Auto" and does exactly that.
    const onSetStops = vi.fn();
    const seeded = viewWith({ own_turn_stops: ['precombat_main', 'postcombat_main'] });
    const { rerender } = render(<PhasePlaque view={seeded} onSetStops={onSetStops} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    expect(screen.getByTestId('plaque-stop-precombat_main').getAttribute('data-stop')).toBe('own');

    fireEvent.click(screen.getByTestId('plaque-stop-precombat_main'));
    expect(onSetStops).toHaveBeenLastCalledWith<[Phase[], Phase[]]>(
      ['precombat_main'],
      ['postcombat_main'],
    );
    rerender(
      <PhasePlaque
        view={viewWith({ stops: ['precombat_main'], own_turn_stops: ['postcombat_main'] })}
        onSetStops={onSetStops}
      />,
    );
    fireEvent.click(screen.getByTestId('plaque-stop-precombat_main'));
    expect(onSetStops).toHaveBeenLastCalledWith<[Phase[], Phase[]]>([], ['postcombat_main']);
  });

  it("reflects the server's echo, never a client-held toggle state", () => {
    const onSetStops = vi.fn();
    const { rerender } = render(
      <PhasePlaque view={viewWith({ stops: [] })} onSetStops={onSetStops} />,
    );
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    const toggle = screen.getByTestId('plaque-stop-upkeep');
    expect(toggle.getAttribute('data-stop')).toBeNull();

    // Pressing it must NOT flip the toggle by itself — only a new view may.
    fireEvent.click(toggle);
    expect(screen.getByTestId('plaque-stop-upkeep').getAttribute('data-stop')).toBeNull();

    rerender(<PhasePlaque view={viewWith({ stops: ['upkeep'] })} onSetStops={onSetStops} />);
    expect(screen.getByTestId('plaque-stop-upkeep').getAttribute('data-stop')).toBe('any');
  });

  it('offers no toggles at all when no setter is wired (the read-only board)', () => {
    render(<PhasePlaque view={viewWith()} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    const list = screen.getByTestId('plaque-steps');
    expect(within(list).queryAllByRole('button')).toEqual([]);
    // The steps themselves still disclose — the list is information, not control.
    expect(list.children.length).toBe(PHASES.length);
  });

  it('names every step it can stop at, and states the setting it is on', () => {
    // Three settings cannot ride `aria-pressed`, so the accessible name says which
    // one is current rather than leaving a screen-reader user to infer it.
    render(<PhasePlaque view={viewWith({ own_turn_stops: ['draw'] })} onSetStops={() => {}} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    for (const phase of PHASES) {
      const described = phase === 'draw' ? 'stop on your turn' : 'passed automatically';
      expect(screen.getByTestId(`plaque-stop-${phase}`).getAttribute('aria-label')).toBe(
        `Stop at ${STEP_NAME[phase]} — ${described}`,
      );
    }
  });
});

describe('the auto-passed cue (§5.2)', () => {
  it('shows the transient badge and points the chevron at the fix', () => {
    render(<PhasePlaque view={viewWith({ auto_passed: true })} />);
    expect(screen.getByTestId('plaque-auto-passed').textContent).toBe('Auto-passed');
    // The cue marks the door to setting a stop — the only real escape hatch from
    // an auto-pass chain (ADR 0020). It is display-only.
    expect(screen.getByTestId('plaque-chevron').getAttribute('data-cue')).toBe('true');
  });

  it('shows neither when the view does not report an auto-pass', () => {
    render(<PhasePlaque view={viewWith()} />);
    expect(screen.queryByTestId('plaque-auto-passed')).toBeNull();
    expect(screen.getByTestId('plaque-chevron').getAttribute('data-cue')).toBeNull();
  });
});

/**
 * Turn pacing (issue #455). ADR 0020's settle loop can advance several steps —
 * or a whole turn — between two broadcasts, and the playtest #455 records is a
 * player who "believes they're still in turn 1" while the game is at turn 2.
 * Every answer below is derived from the ONE view — the ordinal from `view.turn`,
 * the turn's path from `view.log`, and the seat's own skipped steps from
 * `view.auto_passed_steps` — so a hard reload mid-turn reproduces them exactly.
 *
 * The last two are deliberately kept apart. "The turn went through here" and
 * "the server took your priority here" are different claims, only one of which
 * the server actually makes about this seat.
 */
describe('turn pacing legibility (issue #455)', () => {
  function trailView(overrides: Partial<GameView> = {}): GameView {
    return viewWith({
      turn: 2,
      phase: 'precombat_main',
      log: [
        {
          sequence: 1,
          event: { type: 'step_changed', turn: 1, active_player: 'p2', phase: 'end' },
        },
        {
          sequence: 2,
          event: { type: 'step_changed', turn: 2, active_player: 'p1', phase: 'untap' },
        },
        {
          sequence: 3,
          event: { type: 'step_changed', turn: 2, active_player: 'p1', phase: 'draw' },
        },
        {
          sequence: 4,
          event: { type: 'step_changed', turn: 2, active_player: 'p1', phase: 'precombat_main' },
        },
      ],
      ...overrides,
    });
  }

  it('draws the turn ordinal beside the ownership sentence', () => {
    render(<PhasePlaque view={viewWith({ turn: 7 })} />);
    expect(screen.getByTestId('plaque-turn').textContent).toBe('T7');
    // §5's sentence is untouched — the ordinal is a separate chip, so a long
    // seat name can ellipsise without taking the turn number with it.
    expect(screen.getByTestId('plaque-ownership').textContent).toBe('Your turn');
  });

  it('carries the turn in the accessible sentence, including the compact form', () => {
    // 6b drops the drawn ownership line; the number a settle makes easy to lose
    // must still be answerable there, so it rides the status region's name.
    render(<PhasePlaque view={viewWith({ turn: 7, phase: 'end' })} compact />);
    expect(screen.queryByTestId('plaque-turn')).toBeNull();
    expect(screen.getByRole('status').getAttribute('aria-label')).toBe(
      'Turn 7. End Step. Your turn.',
    );
    // …and as data on the plaque itself, in both forms.
    expect(screen.getByTestId('phase-plaque').getAttribute('data-turn')).toBe('7');
  });

  it('marks the steps this turn already passed through, and only those', () => {
    render(<PhasePlaque view={trailView()} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    expect(screen.getByTestId('plaque-step-untap').getAttribute('data-passed')).toBe('true');
    expect(screen.getByTestId('plaque-step-draw').getAttribute('data-passed')).toBe('true');
    // Never recorded for this turn: not on the path, even though it sits between
    // two steps that are. Nothing is interpolated.
    expect(screen.getByTestId('plaque-step-upkeep').getAttribute('data-passed')).toBeNull();
    // The previous turn's end step belongs to the previous turn.
    expect(screen.getByTestId('plaque-step-end').getAttribute('data-passed')).toBeNull();
    // The current step is where the turn IS, not where it has been.
    expect(screen.getByTestId('plaque-step-precombat_main').getAttribute('data-passed')).toBeNull();
    expect(screen.getByTestId('plaque-step-precombat_main').getAttribute('data-current')).toBe(
      'true',
    );
  });

  it('gives each passed step a glyph and a phrase, not a hue', () => {
    // The §11 non-colour requirement, and the reduced-motion form at once: there
    // is no animation here to remove, so a reduced-motion player sees the same
    // trail a standard-motion one does.
    render(<PhasePlaque view={trailView()} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    const mark = screen.getByTestId('plaque-passed-untap');
    expect(mark.textContent).toBe('✓');
    expect(mark.getAttribute('aria-label')).toBe('already passed this turn');
  });

  it('marks nothing when the log window carries no path for this turn', () => {
    render(<PhasePlaque view={viewWith({ turn: 9, log: [] })} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    for (const phase of PHASES) {
      expect(screen.getByTestId(`plaque-step-${phase}`).getAttribute('data-passed')).toBeNull();
    }
  });

  it('marks the steps the server says it was skipped at, and only those', () => {
    // The pacing contract's second half. `auto_passed_steps` is the server's own
    // per-seat statement, so the mark is not inferred from the trail: a step the
    // turn merely went through is not a step this seat was passed at.
    render(<PhasePlaque view={trailView({ auto_passed: true, auto_passed_steps: ['untap'] })} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    expect(screen.getByTestId('plaque-step-untap').getAttribute('data-skipped')).toBe('true');
    // On the path, but the server did not say the seat was passed there.
    expect(screen.getByTestId('plaque-step-draw').getAttribute('data-passed')).toBe('true');
    expect(screen.getByTestId('plaque-step-draw').getAttribute('data-skipped')).toBeNull();
  });

  it('gives a skipped step its own glyph and phrase, replacing the trail mark', () => {
    // Two claims, two glyphs, and never both on one step — and, as with the trail,
    // no animation, so the reduced-motion reading is identical.
    render(<PhasePlaque view={trailView({ auto_passed: true, auto_passed_steps: ['untap'] })} />);
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    const mark = screen.getByTestId('plaque-skipped-untap');
    expect(mark.textContent).toBe('↷');
    expect(mark.getAttribute('aria-label')).toBe('passed for you here');
    expect(screen.queryByTestId('plaque-passed-untap')).toBeNull();
  });

  it("names the skipped steps in the badge's accessible sentence", () => {
    // The drawn word stays "Auto-passed" — the plate has no room for a list — but
    // what a screen reader is told is now the same fact the step list marks, which
    // is the non-visual half of the reduced-motion requirement.
    render(
      <PhasePlaque
        view={trailView({ auto_passed: true, auto_passed_steps: ['untap', 'upkeep', 'draw'] })}
      />,
    );
    const badge = screen.getByTestId('plaque-auto-passed');
    expect(badge.textContent).toBe('Auto-passed');
    expect(badge.getAttribute('aria-label')).toBe('Auto-passed for you at Untap, Upkeep and Draw.');
  });

  it('claims nothing beyond the boolean when the server names no steps', () => {
    // An older server sends `auto_passed` alone. The badge then says exactly what
    // it always said and the step list marks nothing — over-claiming would be
    // inventing game information, which is what #455 exists to stop losing.
    render(<PhasePlaque view={trailView({ auto_passed: true })} />);
    expect(screen.getByTestId('plaque-auto-passed').textContent).toBe('Auto-passed');
    expect(screen.getByTestId('plaque-auto-passed').getAttribute('aria-label')).toBeNull();
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    for (const phase of PHASES) {
      expect(screen.getByTestId(`plaque-step-${phase}`).getAttribute('data-skipped')).toBeNull();
    }
  });
});

/*
 * What jsdom cannot prove here, and what therefore belongs to the maintainer:
 * the drawn hexagon and its 22 px points, the gold gradient's direction, the
 * current pip's ring against the disc, the chevron's real 44 px hit area over the
 * plate's right point, and — most of all — that the disclosed step list actually
 * clamps inside the viewport when it opens upward from a plaque 28 px off the
 * bottom edge. jsdom computes no layout, no `clip-path`, and no media query, so
 * the reduced-motion glyph swap is asserted only as two elements in the DOM.
 * There is no browser suite and none may be added (`AGENTS.md`).
 */
