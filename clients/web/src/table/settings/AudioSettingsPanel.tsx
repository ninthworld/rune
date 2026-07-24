/**
 * The sound and haptics block of the display settings surface (issue #507,
 * inside the #505 overlay).
 *
 * Master mute, master volume, a switch per taxonomy category, and the haptics
 * opt-in — all device-local through {@link audioSettings}, applied immediately,
 * and persisted without touching the protocol. The copy is honest about the
 * state of the world: no production audio ships yet (ADR 0031), so the controls
 * govern hooks that are wired and currently silent.
 */
import { cx } from '../../chrome/cx';
import { AUDIO_CUE_CATEGORIES, AUDIO_CUE_LABELS, hapticsSupported } from '../audio';
import {
  setAudioMuted,
  setAudioVolume,
  setCategoryMuted,
  setHapticsEnabled,
} from './audioSettings';
import { useAudioSettings } from './useAudioSettings';
import s from '../chrome.module.css';

/** One labelled on/off switch row. */
function SwitchRow({
  label,
  description,
  checked,
  testId,
  onToggle,
}: {
  label: string;
  description: string;
  checked: boolean;
  testId: string;
  onToggle: (next: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      data-testid={testId}
      className={cx(s.artOption, checked && s.artOptionActive)}
      onClick={() => onToggle(!checked)}
    >
      <span className={s.artOptionLabel}>
        {label} — {checked ? 'On' : 'Off'}
      </span>
      <span className={s.artOptionDescription}>{description}</span>
    </button>
  );
}

export function AudioSettingsPanel() {
  const settings = useAudioSettings();
  const supported = hapticsSupported();
  const percent = Math.round(settings.volume * 100);

  return (
    <>
      <section className={s.settingsGroup}>
        <h3 className={s.settingsGroupLabel}>Sound</h3>
        <SwitchRow
          label="Sound effects"
          description="Cues for casting, resolving, damage, deaths, draws, and turn flow."
          checked={!settings.muted}
          testId="audio-muted"
          onToggle={(next) => setAudioMuted(!next)}
        />
        <label className={s.settingsSliderRow}>
          <span className={s.artOptionLabel}>Volume</span>
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={percent}
            disabled={settings.muted}
            data-testid="audio-volume"
            className={s.settingsSlider}
            aria-label="Volume"
            onChange={(event) => setAudioVolume(Number(event.target.value) / 100)}
          />
          <span className={s.artStorage} data-testid="audio-volume-readout">
            {percent}%
          </span>
        </label>
        <p className={s.artNote} data-testid="audio-asset-note">
          This build ships no sound assets yet, so the hooks stay silent whatever you choose here.
          Sound is never required to follow a game — the board and the log always say everything.
        </p>
      </section>

      <section className={s.settingsGroup}>
        <h3 className={s.settingsGroupLabel}>Sound categories</h3>
        <div className={s.settingsCategories}>
          {AUDIO_CUE_CATEGORIES.map((category) => {
            const enabled = !settings.mutedCategories.has(category);
            return (
              <button
                key={category}
                type="button"
                role="switch"
                aria-checked={enabled}
                aria-label={AUDIO_CUE_LABELS[category]}
                data-testid={`audio-category-${category}`}
                className={cx(s.settingsCategory, enabled && s.artOptionActive)}
                onClick={() => setCategoryMuted(category, enabled)}
              >
                {AUDIO_CUE_LABELS[category]}
              </button>
            );
          })}
        </div>
        <p className={s.artNote}>Each category mutes on its own, for both sound and haptics.</p>
      </section>

      <section className={s.settingsGroup}>
        <h3 className={s.settingsGroupLabel}>Haptics</h3>
        <SwitchRow
          label="Vibration"
          description="A short buzz on the same events, on devices that support it."
          checked={settings.haptics}
          testId="audio-haptics"
          onToggle={setHapticsEnabled}
        />
        {!supported && (
          <p className={s.artNote} data-testid="haptics-unsupported">
            This device doesn’t report vibration support, so the setting has no effect here.
          </p>
        )}
      </section>
    </>
  );
}
