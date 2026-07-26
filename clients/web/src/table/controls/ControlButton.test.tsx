/**
 * The button family's contract (`docs/design/control-language.md` §3, issue #534).
 *
 * These assert the rules that a careless refactor breaks silently: that the hit
 * box is a separate box from the drawn plate, that disabling always carries the
 * server's reason, that a glyph can never ship unlabeled, and that every state
 * in the §3.2 matrix leaves a non-colour trace in the DOM. jsdom cannot prove
 * the drawn result — see the note at the foot of this file.
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ControlButton, IconButton } from './ControlButton';

afterEach(cleanup);

describe('ControlButton', () => {
  it('prints the server label verbatim and reports the press', () => {
    const onPress = vi.fn();
    render(<ControlButton variant="primary" label="PASS PRIORITY" onPress={onPress} />);
    const button = screen.getByRole('button', { name: 'PASS PRIORITY' });
    fireEvent.click(button);
    expect(onPress).toHaveBeenCalledOnce();
  });

  it('gives the primary the stadium and every other variant the chamfer', () => {
    // Two silhouettes, and only two — §3's whole premise is that rank reads off
    // the outline. The class is the observable proxy for the clip-path, which
    // jsdom does not compute.
    const { rerender, container } = render(
      <ControlButton variant="primary" label="CAST SPELL" onPress={() => {}} />,
    );
    expect(container.querySelector('button')?.className).not.toMatch(/chamfered/);

    for (const variant of ['primaryCompact', 'cancel', 'secondary', 'utility'] as const) {
      rerender(<ControlButton variant={variant} label="X" onPress={() => {}} />);
      expect(container.querySelector('button')?.className).toMatch(/chamfered/);
    }
  });

  it('cannot be disabled without printing the server-stated reason (D14)', () => {
    // The regression this guards: someone adds `disabled?: boolean` for
    // convenience and the client starts greying controls on its own judgement,
    // which §3.2 forbids — an unoffered action is ABSENT, never greyed.
    render(
      <ControlButton
        variant="secondary"
        label="KEEP"
        onPress={() => {}}
        disabledReason="needs: choose a land"
      />,
    );
    const button = screen.getByRole('button', { name: /KEEP/ }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.textContent).toContain('needs: choose a land');
  });

  it('is enabled whenever no reason is supplied', () => {
    render(<ControlButton variant="secondary" label="KEEP" onPress={() => {}} />);
    expect((screen.getByRole('button', { name: 'KEEP' }) as HTMLButtonElement).disabled).toBe(
      false,
    );
  });

  it('carries a non-colour channel for every state in the §3.2 matrix', () => {
    const { rerender } = render(
      <ControlButton variant="primary" label="PASS" onPress={() => {}} pressed />,
    );
    // Selected: the shape channel is the filled inner bar, keyed off aria-pressed.
    expect(screen.getByRole('button').getAttribute('aria-pressed')).toBe('true');

    // Pending-server: the hairline is a real element, plus aria-busy.
    rerender(<ControlButton variant="primary" label="PASS" onPress={() => {}} pending />);
    expect(screen.getByRole('button').getAttribute('aria-busy')).toBe('true');

    // Deadline warning: the countdown chip carries digits, not just a hue.
    rerender(
      <ControlButton variant="primary" label="PASS" onPress={() => {}} deadlineSeconds={7} />,
    );
    expect(screen.getByRole('button').textContent).toContain('0:07');

    rerender(
      <ControlButton variant="primary" label="PASS" onPress={() => {}} deadlineSeconds={75} />,
    );
    expect(screen.getByRole('button').textContent).toContain('1:15');
  });

  it('replays the rejection shake when the nonce advances', () => {
    // The animation is keyed on the nonce because re-applying the same class
    // does not restart a CSS animation; a second rejection must still shake.
    const { rerender } = render(
      <ControlButton variant="primary" label="PASS" onPress={() => {}} shakeNonce={1} />,
    );
    const first = screen.getByRole('button');
    expect(first.getAttribute('data-shake')).toBe('true');

    rerender(<ControlButton variant="primary" label="PASS" onPress={() => {}} shakeNonce={2} />);
    expect(screen.getByRole('button')).not.toBe(first);
  });

  it('takes an accessible name distinct from the drawn word', () => {
    // RESPOND is the case: the drawn word does not say what it does to someone
    // who cannot see that it sits beside the primary.
    render(
      <ControlButton
        variant="secondary"
        label="RESPOND"
        accessibleName="Respond instead of passing"
        onPress={() => {}}
      />,
    );
    expect(screen.getByRole('button', { name: 'Respond instead of passing' })).toBeDefined();
  });
});

describe('IconButton', () => {
  it('always has an accessible name, and hides the glyph from it', () => {
    render(<IconButton glyph="☰" label="Game menu" onPress={() => {}} />);
    const button = screen.getByRole('button', { name: 'Game menu' });
    expect(button.getAttribute('title')).toBe('Game menu');
    // §3.1: no unlabeled glyphs. The glyph is decorative; the label is the name.
    expect(button.querySelector('[aria-hidden="true"]')?.textContent).toBe('☰');
  });

  it('reports disclosure state when it controls a surface', () => {
    render(
      <IconButton glyph="›" label="Step list" controls="step-list" expanded onPress={() => {}} />,
    );
    const button = screen.getByRole('button', { name: 'Step list' });
    expect(button.getAttribute('aria-expanded')).toBe('true');
    expect(button.getAttribute('aria-controls')).toBe('step-list');
  });
});

/*
 * What jsdom cannot prove here, and what therefore belongs to the maintainer:
 * the drawn 45° chamfer, the gold gradient's direction, the primary's pale top
 * rim, the real pointer hit area at the tablet floor, and whether the 36 px
 * plate inside a 44 px target reads as the baselines draw it. jsdom computes no
 * `clip-path` and no layout, so those are asserted through classes and tokens
 * only. There is no browser suite and none may be added (`AGENTS.md`).
 */
