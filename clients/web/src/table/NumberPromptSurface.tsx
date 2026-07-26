/**
 * The numeric prompt surface (issue #554): the control for a server-posed `number`
 * slot — the value of X, how many counters to remove, one share of a divided effect.
 *
 * Every bound is the server's. The control offers exactly `min`..`max` and computes
 * no affordability, no cost, and no legality of its own; the same clamp lives in
 * {@link setActiveNumber} so a typed value and a stepped one agree.
 *
 * Three input paths, none of them exclusive (`clients/web/AGENTS.md`, touch first):
 * a `range` slider for coarse dragging, a `number` field for exact entry and
 * keyboard, and two ≥44px stepper buttons for a touch nudge. All three write the same
 * value, and the slot's answer is the number as a decimal string.
 */
import { SymbolText, symbolNotationText } from '../chrome/symbols';
import s from './chrome.module.css';

interface Props {
  /** The server's slot prompt, shown as the surface heading. */
  prompt: string;
  /** The smallest legal value, inclusive — the server's own bound. */
  min: number;
  /** The largest legal value, inclusive — the server's own bound. */
  max: number;
  /** The currently chosen value. */
  value: number;
  /** Report a new value; the caller clamps it into `min`..`max`. */
  onChange: (value: number) => void;
}

export function NumberPromptSurface({ prompt, min, max, value, onChange }: Props) {
  const step = (delta: number): void => onChange(value + delta);
  return (
    <section
      data-testid="number-prompt"
      className={s.promptSurface}
      aria-label={symbolNotationText(prompt)}
    >
      {/* The server's prompt, its `{…}` runs drawn as symbols (issue #462). */}
      <h2 className={s.promptSurfaceTitle}>
        <SymbolText text={prompt} />
      </h2>
      <span className={s.promptSurfaceZone}>
        {min}–{max}
      </span>
      <div className={s.numberPromptRow}>
        <button
          type="button"
          className={s.numberPromptStep}
          onClick={() => step(-1)}
          disabled={value <= min}
          aria-label="Decrease"
          data-testid="number-prompt-down"
        >
          −
        </button>
        <input
          type="number"
          className={s.numberPromptField}
          min={min}
          max={max}
          step={1}
          value={value}
          onChange={(event) => onChange(Number(event.target.value))}
          aria-label={symbolNotationText(prompt)}
          data-testid="number-prompt-field"
        />
        <button
          type="button"
          className={s.numberPromptStep}
          onClick={() => step(1)}
          disabled={value >= max}
          aria-label="Increase"
          data-testid="number-prompt-up"
        >
          +
        </button>
      </div>
      <input
        type="range"
        className={s.numberPromptSlider}
        min={min}
        max={max}
        step={1}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        aria-label={symbolNotationText(prompt)}
        data-testid="number-prompt-slider"
      />
    </section>
  );
}
