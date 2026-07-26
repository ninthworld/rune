import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ValidAction } from '../protocol';
import { DecisionSheet } from './DecisionSheet';
import {
  activeChosen,
  activeSlot,
  beginMultiSelect,
  setActiveNumber,
  type MultiSelectSession,
} from './multiSelect';
import { normalizeGameView } from '../wire';

/** A cast posing one numeric slot — the X-spell shape the server projects (#554). */
const X_SPELL: ValidAction = {
  id: 'a9',
  type: 'cast_spell',
  label: 'Cast Emberfall Surge',
  token: 'h:x',
  prompts: [{ kind: 'number', slot: 'x', prompt: 'Choose a value for X', min: 0, max: 3 }],
};

const VIEW = normalizeGameView({ phase: 'precombat_main', you: 'p0' });

afterEach(cleanup);

function renderSheet(session: MultiSelectSession, onNumber = vi.fn()) {
  render(
    <DecisionSheet
      view={VIEW}
      multiSelect={session}
      sheetMode
      msSlot={activeSlot(session)}
      onToggle={vi.fn()}
      onMove={vi.fn()}
      onNumber={onNumber}
      onChooseOption={vi.fn()}
    />,
  );
  return onNumber;
}

describe('numeric prompt control (issue #554)', () => {
  it('renders a usable control over exactly the server-stated range', () => {
    const session = beginMultiSelect(X_SPELL);
    renderSheet(session);

    const field = screen.getByTestId('number-prompt-field') as HTMLInputElement;
    const slider = screen.getByTestId('number-prompt-slider') as HTMLInputElement;
    // Both bounds come from the server; the control invents neither.
    for (const control of [field, slider]) {
      expect(control.min).toBe('0');
      expect(control.max).toBe('3');
    }
    // The session pre-fills the minimum, so the control opens on a legal value.
    expect(field.value).toBe('0');
    // At the bottom of the range there is nothing to step down to.
    expect((screen.getByTestId('number-prompt-down') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('number-prompt-up') as HTMLButtonElement).disabled).toBe(false);
  });

  it('reports every input path back to the caller', () => {
    // Touch-first: the value is reachable by stepper, by typing, and by slider —
    // no path is exclusive (clients/web/AGENTS.md).
    const session = setActiveNumber(beginMultiSelect(X_SPELL), 1);
    const onNumber = renderSheet(session);

    fireEvent.click(screen.getByTestId('number-prompt-up'));
    expect(onNumber).toHaveBeenLastCalledWith(2);
    fireEvent.click(screen.getByTestId('number-prompt-down'));
    expect(onNumber).toHaveBeenLastCalledWith(0);
    fireEvent.change(screen.getByTestId('number-prompt-field'), { target: { value: '3' } });
    expect(onNumber).toHaveBeenLastCalledWith(3);
    fireEvent.change(screen.getByTestId('number-prompt-slider'), { target: { value: '2' } });
    expect(onNumber).toHaveBeenLastCalledWith(2);
  });

  it('renders the server prompt verbatim and answers with the numeral as a string', () => {
    const session = setActiveNumber(beginMultiSelect(X_SPELL), 2);
    renderSheet(session);
    expect(screen.getByTestId('number-prompt').textContent).toContain('Choose a value for X');
    expect((screen.getByTestId('number-prompt-field') as HTMLInputElement).value).toBe('2');
    // The slot's answer rides the shared `TargetChoice.chosen` shape.
    expect(activeChosen(session)).toEqual(['2']);
  });

  it('shows no candidate list for a numeric slot', () => {
    // A `number` slot has no candidates, so the list surface must not appear.
    renderSheet(beginMultiSelect(X_SPELL));
    expect(screen.queryByTestId('prompt-surface')).toBeNull();
  });
});
