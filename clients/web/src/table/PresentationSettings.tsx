/**
 * The presentation settings overlay (issue #505, React DOM per ADR 0003): where
 * the player sets this device's quality level, effect density, and motion
 * preference. Reachable from both the front door ({@link ConnectionScreen}) and
 * in-match (the game menu), so the same surface governs the experience before and
 * during a game.
 *
 * Everything here is a device preference (ADR 0024 / ADR 0027 idiom), never game
 * state: closing the overlay leaves a UI fully reconstructable from the next
 * `GameView`. Changing a control applies immediately through the observable
 * settings store — no reload — and the scene is never degraded at any level
 * (`docs/design/presentation-budgets.md` §Quality levels).
 */
import { useEffect } from 'react';
import { cx } from '../chrome/cx';
import type { EffectDensity, EffectQuality } from './effects';
import { usePresentationSettings } from './settings/usePresentationSettings';
import {
  getQualityDetection,
  setDensity,
  setMotion,
  setQuality,
  type MotionPreference,
} from './settings/presentationSettings';
import s from './chrome.module.css';

interface Props {
  /** Close the overlay (backdrop click, Escape, or the close control). */
  onClose: () => void;
}

/** Quality levels, in display order, with the budgets' one-line summary. */
const QUALITY_OPTIONS: { value: EffectQuality; label: string; description: string }[] = [
  {
    value: 'high',
    label: 'High',
    description: 'Full effects, layered shadows, and environment animation.',
  },
  {
    value: 'standard',
    label: 'Standard',
    description: 'Reduced effects and static shadows — the balanced default.',
  },
  {
    value: 'lite',
    label: 'Lite',
    description: 'Brief pulses and edge flashes only; the still backdrop stays put.',
  },
];

/** Effect-density steps — a spawn multiplier independent of the quality level. */
const DENSITY_OPTIONS: { value: EffectDensity; label: string; description: string }[] = [
  { value: 'full', label: 'Full', description: 'Every particle a level allows.' },
  { value: 'reduced', label: 'Reduced', description: 'About 40% of the particles.' },
  { value: 'minimal', label: 'Minimal', description: 'Pulses and paths only, no particles.' },
];

/** Motion preference, composed with the OS reduced-motion query. */
const MOTION_OPTIONS: { value: MotionPreference; label: string; description: string }[] = [
  { value: 'system', label: 'System', description: 'Follow this device’s reduced-motion setting.' },
  { value: 'reduced', label: 'Reduced', description: 'Snap every animation to its end state.' },
  { value: 'full', label: 'Full', description: 'Keep full motion even if the system reduces it.' },
];

/** One labelled radiogroup of option cards (mirrors the card-art picker chrome). */
function OptionGroup<T extends string>({
  label,
  name,
  options,
  active,
  onPick,
}: {
  label: string;
  name: string;
  options: readonly { value: T; label: string; description: string }[];
  active: T;
  onPick: (value: T) => void;
}) {
  return (
    <section className={s.settingsGroup}>
      <h3 className={s.settingsGroupLabel}>{label}</h3>
      <div role="radiogroup" aria-label={label} className={s.artOptions}>
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={active === option.value}
            data-testid={`${name}-${option.value}`}
            className={cx(s.artOption, active === option.value && s.artOptionActive)}
            onClick={() => onPick(option.value)}
          >
            <span className={s.artOptionLabel}>{option.label}</span>
            <span className={s.artOptionDescription}>{option.description}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

export function PresentationSettings({ onClose }: Props) {
  const settings = usePresentationSettings();
  const detection = getQualityDetection();

  // Escape closes the overlay (keyboard parity with the backdrop click). The
  // in-match table also routes Escape here through its keyboard hook; this
  // listener makes the front-door mount self-sufficient.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  return (
    <div
      data-testid="presentation-settings-backdrop"
      className={s.shortcutBackdrop}
      onClick={onClose}
      role="presentation"
    >
      <div
        data-testid="presentation-settings"
        className={s.shortcutPanel}
        role="dialog"
        aria-modal="true"
        aria-label="Display settings"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 className={s.shortcutTitle}>Display</h2>

        <OptionGroup
          label="Quality"
          name="quality"
          options={QUALITY_OPTIONS}
          active={settings.quality}
          onPick={setQuality}
        />
        {settings.qualityAutoDetected && (
          <p className={s.artNote} data-testid="quality-autodetected">
            Auto-detected for this device. {detection.reason} You can change it any time.
          </p>
        )}

        <OptionGroup
          label="Effect density"
          name="density"
          options={DENSITY_OPTIONS}
          active={settings.density}
          onPick={setDensity}
        />

        <OptionGroup
          label="Motion"
          name="motion"
          options={MOTION_OPTIONS}
          active={settings.motion}
          onPick={setMotion}
        />

        <div className={s.artActionsRow}>
          <button
            type="button"
            className={s.button}
            data-testid="presentation-settings-close"
            onClick={onClose}
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
