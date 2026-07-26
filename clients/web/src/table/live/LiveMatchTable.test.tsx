import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  MULLIGAN_GAME_VIEW_JSON,
  SAMPLE_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
  ZONE_SELECT_GAME_VIEW_JSON,
} from '../../game-view.fixture';
import type { TargetChoice, ValidAction } from '../../protocol';
import { useGameStore } from '../../store';
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

const MANA_VIEW_JSON = JSON.stringify({
  you: 'p1',
  my_hand: [],
  opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
  battlefield: [
    {
      id: 'perm_f',
      controller: 'p1',
      owner: 'p1',
      card: { id: 'c_f', name: 'Forest', type_line: 'Basic Land — Forest' },
    },
  ],
  phase: 'precombat_main',
  valid_actions: [
    { id: 'a1', type: 'pass_priority', label: 'Pass', token: 'h:pass' },
    {
      id: 'a2',
      type: 'activate_ability',
      label: '{T}: Add {G}.',
      subject: ['perm_f'],
      mana_ability: true,
      token: 'h:tap',
    },
  ],
});

describe('LiveMatchTable', () => {
  beforeEach(() => {
    effectsMock.persistent = [];
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('composes the real view through scene, effects, hand, and screen chrome', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    expect(screen.getByTestId('live-match-table')).toBeTruthy();
    expect(screen.getByTestId('live-2-5d-plane')).toBeTruthy();
    expect(screen.getByTestId('effects-surface')).toBeTruthy();
    // ADR 0032's anatomy: no permanent top bar and no permanent rail. The
    // contextual surfaces stand in their place — the cluster is the one action
    // home, the stack stage is drawn because this fixture's stack is two deep,
    // and the activity surface replaces the rail's log column.
    expect(screen.queryByTestId('top-bar')).toBeNull();
    expect(screen.queryByTestId('rail')).toBeNull();
    // The decision surface is contextual (#567): with nothing being decided it
    // is absent, and the phase plaque in the cluster carries step and priority.
    // The permanent prompt strip that used to say so is retired with it.
    expect(screen.queryByTestId('prompt-banner')).toBeNull();
    expect(screen.queryByTestId('decision-area')).toBeNull();
    expect(screen.getByTestId('phase-plaque')).toBeTruthy();
    expect(screen.getByTestId('control-cluster')).toBeTruthy();
    expect(screen.getByTestId('stack-stage')).toBeTruthy();
    expect(screen.getByTestId('activity-surface')).toBeTruthy();
    expect(screen.getByTestId('live-hand-card-c1')).toBeTruthy();
    expect(document.querySelector('canvas')).toBeNull();
  });

  /**
   * `hand` is a full-card tier, so its face carries a rules area
   * (`docs/design/card-representation.md` §3.2) and the server's `rules_text` is
   * the only thing allowed to fill it. The face blanks that area when the prop
   * is omitted, which is silent: the card still renders, just with no rules on
   * any card in hand. Asserted on the shipped table, not on `CardFace`, because
   * the defect was the call site.
   */
  it('renders the server rules text on every card in hand', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    const card = screen.getByTestId('live-hand-card-c1');
    // The words are the server's; the `{…}` notation is drawn as symbols rather
    // than printed as braces (issue #462), each announcing its own name.
    expect(card.textContent).not.toContain('{');
    expect(card.textContent).toContain(': Add ');
    expect(
      Array.from(card.querySelectorAll('[data-symbol]')).map((el) => el.getAttribute('aria-label')),
    ).toEqual(['tap', 'green mana']);
  });

  it('echoes only an offered global action through the existing dock', () => {
    const choose = seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByRole('button', { name: /^Pass/ }));
    expect(choose).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'a1', type: 'pass_priority' }),
    );
  });

  it('drops ephemeral highlights when a new authoritative view arrives', () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const logReference = screen.getByTestId('activity-ref-perm_xyz');
    fireEvent.click(logReference);
    expect(
      document.querySelector('[data-entity-id="perm_xyz"] [data-selected="true"]'),
    ).not.toBeNull();

    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(document.querySelector('[data-entity-id="perm_xyz"] [data-selected="true"]')).toBeNull();
  });

  it('restores entity focus across a fresh view and falls back spatially if it leaves', async () => {
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const entity = document.querySelector<HTMLElement>('[data-entity="perm_xyz"]')!;
    expect(entity).not.toBeNull();
    entity.focus();

    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    await waitFor(() =>
      expect((document.activeElement as HTMLElement).dataset.entity).toBe('perm_xyz'),
    );

    const departed = JSON.parse(SAMPLE_GAME_VIEW_JSON) as Record<string, unknown>;
    departed.battlefield = [];
    act(() => useGameStore.getState().ingest(JSON.stringify(departed)));
    await waitFor(() => {
      expect(document.activeElement).toBeInstanceOf(HTMLButtonElement);
      expect(screen.getByTestId('live-match-table').contains(document.activeElement)).toBe(true);
      expect((document.activeElement as HTMLElement).dataset.entity).not.toBe('perm_xyz');
    });
  });

  it('requires deliberate activation for a flagged mana ability', () => {
    const choose = seed(MANA_VIEW_JSON);
    render(<LiveMatchTable />);
    const land = screen.getByTestId('entity-perm_f');

    // The hotspot's own accessible name is a pure-text context, so the offered
    // label is spoken rather than braced (issue #462, leak site 5).
    expect(land.getAttribute('aria-label')).toBe('Forest — playable: tap: Add green mana.');

    fireEvent.click(land);
    expect(choose).not.toHaveBeenCalled();
    expect(land.getAttribute('aria-pressed')).toBe('true');
    // The echoed control wears the server label with its notation drawn as
    // symbols, so its accessible name is the spoken form (issue #462).
    const echo = screen.getByRole('button', { name: 'tap: Add green mana.' });
    expect(echo.textContent).not.toContain('{');

    fireEvent.click(land);
    expect(choose).toHaveBeenCalledTimes(1);
    expect(choose.mock.calls[0]![0]).toEqual(expect.objectContaining({ id: 'a2' }));
  });

  it('offers the same deliberate mana path to touch and the accessible dock', () => {
    const choose = seed(MANA_VIEW_JSON);
    render(<LiveMatchTable />);
    const land = screen.getByTestId('entity-perm_f');

    fireEvent.pointerDown(land, { pointerType: 'touch', button: 0 });
    fireEvent.click(land);
    expect(choose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'tap: Add green mana.' }));
    expect(choose.mock.calls[0]![0]).toEqual(expect.objectContaining({ id: 'a2' }));
  });

  it('uses keyboard activation and spatial focus on destination controls', () => {
    const choose = seed(MANA_VIEW_JSON);
    render(<LiveMatchTable />);
    const land = screen.getByTestId('entity-perm_f');
    land.focus();

    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose).not.toHaveBeenCalled();
    expect(land.getAttribute('aria-pressed')).toBe('true');
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(choose.mock.calls[0]![0]).toEqual(expect.objectContaining({ id: 'a2' }));
  });

  it('routes a targeted hand action through the dock and submits its pick atomically', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('live-hand-card-c3'));
    fireEvent.click(screen.getByRole('button', { name: 'Cast Lightning Bolt' }));
    expect(choose).not.toHaveBeenCalled();
    expect(screen.getByTestId('decision-prompt').textContent).toContain(
      'target creature or player',
    );

    fireEvent.click(screen.getByTestId('target-perm_xyz'));
    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action).toEqual(expect.objectContaining({ id: 'a3', token: 'h:9f2c' }));
    expect(targets).toEqual([{ slot: 't0', chosen: ['perm_xyz'] }]);
  });

  it('previews a one-target path from the hand to the focused legal candidate', () => {
    seed(TARGETING_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    fireEvent.click(screen.getByTestId('live-hand-card-c3'));
    fireEvent.click(screen.getByRole('button', { name: 'Cast Lightning Bolt' }));
    fireEvent.pointerEnter(screen.getByTestId('target-perm_xyz'));

    expect(effectsMock.persistent.at(-1)).toContainEqual(
      expect.objectContaining({
        category: 'targeting-path',
        from: { ref: 'hand:p1' },
        to: { ref: 'perm_xyz' },
      }),
    );
  });

  it('targets player crests from the same server-enumerated target slot', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('live-hand-card-c3'));
    fireEvent.click(screen.getByRole('button', { name: 'Cast Lightning Bolt' }));
    fireEvent.click(screen.getByTestId('target-player-p2'));

    const [, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(targets).toEqual([{ slot: 't0', chosen: ['p2'] }]);
  });

  it('enters combat from a candidate, toggles, and confirms one atomic declaration', () => {
    const choose = seed(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('entity-atk_1'));
    expect(choose).not.toHaveBeenCalled();
    expect(screen.getByTestId('target-atk_1').getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByTestId('target-atk_2'));
    fireEvent.click(screen.getByTestId('decision-area-confirm'));

    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action).toEqual(expect.objectContaining({ id: 'a5', token: 'h:atk0' }));
    expect(targets).toEqual([{ slot: 'attackers', chosen: ['atk_1', 'atk_2'] }]);
  });

  it('keeps option/count prompts on the shared atomic decision surface', () => {
    const choose = seed(MULLIGAN_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByTestId('live-hand-card-card_a'));
    fireEvent.click(screen.getByTestId('multiselect-option-keep'));

    const [action, targets] = choose.mock.calls[0] as [ValidAction, TargetChoice[]];
    expect(action).toEqual(expect.objectContaining({ token: 'h:mull' }));
    expect(targets).toEqual([
      { slot: 'decision', chosen: ['keep'] },
      { slot: 'bottom', chosen: ['card_a'] },
    ]);
  });

  it('presents a forced decision with the view and keeps it there (issue #451)', () => {
    // The mulligan used to hide behind a dock button that every server frame
    // closed — including the resync answering a rejection and the fresh hand a
    // mulligan deals — so the player had to rediscover it after each click.
    seed(MULLIGAN_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    expect(screen.getByTestId('multiselect-options')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Keep or mulligan' })).toBeNull();
    // Nothing to fall back to, so no cancel that would re-open itself.
    expect(screen.queryByTestId('multiselect-cancel')).toBeNull();

    // A fresh frame (a new hand, or the resync after a rejection) re-presents it.
    act(() => useGameStore.getState().ingest(MULLIGAN_GAME_VIEW_JSON));
    expect(screen.getByTestId('multiselect-options')).toBeTruthy();
    expect(screen.getByTestId('live-hand-card-card_a').getAttribute('aria-pressed')).toBe('false');

    // Escape cannot dismiss the only thing the server is waiting on.
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.getByTestId('multiselect-options')).toBeTruthy();
  });

  it('gates Keep on the exact bottoming count, picked on the scene (issue #451)', () => {
    const choose = seed(MULLIGAN_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    const keep = screen.getByTestId('multiselect-option-keep') as HTMLButtonElement;
    const another = screen.getByTestId('multiselect-option-mulligan') as HTMLButtonElement;

    // One card is owed and none is picked: keep is closed, take-another is open.
    expect(keep.disabled).toBe(true);
    expect(another.disabled).toBe(false);

    // The surface stands above the control cluster, clear of the hand it is
    // asking about, and takes no pointer events outside its own plate — so it
    // cannot swallow the clicks that answer it (the retired scrim did).
    expect(screen.getByTestId('decision-area').dataset.pointerThrough).toBe('true');

    fireEvent.click(screen.getByTestId('live-hand-card-card_a'));
    expect(screen.getByTestId('live-hand-card-card_a').getAttribute('aria-pressed')).toBe('true');
    expect(keep.disabled).toBe(false);

    // Over the owed count neither choice is offered — the server would reject both.
    fireEvent.click(screen.getByTestId('live-hand-card-card_b'));
    expect(keep.disabled).toBe(true);
    expect(another.disabled).toBe(true);
    expect(choose).not.toHaveBeenCalled();
  });

  it('hosts a non-board zone pick as rows on the same one surface', () => {
    // A decision whose candidates are not on the board — a graveyard return —
    // used to open a second surface (the sheet) with its own copy of the
    // question. Now the rows ride the same area as the question and the
    // controls, so there is one place to read it and one place to answer it.
    seed(ZONE_SELECT_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    fireEvent.click(screen.getByRole('button', { name: 'Return a card to hand' }));
    const area = screen.getByTestId('decision-area');
    expect(area.contains(screen.getByTestId('prompt-surface'))).toBe(true);
    expect(screen.queryByTestId('decision-sheet')).toBeNull();
    expect(document.querySelectorAll('[data-decision-prompt]')).toHaveLength(1);
  });

  it('clears an in-progress target session on the next complete view', () => {
    const choose = seed(TARGETING_GAME_VIEW_JSON);
    render(<LiveMatchTable />);
    fireEvent.click(screen.getByTestId('live-hand-card-c3'));
    fireEvent.click(screen.getByRole('button', { name: 'Cast Lightning Bolt' }));
    expect(screen.getByTestId('target-perm_xyz')).toBeTruthy();

    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('target-perm_xyz')).toBeNull();
    expect(screen.queryByTestId('decision-prompt')).toBeNull();
    expect(screen.queryByTestId('decision-area')).toBeNull();
    expect(choose).not.toHaveBeenCalled();
  });

  it('keeps drag-to-play as a pointer enhancement over the universal path', () => {
    const raw = JSON.parse(SAMPLE_GAME_VIEW_JSON) as Record<string, unknown>;
    raw.valid_actions = [
      {
        id: 'play-c1',
        type: 'cast_spell',
        label: 'Cast Llanowar Elves',
        subject: ['c1'],
        token: 'h:play',
      },
    ];
    const choose = seed(JSON.stringify(raw));
    render(<LiveMatchTable />);
    const hand = screen.getByTestId('live-hand-card-c1');
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: vi.fn(() => screen.getByTestId('drop-board')),
    });

    fireEvent.pointerDown(hand, { button: 0, clientX: 40, clientY: 500 });
    fireEvent.pointerMove(window, { clientX: 80, clientY: 420 });
    expect(screen.getByTestId('drag-ghost')).toBeTruthy();
    expect(screen.getByTestId('drop-board')).toBeTruthy();
    fireEvent.pointerUp(window, { clientX: 120, clientY: 240 });

    expect(choose.mock.calls[0]![0]).toEqual(expect.objectContaining({ id: 'play-c1' }));
  });

  it('does not play an untargeted hand card onto an opponent region', () => {
    const raw = JSON.parse(SAMPLE_GAME_VIEW_JSON) as Record<string, unknown>;
    raw.valid_actions = [
      {
        id: 'play-c1',
        type: 'cast_spell',
        label: 'Cast Llanowar Elves',
        subject: ['c1'],
        token: 'h:play',
      },
    ];
    const choose = seed(JSON.stringify(raw));
    render(<LiveMatchTable />);
    const hand = screen.getByTestId('live-hand-card-c1');
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: vi.fn(() => screen.getByTestId('focus-seat-p2')),
    });

    fireEvent.pointerDown(hand, { button: 0, clientX: 40, clientY: 500 });
    fireEvent.pointerMove(window, { clientX: 80, clientY: 420 });
    fireEvent.pointerUp(window, { clientX: 120, clientY: 240 });

    expect(choose).not.toHaveBeenCalled();
  });

  it('discards an armed drag when a newer authoritative view arrives', () => {
    const raw = JSON.parse(SAMPLE_GAME_VIEW_JSON) as Record<string, unknown>;
    raw.valid_actions = [
      {
        id: 'play-c1',
        type: 'cast_spell',
        label: 'Cast Llanowar Elves',
        subject: ['c1'],
        token: 'h:stale',
      },
    ];
    const choose = seed(JSON.stringify(raw));
    render(<LiveMatchTable />);
    const hand = screen.getByTestId('live-hand-card-c1');
    Object.defineProperty(document, 'elementFromPoint', {
      configurable: true,
      value: vi.fn(() => screen.getByTestId('drop-board')),
    });

    fireEvent.pointerDown(hand, { button: 0, clientX: 40, clientY: 500 });
    fireEvent.pointerMove(window, { clientX: 80, clientY: 420 });
    expect(screen.getByTestId('drag-ghost')).toBeTruthy();

    act(() => useGameStore.getState().ingest(SAMPLE_GAME_VIEW_JSON));
    expect(screen.queryByTestId('drag-ghost')).toBeNull();
    fireEvent.pointerUp(window, { clientX: 120, clientY: 240 });

    expect(choose).not.toHaveBeenCalled();
  });
});
