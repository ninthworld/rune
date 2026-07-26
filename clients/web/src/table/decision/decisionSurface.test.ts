/**
 * The one decision derivation (issue #567).
 *
 * These are the rules the three-surface arrangement got wrong. A decision used
 * to be assembled once for `PromptStrip`, again for `DecisionSheet`, and a third
 * time for `DecisionPlaque` — which is how a forced mulligan ended up drawn three
 * times with the third copy unable to answer. There is now one function, so the
 * question, the rows, the choices, and the controls are proven together here and
 * the shell's own tests only have to prove there is one surface drawing them.
 *
 * Nothing below computes legality: every expectation is a read of the server's
 * own `requirements`/`prompts` through the shipped session machine.
 */
import { describe, expect, it } from 'vitest';
import {
  BOTTOM_GAME_VIEW_JSON,
  DECLARE_ATTACKERS_GAME_VIEW_JSON,
  MULLIGAN_GAME_VIEW_JSON,
  TARGETING_GAME_VIEW_JSON,
  ZONE_SELECT_GAME_VIEW_JSON,
} from '../../game-view.fixture';
import type { GameView, ValidAction } from '../../protocol';
import { normalizeGameView } from '../../wire';
import { beginMultiSelect, setActiveNumber, toggle } from '../multiSelect';
import { beginTargeting } from '../targeting';
import { forcedDecision } from '../tableView';
import { deriveDecision } from './decisionSurface';

function viewOf(json: string): GameView {
  return normalizeGameView(JSON.parse(json) as Record<string, unknown>);
}

/** The one non-concede action a decision fixture offers. */
function decisionActionOf(view: GameView) {
  return view.valid_actions.find((action) => action.type !== 'concede')!;
}

const IDLE = { targeting: null, multiSelect: null, forced: null };

describe('deriveDecision', () => {
  it('is absent when nothing is being decided', () => {
    const view = viewOf(MULLIGAN_GAME_VIEW_JSON);
    const { surface, staging } = deriveDecision(view, IDLE);

    // ADR 0032: contextual surfaces appear when relevant and are otherwise
    // absent. There is no neutral form of the decision area to keep in sync
    // with the phase plaque, which is what makes "asked once" checkable.
    expect(surface).toBeNull();
    expect(staging.selecting).toBe(false);
    expect(staging.candidates).toEqual([]);
  });

  it('states a forced mulligan once, with its named choices and no cancel', () => {
    const view = viewOf(MULLIGAN_GAME_VIEW_JSON);
    const forced = forcedDecision(view);
    const session = beginMultiSelect(forced!);
    const { surface } = deriveDecision(view, { targeting: null, multiSelect: session, forced });

    expect(surface).not.toBeNull();
    expect(surface!.title).toBe('Keep or mulligan');
    // The sentence is the SERVER's active slot prompt, stated once.
    expect(surface!.prompt).toBe('Put 1 card(s) on the bottom of your library');
    expect(surface!.choices?.map((choice) => choice.id)).toEqual(['keep', 'mulligan']);
    // §8/D19: the view forces this, so there is no neutral state to cancel to.
    expect(surface!.cancel).toBe(false);
    // Answering IS pressing a named choice, so no second confirm competes with it.
    expect(surface!.confirm).toBe(false);
  });

  it('closes a named choice only with the server’s own stated reason', () => {
    const view = viewOf(MULLIGAN_GAME_VIEW_JSON);
    const forced = forcedDecision(view);
    const owing = beginMultiSelect(forced!);
    const sessions = { targeting: null, multiSelect: owing, forced };

    const closed = deriveDecision(view, sessions).surface!.choices!;
    const keep = closed.find((choice) => choice.id === 'keep')!;
    const mulligan = closed.find((choice) => choice.id === 'mulligan')!;
    // `keep` names the `bottom` slot in its own `requires`; the reason printed
    // is that slot's prompt, verbatim. D14/GAP-4: no client-invented reason may
    // grey a control, which is why the type is a string.
    expect(keep.disabledReason).toBe('needs: Put 1 card(s) on the bottom of your library');
    expect(mulligan.disabledReason).toBeUndefined();

    const answered = deriveDecision(view, {
      ...sessions,
      multiSelect: toggle(owing, 'card_a'),
    }).surface!.choices!;
    expect(answered.find((choice) => choice.id === 'keep')!.disabledReason).toBeUndefined();
  });

  it('lights the hand rather than drawing rows when the candidates are on the board', () => {
    // Mulligan bottoming is picked ON THE CARDS. A surface that listed them
    // would be asking the same question a second way.
    const view = viewOf(MULLIGAN_GAME_VIEW_JSON);
    const forced = forcedDecision(view);
    const { surface, staging } = deriveDecision(view, {
      targeting: null,
      multiSelect: beginMultiSelect(forced!),
      forced,
    });

    expect(surface!.rows).toBeUndefined();
    expect(staging.candidates).toEqual(['card_a', 'card_b']);
  });

  it('draws rows, and lights nothing, for a zone the board does not show', () => {
    const view = viewOf(ZONE_SELECT_GAME_VIEW_JSON);
    const action = decisionActionOf(view);
    const { surface, staging } = deriveDecision(view, {
      targeting: null,
      multiSelect: beginMultiSelect(action),
      forced: null,
    });

    expect(surface!.rows?.mode).toBe('select');
    expect(surface!.rows?.zone).toBe('graveyard');
    expect(surface!.rows?.items.length).toBeGreaterThan(0);
    // Nothing on the board to pick, so nothing on the board is lit.
    expect(staging.candidates).toEqual([]);
  });

  it('counts a count slot against the server’s own count, and gates confirm on it', () => {
    const view = viewOf(BOTTOM_GAME_VIEW_JSON);
    const action = decisionActionOf(view);
    const empty = beginMultiSelect(action);
    const one = toggle(empty, 'card_a');

    const start = deriveDecision(view, {
      targeting: null,
      multiSelect: empty,
      forced: null,
    }).surface!;
    expect(start.count).toBe('0 of 2 selected');
    expect(start.confirm).toBe(true);
    expect(start.confirmDisabledReason).toContain('needs:');

    const partial = deriveDecision(view, {
      targeting: null,
      multiSelect: one,
      forced: null,
    }).surface!;
    expect(partial.count).toBe('1 of 2 selected');
    expect(partial.confirmDisabledReason).toContain('needs:');

    const full = deriveDecision(view, {
      targeting: null,
      multiSelect: toggle(one, 'card_b'),
      forced: null,
    }).surface!;
    expect(full.confirmDisabledReason).toBeUndefined();
  });

  it('offers cancel for a declaration the player entered themselves', () => {
    const view = viewOf(DECLARE_ATTACKERS_GAME_VIEW_JSON);
    const action = decisionActionOf(view);
    const { surface } = deriveDecision(view, {
      targeting: null,
      multiSelect: beginMultiSelect(action),
      forced: null,
    });

    // §8: there IS a neutral state to return to here, so cancel renders.
    expect(surface!.cancel).toBe(true);
    expect(surface!.confirm).toBe(true);
    expect(surface!.rows).toBeUndefined();
  });

  it('walks a targeting session with progress and no confirm', () => {
    const view = viewOf(TARGETING_GAME_VIEW_JSON);
    const action = view.valid_actions.find((entry) => (entry.requirements?.length ?? 0) > 0)!;
    const { surface, staging } = deriveDecision(view, {
      targeting: beginTargeting(action),
      multiSelect: null,
      forced: null,
      canRetract: false,
    });

    expect(surface!.kind).toBe('targeting');
    expect(surface!.prompt).toBe(action.requirements![0]!.prompt);
    // §4.2 rule 2: the last pick submits, so there is nothing to confirm and no
    // "Next" to press — the plaque's confirm was always absent here.
    expect(surface!.confirm).toBe(false);
    expect(surface!.advance).toBe(false);
    expect(surface!.undo).toBe(false);
    expect(staging.candidates).toEqual(action.requirements![0]!.candidates);
    // A seat is a candidate exactly when the server listed it as one.
    expect(staging.playerCandidates).toContain('p2');
  });

  it('carries a numeric slot as its own control, not as an empty row list', () => {
    // A `number` slot (issue #554) is the one slot kind with no candidates at all.
    // Classified by candidate emptiness alone it reads as "answered in a list the
    // board cannot show", which would draw an empty row surface and no way to
    // answer; it is excluded explicitly instead, and brings its own control.
    const view = viewOf(TARGETING_GAME_VIEW_JSON);
    const x: ValidAction = {
      id: 'a9',
      type: 'cast_spell',
      label: 'Cast Emberfall Surge',
      token: 'h:x',
      prompts: [{ kind: 'number', slot: 'x', prompt: 'Choose a value for X', min: 1, max: 4 }],
    };
    const { surface, staging } = deriveDecision(view, {
      ...IDLE,
      multiSelect: beginMultiSelect(x),
    });

    expect(surface!.rows).toBeUndefined();
    // Every bound is the server's, and the value opens pre-filled at the minimum,
    // so Confirm is meaningful without touching the control.
    expect(surface!.number).toEqual({
      prompt: 'Choose a value for X',
      min: 1,
      max: 4,
      value: 1,
    });
    expect(surface!.confirm).toBe(true);
    // Nothing on the board or on a crest is staged for a numeric answer.
    expect(staging.candidates).toEqual([]);
    expect(staging.playerCandidates).toEqual([]);

    // The chosen value is what the control reads back.
    const raised = deriveDecision(view, {
      ...IDLE,
      multiSelect: setActiveNumber(beginMultiSelect(x), 3),
    });
    expect(raised.surface!.number?.value).toBe(3);
    // Out-of-range input is the session's clamp, not the surface's invention.
    const clamped = deriveDecision(view, {
      ...IDLE,
      multiSelect: setActiveNumber(beginMultiSelect(x), 99),
    });
    expect(clamped.surface!.number?.value).toBe(4);
  });

  it('leaves `number` undefined for every slot that is not one', () => {
    const view = viewOf(ZONE_SELECT_GAME_VIEW_JSON);
    const session = beginMultiSelect(decisionActionOf(view));
    expect(deriveDecision(view, { ...IDLE, multiSelect: session }).surface!.number).toBeUndefined();
  });

  it('offers the local retract only while there is a pick to give back', () => {
    const view = viewOf(TARGETING_GAME_VIEW_JSON);
    const action = view.valid_actions.find((entry) => (entry.requirements?.length ?? 0) > 0)!;
    const sessions = { targeting: beginTargeting(action), multiSelect: null, forced: null };

    expect(deriveDecision(view, { ...sessions, canRetract: true }).surface!.undo).toBe(true);
    expect(deriveDecision(view, { ...sessions, canRetract: false }).surface!.undo).toBe(false);
  });

  it('carries the server clock onto the one surface that is asking', () => {
    const view = viewOf(BOTTOM_GAME_VIEW_JSON);
    const action = decisionActionOf(view);
    const { surface } = deriveDecision(view, {
      targeting: null,
      multiSelect: beginMultiSelect(action),
      forced: null,
      deadline: 7,
    });

    expect(surface!.deadline).toBe(7);
  });
});
