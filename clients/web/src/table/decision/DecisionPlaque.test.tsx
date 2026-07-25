/**
 * The decision plaque's contract (`control-language.md` §3.1, §8, §10, §11 —
 * issue #534).
 *
 * These pin the rules a careless edit breaks silently: that the plaque titles
 * itself from the server's label, that CONFIRM leads and CANCEL trails (§11's
 * non-colour channel for the pair), that a view-forced decision gets no cancel at
 * all (§8/D19 — the shipped #451 behaviour), that UNDO stays the local
 * retract-one-pick control and never grows into a takeback (GAP-1), and that the
 * plate is pointer-transparent where it hosts no control so the candidates
 * underneath stay tappable (#451).
 *
 * jsdom proves the DOM, never the drawing — see the note at the foot.
 */
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DecisionPlaque } from './DecisionPlaque';
import { confirmDisabledReason } from './confirmReason';
import type { PlaquePlacement } from './plaqueAnchor';

afterEach(cleanup);

const PLACEMENT: PlaquePlacement = {
  rect: { x: 514, y: 252, w: 272, h: 102 },
  form: 'anchored',
  side: 'below',
  slide: 0,
};

describe('DecisionPlaque', () => {
  it('titles itself from the server label, verbatim', () => {
    // The baseline's own plaque (control-ui panel 7). The client does not name
    // the server's actions — the only rewrite is the drawn small caps, which is
    // a property of the face and lives in CSS.
    render(<DecisionPlaque title="Choose attackers" placement={PLACEMENT} />);
    expect(screen.getByTestId('decision-plaque-title').textContent).toBe('Choose attackers');
    expect(screen.getByRole('group', { name: 'Choose attackers' })).toBeTruthy();
  });

  it('renders CONFIRM leading and CANCEL trailing (§11)', () => {
    // Order is the non-colour channel for this pair: it is the cue that survives
    // a colour-blind path, so it is not a layout preference to be re-flowed.
    render(
      <DecisionPlaque
        title="Choose attackers"
        placement={PLACEMENT}
        confirm={{ label: 'Confirm', onConfirm: () => {} }}
        onAdvance={() => {}}
        onUndo={() => {}}
        cancel={{ onCancel: () => {} }}
      />,
    );
    const labels = screen
      .getAllByRole('button')
      .map((button) => button.getAttribute('data-testid'));
    expect(labels[0]).toBe('decision-plaque-confirm');
    expect(labels[labels.length - 1]).toBe('decision-plaque-cancel');
  });

  it('reports every control press back to the caller', () => {
    const onConfirm = vi.fn();
    const onAdvance = vi.fn();
    const onUndo = vi.fn();
    const onCancel = vi.fn();
    render(
      <DecisionPlaque
        title="Declare blockers"
        placement={PLACEMENT}
        confirm={{ label: 'Confirm', onConfirm }}
        onAdvance={onAdvance}
        onUndo={onUndo}
        cancel={{ onCancel }}
      />,
    );
    fireEvent.click(screen.getByTestId('decision-plaque-confirm'));
    fireEvent.click(screen.getByTestId('decision-plaque-advance'));
    fireEvent.click(screen.getByTestId('decision-plaque-undo'));
    fireEvent.click(screen.getByTestId('decision-plaque-cancel'));
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onAdvance).toHaveBeenCalledOnce();
    expect(onUndo).toHaveBeenCalledOnce();
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('offers NO cancel for a view-forced decision (§8/D19)', () => {
    // A mulligan has no neutral state to return to, so #451 offers no cancel
    // rather than one that re-opens what it just closed. The caller signals it
    // by omitting the prop; the plaque must not invent a dismissal.
    render(
      <DecisionPlaque
        title="Mulligan"
        placement={PLACEMENT}
        confirm={{ label: 'Keep', onConfirm: () => {} }}
      />,
    );
    expect(screen.queryByTestId('decision-plaque-cancel')).toBeNull();
  });

  it('omits the advance and undo controls when they are not offered', () => {
    // §8/C8: the UNDO pill does NOT render in the neutral state the baseline
    // draws it in — an unavailable action does not render, and there is nothing
    // to retract before a pick exists.
    render(<DecisionPlaque title="Cast Lightning Bolt" placement={PLACEMENT} />);
    expect(screen.queryByTestId('decision-plaque-undo')).toBeNull();
    expect(screen.queryByTestId('decision-plaque-advance')).toBeNull();
  });

  it('calls UNDO a retraction, not a takeback (GAP-1)', () => {
    // "Undo" promises a takeback to anyone who cannot see where the control
    // sits. There is none: `ChooseAction` is final and no undo exists in
    // `valid_actions`, so the accessible name says what it actually does.
    render(<DecisionPlaque title="Choose attackers" placement={PLACEMENT} onUndo={() => {}} />);
    expect(screen.getByRole('button', { name: 'Retract the last pick' })).toBeTruthy();
  });

  it('disables CONFIRM only with the server’s stated reason (§3.2, D14)', () => {
    render(
      <DecisionPlaque
        title="Mulligan"
        placement={PLACEMENT}
        confirm={{
          label: 'Keep',
          onConfirm: () => {},
          disabledReason: 'needs: put two cards on the bottom',
        }}
      />,
    );
    const confirm = screen.getByTestId('decision-plaque-confirm') as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    expect(confirm.textContent).toContain('needs: put two cards on the bottom');
  });

  it('enables CONFIRM once the server-stated cardinality is met', () => {
    render(
      <DecisionPlaque
        title="Mulligan"
        placement={PLACEMENT}
        confirm={{ label: 'Keep', onConfirm: () => {} }}
      />,
    );
    expect((screen.getByTestId('decision-plaque-confirm') as HTMLButtonElement).disabled).toBe(
      false,
    );
  });

  it('is pointer-transparent except where it hosts a control (§10.2, #451)', () => {
    // The regression this guards is the shipped one: a decision surface that
    // swallowed every click on the very cards being chosen, leaving the mulligan
    // selectable-looking and unanswerable.
    const { container } = render(
      <DecisionPlaque
        title="Choose attackers"
        placement={PLACEMENT}
        confirm={{ label: 'Confirm', onConfirm: () => {} }}
      />,
    );
    const plaque = screen.getByTestId('decision-plaque');
    expect(plaque.getAttribute('data-pointer-through')).toBe('true');
    // The control row is the one part that takes events back.
    const row = container.querySelector('[class*="controls"]');
    expect(row).not.toBeNull();
    expect(within(row as HTMLElement).getAllByRole('button').length).toBe(1);
  });

  it('draws itself exactly where the placement said, and nowhere else', () => {
    render(<DecisionPlaque title="Choose attackers" placement={PLACEMENT} />);
    const plaque = screen.getByTestId('decision-plaque');
    expect(plaque.style.left).toBe('514px');
    expect(plaque.style.top).toBe('252px');
    expect(plaque.style.width).toBe('272px');
    // An anchored plaque grows to its content; only the sheet's capped height is
    // load-bearing, so it is the only form that pins one.
    expect(plaque.style.height).toBe('');
    expect(plaque.getAttribute('data-form')).toBe('anchored');
    expect(plaque.getAttribute('data-side')).toBe('below');
  });

  it('honours the bottom sheet’s capped height (§10.2)', () => {
    render(
      <DecisionPlaque
        title="Mulligan"
        placement={{
          rect: { x: 16, y: 260, w: 358, h: 337 },
          form: 'sheet',
          side: 'sheet',
          slide: 0,
        }}
      />,
    );
    const plaque = screen.getByTestId('decision-plaque');
    expect(plaque.getAttribute('data-form')).toBe('sheet');
    // Without this the sheet grows past its cap and reaches down over the
    // receiver's band — the one thing §10.2 states outright about this form.
    expect(plaque.style.height).toBe('337px');
  });
});

describe('confirmDisabledReason', () => {
  it('prints the server’s slot prompt verbatim, in the required phrasing', () => {
    expect(confirmDisabledReason(false, 'put two cards on the bottom')).toBe(
      'needs: put two cards on the bottom',
    );
  });

  it('is undefined once the cardinality is satisfied', () => {
    expect(confirmDisabledReason(true, 'put two cards on the bottom')).toBeUndefined();
  });

  it('refuses to disable a control the server gave no reason for (D14/GAP-4)', () => {
    // There is no general unavailability reason in the protocol. Rather than
    // invent one — which would be the client stating legality — the control
    // stays enabled and the server rejects a premature answer.
    expect(confirmDisabledReason(false, undefined)).toBeUndefined();
    expect(confirmDisabledReason(false, '')).toBeUndefined();
  });
});

/*
 * What jsdom cannot prove, and what belongs to the maintainer:
 *
 * - That a click really passes through the plate to a candidate underneath.
 *   jsdom does not hit-test and does not apply `pointer-events`, so the rule is
 *   asserted as a class and an attribute, never as behaviour.
 * - That every control clears the 44 px floor. The hit box is CSS
 *   (`--rune-control-hit`); jsdom reports every element as 0 × 0.
 * - That the plaque actually paints above the shell's chrome. The `decision`
 *   rung is a `z-index` token; jsdom computes no stacking order.
 * - That the drawn plate, its gold frame, and the small-caps title match the
 *   baseline. Those are pixels.
 */
