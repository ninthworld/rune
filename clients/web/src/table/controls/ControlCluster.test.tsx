/**
 * The control cluster's contract (`docs/design/control-language.md` §3.3 and §4,
 * issue #534).
 *
 * The derivation itself is proved in `controlPrimary.test.ts`; these assert what
 * the *composition* has to keep true no matter which rule fired: at most one blue
 * primary, labels printed verbatim, the pressed control echoing the server's own
 * entry, `concede` never on this surface, RESPOND sending nothing, and the three
 * forms of §3.3 / §4.4 appearing when the view says so.
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GameView, StackItem, ValidAction } from '../../protocol';
import { ControlCluster } from './ControlCluster';

afterEach(cleanup);

function action(id: string, type: string, label: string, subject: string[] = []): ValidAction {
  return { id, type, label, subject };
}

/** One item on the stack — enough for §4.4's form switch to see a depth. */
const stackItem = { id: 's1', name: 'Arcane Bolt', controller: 'p2' } as unknown as StackItem;

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

const pass = action('a-pass', 'pass_priority', 'PASS PRIORITY');
const concede = action('a-concede', 'concede', 'CONCEDE');
const castBolt = action('a-cast', 'cast_spell', 'CAST SPELL', ['c1']);

/** The props every render needs; individual tests override what they exercise. */
function props(overrides: Partial<Parameters<typeof ControlCluster>[0]> = {}) {
  return {
    view: viewWith(),
    onChoose: vi.fn(),
    onOpenMenu: vi.fn(),
    ...overrides,
  };
}

describe('the one blue primary (§4.1)', () => {
  it('renders the server label verbatim and echoes back that exact action', () => {
    const onChoose = vi.fn();
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }), onChoose })} />);
    const primary = screen.getByTestId('control-primary');
    expect(primary.textContent).toContain('PASS PRIORITY');
    fireEvent.click(primary);
    // By identity: the cluster may only ever return an entry the server sent.
    expect(onChoose).toHaveBeenCalledWith(pass);
  });

  it('draws at most one primary, whatever the view offers', () => {
    const other = action('a-attack', 'declare_attackers', 'CONFIRM ATTACKERS');
    render(
      <ControlCluster {...props({ view: viewWith({ valid_actions: [pass, other, castBolt] }) })} />,
    );
    expect(screen.getAllByTestId('control-primary').length).toBe(1);
  });

  it('renders NO primary and flat secondaries for a tie (§4.2 rule 7)', () => {
    const a = action('a-1', 'declare_attackers', 'CONFIRM ATTACKERS');
    const b = action('a-2', 'declare_blockers', 'CONFIRM BLOCKERS');
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [a, b] }) })} />);
    expect(screen.queryByTestId('control-primary')).toBeNull();
    expect(screen.getByTestId('control-secondary-a-1').textContent).toContain('CONFIRM ATTACKERS');
    expect(screen.getByTestId('control-secondary-a-2')).toBeDefined();
  });

  it("routes a secondary press back as the server's own entry too", () => {
    const a = action('a-1', 'declare_attackers', 'CONFIRM ATTACKERS');
    const b = action('a-2', 'declare_blockers', 'CONFIRM BLOCKERS');
    const onChoose = vi.fn();
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [a, b] }), onChoose })} />);
    fireEvent.click(screen.getByTestId('control-secondary-a-2'));
    expect(onChoose).toHaveBeenCalledWith(b);
  });

  it('never puts concede on this surface (D9, §3.3)', () => {
    // It stays a server-offered action; the game menu is where it renders, with
    // a confirmation, so it is never one slip away from the pass button.
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [pass, concede] }) })} />);
    expect(screen.queryByTestId('control-secondary-a-concede')).toBeNull();
    expect(screen.getByTestId('control-cluster').textContent).not.toContain('CONCEDE');
  });
});

describe('the three forms (§3.3, §4.4 / D7)', () => {
  it('uses the full-width stadium primary while the stack is empty', () => {
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }) })} />);
    expect(screen.getByTestId('control-primary').getAttribute('data-variant')).toBe('primary');
    expect(screen.queryByTestId('control-respond')).toBeNull();
  });

  it('switches to the compact primary + RESPOND pair while the stack is non-empty', () => {
    render(
      <ControlCluster
        {...props({
          view: viewWith({ valid_actions: [pass], stack: [stackItem] }),
          onRespond: vi.fn(),
        })}
      />,
    );
    expect(screen.getByTestId('control-primary').getAttribute('data-variant')).toBe(
      'primaryCompact',
    );
    // The cluster itself does not move — the rail's foot IS the cluster (D7).
    expect(screen.getByTestId('control-respond')).toBeDefined();
  });

  it('degrades to panel 6b — plaque plus menu icon — when nothing is offered', () => {
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [] }) })} />);
    const cluster = screen.getByTestId('control-cluster');
    expect(cluster.getAttribute('data-compact')).toBe('true');
    expect(screen.queryByTestId('control-primary')).toBeNull();
    expect(screen.queryByTestId('control-undo')).toBeNull();
    // The two things always available survive the degrade.
    expect(screen.getByTestId('phase-plaque')).toBeDefined();
    expect(screen.getAllByTestId('control-menu').length).toBe(1);
    // Rule 8, and only rule 8, is what makes the plaque read "Waiting".
    expect(screen.getByTestId('plaque-step').textContent).toBe('Waiting');
  });

  it('does not claim "Waiting" when actions exist but none reaches the cluster', () => {
    // An entity action with nothing selected: the blue slot is empty, so the
    // cluster degrades, but the player is not idle and the plaque must not say so.
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [castBolt] }) })} />);
    expect(screen.getByTestId('control-cluster').getAttribute('data-compact')).toBe('true');
    expect(screen.getByTestId('plaque-step').textContent).toBe('Main Phase 1');
  });
});

describe('RESPOND is navigation, not an action (§4.3 / D6)', () => {
  it('sends nothing, and says what it does to assistive tech', () => {
    const onChoose = vi.fn();
    const onRespond = vi.fn();
    render(
      <ControlCluster
        {...props({
          view: viewWith({ valid_actions: [pass], stack: [stackItem] }),
          onChoose,
          onRespond,
        })}
      />,
    );
    const button = screen.getByRole('button', { name: 'Respond instead of passing' });
    fireEvent.click(button);
    expect(onRespond).toHaveBeenCalledOnce();
    // The regression that matters: no `ChooseAction` may leave this control.
    expect(onChoose).not.toHaveBeenCalled();
  });

  it('does not render when no navigation target is wired', () => {
    render(
      <ControlCluster
        {...props({ view: viewWith({ valid_actions: [pass], stack: [stackItem] }) })}
      />,
    );
    expect(screen.queryByTestId('control-respond')).toBeNull();
  });

  it('never renders beside a non-pass primary, however deep the stack', () => {
    render(
      <ControlCluster
        {...props({
          view: viewWith({ valid_actions: [castBolt], stack: [stackItem] }),
          selectedId: 'c1',
          onRespond: vi.fn(),
        })}
      />,
    );
    expect(screen.getByTestId('control-primary').textContent).toContain('CAST SPELL');
    expect(screen.queryByTestId('control-respond')).toBeNull();
  });
});

describe('the utility row (D5, §8, GAP-1)', () => {
  it('always offers the game-menu handle', () => {
    const onOpenMenu = vi.fn();
    render(
      <ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }), onOpenMenu })} />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Game menu' }));
    expect(onOpenMenu).toHaveBeenCalledOnce();
  });

  it('does NOT draw the UNDO pill in the neutral state (C8)', () => {
    // Panel 6 draws it there; GAP-1 says nothing can be undone in that state and
    // §543's own rule says an unavailable action does not render.
    render(<ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }) })} />);
    expect(screen.queryByTestId('control-undo')).toBeNull();
  });

  it('draws UNDO only when a local step can be retracted, and retracts locally', () => {
    const onChoose = vi.fn();
    const onUndo = vi.fn();
    render(
      <ControlCluster
        {...props({ view: viewWith({ valid_actions: [pass] }), onChoose, onUndo })}
      />,
    );
    fireEvent.click(screen.getByTestId('control-undo'));
    expect(onUndo).toHaveBeenCalledOnce();
    // Retracting a pick is purely local — nothing reaches the wire (§8).
    expect(onChoose).not.toHaveBeenCalled();
  });
});

describe('sessions and view-driven state', () => {
  it('empties the blue slot while a decision session owns the advance', () => {
    for (const session of ['targeting', 'multiSelect'] as const) {
      render(<ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }), session })} />);
      expect(screen.queryByTestId('control-primary')).toBeNull();
      cleanup();
    }
  });

  it('rides the deadline and the pending lock on the primary', () => {
    render(
      <ControlCluster
        {...props({ view: viewWith({ valid_actions: [pass], action_deadline: 7 }), pending: true })}
      />,
    );
    const primary = screen.getByTestId('control-primary');
    expect(primary.textContent).toContain('0:07');
    expect(primary.getAttribute('aria-busy')).toBe('true');
  });

  it('hints the shipped P binding only on the pass primary', () => {
    const { rerender } = render(
      <ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }) })} />,
    );
    expect(screen.getByTestId('control-primary').textContent).toContain('P');

    rerender(
      <ControlCluster
        {...props({ view: viewWith({ valid_actions: [castBolt] }), selectedId: 'c1' })}
      />,
    );
    expect(screen.getByTestId('control-primary').textContent).toBe('CAST SPELL');
  });

  it('carries the plaque, and its stops setter, at the foot of every form', () => {
    const onSetStops = vi.fn();
    render(
      <ControlCluster {...props({ view: viewWith({ valid_actions: [pass] }), onSetStops })} />,
    );
    fireEvent.click(screen.getByTestId('plaque-chevron'));
    fireEvent.click(screen.getByTestId('plaque-stop-upkeep'));
    // Both halves of the preference reach the setter (issue #455): one click off
    // "Auto" lands on the narrower own-turn stop, and the any-turn list rides empty.
    expect(onSetStops).toHaveBeenCalledWith([], ['upkeep']);
  });

  it('rebuilds entirely from one view: a fresh mount reproduces the same cluster', () => {
    // The reconnect/replay invariant, as a whole-markup comparison: no ephemeral
    // state may survive a mount. React's `useId` counter is per-render-root and
    // is the one legitimately unstable value, so it is normalized away.
    const stripIds = (html: string): string => html.replace(/:r[0-9a-z]+:/g, ':id:');
    const view = viewWith({ valid_actions: [pass], stack: [stackItem] });
    const first = render(<ControlCluster {...props({ view, onRespond: vi.fn() })} />);
    const before = stripIds(first.container.innerHTML);
    cleanup();

    const second = render(<ControlCluster {...props({ view, onRespond: vi.fn() })} />);
    expect(stripIds(second.container.innerHTML)).toBe(before);
  });
});

/*
 * What jsdom cannot prove here, and what therefore belongs to the maintainer:
 * that the column really lands 28 px off the viewport edge with 12 px rows, that
 * the 118 px pair and the 268 px plaque share one right edge, that a 36 px plate
 * inside a 44 px target reads as the baselines draw it, and that the cluster is
 * mounted as a sibling of the shell's regions rather than trapped inside one of
 * their stacking contexts. jsdom computes no layout. There is no browser suite
 * and none may be added (`AGENTS.md`).
 */
