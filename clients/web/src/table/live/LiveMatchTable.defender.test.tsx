/**
 * The multiplayer **defender assignment**, end to end on the shipped shell
 * (issue #457).
 *
 * #457's investigation found no defect: the per-attacker `defend_<id>` slot is
 * implemented on both sides of the wire, and "it didn't ask" is what a *single
 * living opponent* correctly looks like. What was missing was (a) coverage of
 * the whole flow above the pure `multiSelect` state machine, and (b) an
 * affordance strong enough that the question — whose answer surface is the
 * **player panels**, not the board — is not simply missed mid-combat.
 *
 * Everything asserted here is server-stated: the slots, their candidates, and
 * the attacker each slot is keyed by. Nothing in the client decides who *may*
 * block, who *must* be blocked, or who a lone opponent is — the one-opponent
 * case is proven by the **absence** of a question, which is the server declining
 * to pose one (`requirements.rs`: `defenders.len() > 1`).
 *
 * There is no browser suite and none may be added (`AGENTS.md`); the closing
 * note at the foot of this file states what therefore stays unverified.
 */
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON,
  FOUR_PLAYER_GAME_VIEW_JSON,
} from '../../game-view.fixture';
import type { TargetChoice, ValidAction } from '../../protocol';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';

const effectsMock = vi.hoisted(() => ({ persistent: [] as unknown[][] }));

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(effects: unknown[]): void {
      effectsMock.persistent.push(effects);
    }
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

registerTableTestHooks();

/** The persistent-effect set the scene last published. */
function lastPersistent(): Array<Record<string, unknown>> {
  return (effectsMock.persistent[effectsMock.persistent.length - 1] ?? []) as Array<
    Record<string, unknown>
  >;
}

describe('defender assignment with two or more living opponents (issue #457)', () => {
  beforeEach(() => {
    effectsMock.persistent = [];
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('poses one defender question per declared attacker and gates confirm on it', () => {
    const choose = seed(DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    // Declare both attackers. The attackers slot alone is satisfiable, but the
    // two `defend_` slots it puts in play are not.
    fireEvent.click(screen.getByTestId('entity-perm_1'));
    fireEvent.click(screen.getByTestId('target-perm_2'));
    expect(screen.getByTestId('decision-plaque-confirm').hasAttribute('disabled')).toBe(true);

    // Walk to the first attacker's defender slot: the question is the server's
    // own prompt, naming the creature being routed.
    fireEvent.click(screen.getByTestId('decision-plaque-advance'));
    expect(screen.getByTestId('prompt-banner').textContent).toContain(
      'Choose whom Charging Rhino attacks',
    );

    // Both living opponents are offered, and only them.
    expect(screen.getByTestId('target-player-p2')).toBeTruthy();
    expect(screen.getByTestId('target-player-p3')).toBeTruthy();
    expect(screen.queryByTestId('target-player-p1')).toBeNull();
    expect(screen.getByTestId('decision-plaque-confirm').hasAttribute('disabled')).toBe(true);

    // Answering one attacker walks straight to the next one that owes an answer
    // (`pickDefender` advances), and confirm stays gated until it has one.
    fireEvent.click(screen.getByTestId('target-player-p2'));
    expect(screen.getByTestId('decision-plaque-confirm').hasAttribute('disabled')).toBe(true);
    expect(screen.getByTestId('prompt-banner').textContent).toContain(
      'Choose whom Skyshroud Falcon attacks',
    );
    fireEvent.click(screen.getByTestId('target-player-p3'));

    fireEvent.click(screen.getByTestId('decision-plaque-confirm'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action).toEqual(expect.objectContaining({ id: 'a5', token: 'h:atk0' }));
    expect(targets).toEqual([
      { slot: 'attackers', chosen: ['perm_1', 'perm_2'] },
      { slot: 'defend_1', chosen: ['p2'] },
      { slot: 'defend_2', chosen: ['p3'] },
    ]);
  });

  it('makes the candidate panels and the routed attacker structurally unmistakable', () => {
    seed(DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('entity-perm_1'));
    fireEvent.click(screen.getByTestId('decision-plaque-advance'));

    for (const seat of ['p2', 'p3']) {
      const panel = screen.getByTestId(`target-player-${seat}`);
      // The flag the defender treatment selects on — a candidate seat cannot
      // wear the same quiet dashed ring an ordinary spell target does.
      expect(panel.getAttribute('data-defender-candidate')).toBe('true');
      // Answered vs still-asking is a state on the control, not a hue only.
      expect(panel.getAttribute('aria-pressed')).toBe('false');
      // Every panel is a real ≥ 44 px target on desktop and touch alike; the
      // plane stages the crest's hit rect at the floor and the control adopts it.
      const rect = panel.getAttribute('style') ?? '';
      expect(rect).toMatch(/width: \d/);
    }

    // The subject of the question carries its own ring in the combat hue, so
    // "which creature am I routing" never has to be inferred from the blue
    // selection ring.
    const attacker = screen.getByTestId('inspect-surface-perm_1');
    expect(attacker.getAttribute('data-routing')).toBe('true');
    expect(attacker.getAttribute('aria-label')).toBe('Charging Rhino — choosing whom it attacks');
    // The creature that is NOT being routed carries neither.
    expect(screen.getByTestId('inspect-surface-perm_2').getAttribute('data-routing')).toBeNull();

    fireEvent.click(screen.getByTestId('target-player-p2'));
    expect(screen.getByTestId('target-player-p2').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId('target-player-p3').getAttribute('aria-pressed')).toBe('false');
  });

  it('flags nothing while no defender slot is open', () => {
    // The attackers slot is a board pick; the panels must stay quiet, or the
    // treatment would mean "somewhere to click" rather than "answer here".
    seed(DECLARE_ATTACKERS_MULTIPLAYER_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('entity-perm_1'));
    expect(screen.queryByTestId('target-player-p2')).toBeNull();
    expect(screen.getByTestId('focus-seat-p2').getAttribute('data-defender-candidate')).toBeNull();
    // The board pick is live, so the creatures are candidates rather than
    // inspect surfaces — and neither of them is being routed anywhere yet.
    expect(screen.getByTestId('target-perm_1').getAttribute('data-routing')).toBeNull();
    expect(screen.getByTestId('target-perm_2').getAttribute('data-routing')).toBeNull();
  });
});

describe('defender assignment with a single living opponent (issue #457)', () => {
  beforeEach(() => {
    effectsMock.persistent = [];
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('asks no defender question and submits the attack with no defender slot', () => {
    // "It didn't ask" is correct here: the server omits `defend_*` entirely with
    // one legal defender and assigns them itself (`binding.rs`). The client
    // neither invents the question nor names the sole opponent.
    const choose = seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('entity-atk_1'));
    expect(screen.queryByTestId('decision-plaque-advance')).toBeNull();
    expect(screen.queryByTestId('target-player-p2')).toBeNull();
    expect(screen.getByTestId('decision-plaque-confirm').hasAttribute('disabled')).toBe(false);

    fireEvent.click(screen.getByTestId('decision-plaque-confirm'));
    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['atk_1'] }]);
  });
});

describe('assignment stays legible after declaration (issue #457)', () => {
  beforeEach(() => {
    effectsMock.persistent = [];
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('draws one confirmed attack path per attacker, to the defender the view names', () => {
    // The four-player split attack: `attacking_player` is the server's own
    // statement of where each attack landed, and the derived scene renders it
    // rather than re-deriving anything.
    seed(FOUR_PLAYER_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    const paths = lastPersistent().filter((effect) => effect.category === 'attack-path');
    expect(paths.length).toBeGreaterThanOrEqual(2);
    for (const path of paths) {
      expect(path.state).toBe('confirmed');
      expect(path.endpoint).toBe('player');
    }
    const routes = paths.map((path) => [
      (path.from as { ref: string }).ref,
      (path.to as { ref: string }).ref,
    ]);
    expect(routes).toContainEqual(['p1_atk_a', 'seat:p2']);
    expect(routes).toContainEqual(['p1_atk_b', 'seat:p4']);
  });

  it('rings every attacked seat’s crest, focused or not', () => {
    // "Off-focus activity is never silent": a seat being attacked reads at every
    // rung, so the split attack is legible from the receiver's own board without
    // changing focus.
    seed(FOUR_PLAYER_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    // A seat draws as a wing region's crest or — where the geometry has degraded
    // it — as a summary tile. Both publish the attacked flag, which is the point:
    // the marker can never degrade away.
    const attackedFlag = (seat: string): string | null | undefined => {
      const seatEl = document.querySelector(
        `[data-slot="crest"][data-seat="${seat}"], [data-slot="tile"][data-seat="${seat}"]`,
      );
      return seatEl?.getAttribute('data-attacked');
    };
    expect(attackedFlag('p2')).toBe('true');
    expect(attackedFlag('p4')).toBe('true');
    expect(attackedFlag('p3')).toBe('false');
  });
});

/*
 * Not verified here, and the maintainer's own to judge in a real browser: whether
 * the candidate ring, corner ticks, halo, and pulse actually draw the eye
 * mid-combat; whether the routed attacker's combat-hue ring reads as distinct
 * from the blue selection ring at battlefield scale; the real 44 px hit areas
 * under the plane's 4° tilt and 0.985 scale; the reduced-motion media query; and
 * multi-seat timing. jsdom computes no layout, no media query, and no paint.
 */
