/**
 * The match control **button family** (`docs/design/control-language.md` §3,
 * issue #534, under [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * One component renders every control the approved baselines draw, because §3
 * says there is only ever one control: two silhouettes (stadium and chamfered
 * rect), and beyond that a fill, a frame, and a size. Rendering them from one
 * place is what keeps that true — a second button component is how a third
 * silhouette gets in.
 *
 * ## What this component refuses to do
 *
 * It never decides that a control should exist, and it never writes its own
 * label. Both are the server's: a control renders because an entry sits in
 * `valid_actions[]`, and its label is that entry's `label`, printed verbatim.
 * The one rewrite applied is `text-transform: uppercase`, which is a drawn
 * property of the face, not a change of words.
 *
 * It also never renders itself disabled on its own judgement. §3.2 and D14 are
 * blunt about this: an action the server has not offered is *absent*, not
 * greyed, and the only disablement that may render is a `PromptOption.requires`
 * the server itself states — which is why {@link ControlButtonProps.disabledReason}
 * is a **string** and not a boolean. There is no way to disable a control here
 * without printing the server's reason next to it.
 *
 * ## Plate versus hit box
 *
 * The `<button>` is the hit box, floored at 44 px; the `.face` inside it is the
 * drawn plate at its baseline height. See `controls.module.css` for why the two
 * are separate boxes, and `controlTokens.ts` for the numbers.
 */
import type { ReactNode } from 'react';
import { cx } from '../../chrome/cx';
import { SymbolText, hasSymbolNotation, symbolNotationText } from '../../chrome/symbols';
import s from './controls.module.css';

/**
 * The drawn variants of §3.1. Each is a fill and a size over one of the two
 * silhouettes; nothing here adds a shape.
 */
export type ControlVariant =
  /** The large stadium pill — the single blue primary of §4.1. */
  | 'primary'
  /** The compact blue pair-width primary (zones panel 10 `RESOLVE`). */
  | 'primaryCompact'
  /** Green confirm (control-ui panel 7). Always leads its pair. */
  | 'confirm'
  /** Red cancel / destructive (control-ui panel 7). Always trails its pair. */
  | 'cancel'
  /** Dark outline secondary (zones panel 10 `RESPOND`). */
  | 'secondary'
  /** Dark utility pill (control-ui panel 6 `UNDO`). */
  | 'utility';

export interface ControlButtonProps {
  variant: ControlVariant;
  /**
   * The drawn label. For anything that answers a `ValidAction` this MUST be
   * `action.label` verbatim — the client does not name the server's actions.
   */
  label: string;
  onPress: () => void;
  /**
   * The accessible name, when the drawn label is not a complete sentence on its
   * own. `RESPOND` uses it ("Respond instead of passing"), because the drawn
   * word does not say what the control does to someone who cannot see where it
   * sits.
   */
  accessibleName?: string;
  /**
   * The server's stated reason this control cannot be used yet — today only a
   * `PromptOption.requires` that is unsatisfied, rendered verbatim as
   * "needs: <slot prompt>". Passing it is the ONLY way to render disabled, and
   * the reason is always drawn (§3.2, D14, GAP-4).
   */
  disabledReason?: string;
  /** Toggled state, for controls that carry one (`aria-pressed`). */
  pressed?: boolean;
  /**
   * The local, ≤5 s, non-load-bearing submission lock (D13). The protocol has no
   * per-submission acknowledgement (GAP-5), so this can never be authoritative —
   * it is released by any inbound view, and a fresh mount never reproduces it.
   */
  pending?: boolean;
  /** Seconds left on the server clock; renders the countdown chip and warning frame. */
  deadlineSeconds?: number;
  /**
   * Bumped when the server rejects an action, to play the recovery shake once.
   * A nonce rather than a boolean, so a second rejection re-plays it.
   */
  shakeNonce?: number;
  /** A keyboard hint drawn on the control (the shipped `P` binding on pass). */
  hint?: string;
  /** A glyph drawn before the label. */
  leading?: ReactNode;
  testId?: string;
}

/** The seconds → `m:ss` chip the deadline warning appends. */
function countdown(seconds: number): string {
  const clamped = Math.max(0, Math.floor(seconds));
  return `${Math.floor(clamped / 60)}:${String(clamped % 60).padStart(2, '0')}`;
}

export function ControlButton({
  variant,
  label,
  onPress,
  accessibleName,
  disabledReason,
  pressed,
  pending,
  deadlineSeconds,
  shakeNonce,
  hint,
  leading,
  testId,
}: ControlButtonProps) {
  // The stadium is the primary's alone; everything else is chamfered. Clipping
  // the frame and the face with the same polygon is what keeps the 45° cut and
  // the gradient frame together (see the stylesheet's note).
  const chamfered = variant !== 'primary';
  const disabled = disabledReason !== undefined;
  // An ability's server label carries symbol notation (`{T}: Add {G}.`). The
  // face draws it as symbols (issue #462); the accessible name then has to be
  // the spoken substitution, because the drawn letters are `role="img"` and a
  // reader would otherwise hear a label with holes in it. An explicit
  // `accessibleName` still wins — it is the caller saying the drawn words are
  // not a sentence.
  const spokenLabel =
    accessibleName ?? (hasSymbolNotation(label) ? symbolNotationText(label) : undefined);

  return (
    <button
      type="button"
      className={cx(s.button, s[variant], chamfered && s.chamfered)}
      onClick={onPress}
      disabled={disabled}
      aria-label={spokenLabel}
      aria-pressed={pressed}
      aria-busy={pending || undefined}
      data-variant={variant}
      data-deadline={deadlineSeconds !== undefined || undefined}
      data-shake={shakeNonce ? 'true' : undefined}
      data-testid={testId}
      // Re-mounting on a new nonce is what makes the recovery shake replay; a
      // CSS animation does not restart when the same class is re-applied.
      key={shakeNonce}
    >
      <span className={cx(s.frame, chamfered && s.chamfered)}>
        <span className={cx(s.face, chamfered && s.chamfered)}>
          {leading}
          <span>
            <SymbolText text={label} />
          </span>
          {disabledReason !== undefined && <span className={s.reason}>{disabledReason}</span>}
          {hint !== undefined && (
            <kbd className={s.hint} aria-hidden="true">
              {hint}
            </kbd>
          )}
          {deadlineSeconds !== undefined && (
            <span className={s.chip}>{countdown(deadlineSeconds)}</span>
          )}
          {pending && <span className={s.sweep} aria-hidden="true" />}
        </span>
      </span>
    </button>
  );
}

export interface IconButtonProps {
  /** The drawn glyph. Never the accessible name — see {@link IconButtonProps.label}. */
  glyph: ReactNode;
  /**
   * The accessible name. Required, not optional: §3.1 states that every control
   * ships one and that there are no unlabeled glyphs, so the type makes an
   * unlabeled icon impossible to write rather than merely discouraged.
   */
  label: string;
  onPress: () => void;
  /** For a disclosure icon: whether the surface it controls is open. */
  expanded?: boolean;
  /** The id of the surface this icon discloses. */
  controls?: string;
  pressed?: boolean;
  testId?: string;
}

/**
 * The 44 ⌀ circular icon button — the game-menu handle (D5).
 *
 * This is the control that pins the whole spec's scale: it measures 44 × 44 in
 * the baselines, exactly the touch floor, which is what fixes 1 baseline px =
 * 1 CSS px (D1). It is deliberately the only circle in the family.
 */
export function IconButton({
  glyph,
  label,
  onPress,
  expanded,
  controls,
  pressed,
  testId,
}: IconButtonProps) {
  return (
    <button
      type="button"
      className={cx(s.button, s.icon)}
      onClick={onPress}
      aria-label={label}
      title={label}
      aria-expanded={expanded}
      aria-controls={controls}
      aria-pressed={pressed}
      data-testid={testId}
    >
      <span className={s.frame}>
        <span className={s.face} aria-hidden="true">
          {glyph}
        </span>
      </span>
    </button>
  );
}
