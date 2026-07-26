/**
 * The one decision surface (issue #567).
 *
 * `decisionSurface.test.ts` proves *what* is being asked; this proves *how* it is
 * drawn and answered, and that it is drawn once. The failures it stands against
 * are the ones the issue names: the same question in three places, a plaque with
 * no controls on it, a forced decision offering a cancel that would re-open
 * itself, and named choices in a button family of their own.
 *
 * jsdom draws nothing and resolves no `clip-path` or layout, so nothing here
 * claims a pixel — only which nodes exist, what they announce, and what pressing
 * them calls.
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DecisionArea } from './DecisionArea';
import type { DecisionSurface } from './decisionSurface';

afterEach(cleanup);

function handlers() {
  return {
    onConfirm: vi.fn(),
    onAdvance: vi.fn(),
    onUndo: vi.fn(),
    onCancel: vi.fn(),
    onToggleRow: vi.fn(),
    onMoveRow: vi.fn(),
    onChooseOption: vi.fn(),
  };
}

function surfaceOf(overrides: Partial<DecisionSurface> = {}): DecisionSurface {
  return {
    kind: 'multiSelect',
    title: 'Declare attackers',
    prompt: 'Choose attackers',
    confirm: true,
    advance: false,
    undo: false,
    cancel: true,
    ...overrides,
  };
}

describe('DecisionArea', () => {
  it('draws the question exactly once, and titles itself from the server label', () => {
    render(<DecisionArea surface={surfaceOf()} {...handlers()} />);

    // The whole point of #567's first bullet: one node carries the sentence.
    // The strip and the sheet that used to restate it no longer exist, and this
    // attribute is what the shell's own tests count.
    expect(document.querySelectorAll('[data-decision-prompt]')).toHaveLength(1);
    expect(screen.getByTestId('decision-prompt').textContent).toBe('Choose attackers');
    expect(screen.getByRole('group', { name: 'Declare attackers' })).toBeTruthy();
    // The area announces itself on appearance, which is the accessibility cost
    // ADR 0032 accepts for contextual chrome.
    expect(screen.getByRole('status')).toBeTruthy();
  });

  it('answers with the controls the derivation offered, and only those', () => {
    const on = handlers();
    render(<DecisionArea surface={surfaceOf({ advance: true, undo: true })} {...on} />);

    fireEvent.click(screen.getByTestId('decision-area-confirm'));
    fireEvent.click(screen.getByTestId('decision-area-advance'));
    fireEvent.click(screen.getByTestId('decision-area-undo'));
    fireEvent.click(screen.getByTestId('decision-area-cancel'));

    expect(on.onConfirm).toHaveBeenCalledTimes(1);
    expect(on.onAdvance).toHaveBeenCalledTimes(1);
    expect(on.onUndo).toHaveBeenCalledTimes(1);
    expect(on.onCancel).toHaveBeenCalledTimes(1);
  });

  it('names UNDO for what it does, because it is not a takeback', () => {
    // §8/GAP-1: no `undo` exists in `valid_actions` and no message carries one.
    // "Undo" promises a takeback to a reader who cannot see the drawn pill.
    render(<DecisionArea surface={surfaceOf({ undo: true })} {...handlers()} />);
    expect(screen.getByRole('button', { name: 'Retract the last pick' })).toBeTruthy();
  });

  it('offers no cancel for a decision the view forces', () => {
    // §8/D19, #451: there is no neutral state to return to, and a control that
    // instantly re-opens what it closed is worse than no control.
    render(<DecisionArea surface={surfaceOf({ cancel: false })} {...handlers()} />);
    expect(screen.queryByTestId('decision-area-cancel')).toBeNull();
  });

  it('never renders a control-less surface', () => {
    // The defect: with named options in play the plaque's confirm was undefined
    // and, being forced, so was its cancel — so it drew a title and nothing to
    // press. Whatever the derivation says, the surface always carries an answer.
    const forcedOption = surfaceOf({
      confirm: false,
      cancel: false,
      choices: [{ id: 'keep', label: 'Keep this hand' }],
    });
    render(<DecisionArea surface={forcedOption} {...handlers()} />);
    const area = screen.getByTestId('decision-area');
    expect(area.querySelectorAll('button').length).toBeGreaterThan(0);
  });

  it('draws named choices in the control language, beside the primary action', () => {
    const on = handlers();
    render(
      <DecisionArea
        surface={surfaceOf({
          confirm: false,
          choices: [
            { id: 'keep', label: 'Keep this hand', disabledReason: 'needs: bottom one card' },
            { id: 'mulligan', label: 'Mulligan' },
          ],
        })}
        {...on}
      />,
    );

    const keep = screen.getByTestId<HTMLButtonElement>('multiselect-option-keep');
    const mulligan = screen.getByTestId<HTMLButtonElement>('multiselect-option-mulligan');
    // The shared family, not a bespoke button: `data-variant` is `ControlButton`'s.
    expect(keep.dataset.variant).toBe('confirm');
    // The ONE permitted disablement prints the server's reason beside the label
    // (§3.2, D14) — a greyed control with no reason is unwritable by construction.
    expect(keep.disabled).toBe(true);
    expect(keep.textContent).toContain('needs: bottom one card');
    expect(mulligan.disabled).toBe(false);

    fireEvent.click(mulligan);
    expect(on.onChooseOption).toHaveBeenCalledWith('mulligan');
  });

  it('hosts a list-answered slot’s rows on the same surface as the question', () => {
    const on = handlers();
    render(
      <DecisionArea
        surface={surfaceOf({
          prompt: 'Choose a card to return',
          rows: {
            mode: 'select',
            zone: 'graveyard',
            items: [{ id: 'g1', label: 'Grizzly Bears', chosen: false }],
          },
        })}
        {...on}
      />,
    );

    const area = screen.getByTestId('decision-area');
    expect(area.contains(screen.getByTestId('prompt-surface'))).toBe(true);
    fireEvent.click(screen.getByTestId('zone-select-g1'));
    expect(on.onToggleRow).toHaveBeenCalledWith('g1');
    // Still one sentence node, even with the surface's own heading present.
    expect(document.querySelectorAll('[data-decision-prompt]')).toHaveLength(1);
  });

  it('reorders an order slot with row controls rather than drag (D22)', () => {
    const on = handlers();
    render(
      <DecisionArea
        surface={surfaceOf({
          prompt: 'Order these triggers',
          rows: {
            mode: 'order',
            items: [
              { id: 't1', label: 'First', chosen: true },
              { id: 't2', label: 'Second', chosen: true },
            ],
          },
        })}
        {...on}
      />,
    );

    fireEvent.click(screen.getByTestId('order-down-t1'));
    expect(on.onMoveRow).toHaveBeenCalledWith('t1', 1);
  });

  it('shows progress, count, and the server clock without repeating the question', () => {
    render(
      <DecisionArea
        surface={surfaceOf({ progress: 'Step 2 of 3', count: '1 of 2 selected', deadline: 9 })}
        {...handlers()}
      />,
    );

    expect(screen.getByTestId('decision-progress').textContent).toBe('Step 2 of 3');
    expect(screen.getByTestId('decision-count').textContent).toBe('1 of 2 selected');
    expect(document.querySelectorAll('[data-decision-prompt]')).toHaveLength(1);
  });

  it('reaches every control by keyboard and touch, at the 44px floor', () => {
    // §7's parity table: there is no hover-only and no drag-only affordance
    // here. Every control is a real <button>, so click, Enter/Space, and tap are
    // one path; `ControlButton` floors the hit box.
    render(<DecisionArea surface={surfaceOf({ advance: true, undo: true })} {...handlers()} />);
    const area = screen.getByTestId('decision-area');
    for (const button of Array.from(area.querySelectorAll('button'))) {
      expect(button.tagName).toBe('BUTTON');
      expect(button.getAttribute('type')).toBe('button');
    }
  });

  it('installs no key handler of its own', () => {
    // `useTableKeyboard` owns Escape for the whole shell; a second handler here
    // would let the two disagree about what one press cancels.
    const on = handlers();
    render(<DecisionArea surface={surfaceOf()} {...on} />);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(on.onCancel).not.toHaveBeenCalled();
  });
});
