/**
 * The keyboard shortcut reference overlay (issue #266, React DOM per ADR 0003).
 *
 * A lightweight, discoverable list of the live key bindings, toggled with `?`.
 * Every binding maps to an interaction the pointer path already has — the keyboard
 * introduces no new game semantics (AGENTS.md hard rule). Bindings that have no
 * matching action right now are shown dimmed as "unavailable", so the reference
 * reflects the current view rather than a static cheat-sheet.
 */
import {
  shortcutBackdrop,
  shortcutKey,
  shortcutPanel,
  shortcutRow,
  shortcutRowOff,
  shortcutTitle,
} from './styles';

/** One binding row: a stable id, the key(s), what it does, and whether it applies. */
export interface Binding {
  /** Stable id for the row's testid (the visible `keys` may contain symbols). */
  id: string;
  keys: string;
  description: string;
  available: boolean;
}

interface Props {
  bindings: Binding[];
  onClose: () => void;
}

export function ShortcutHelp({ bindings, onClose }: Props) {
  return (
    <div
      data-testid="shortcut-help-backdrop"
      style={shortcutBackdrop}
      onClick={onClose}
      role="presentation"
    >
      <div
        data-testid="shortcut-help"
        style={shortcutPanel}
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 style={shortcutTitle}>Keyboard shortcuts</h2>
        <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
          {bindings.map((binding) => (
            <li
              key={binding.id}
              data-testid={`shortcut-${binding.id}`}
              data-available={binding.available || undefined}
              style={binding.available ? shortcutRow : { ...shortcutRow, ...shortcutRowOff }}
            >
              <kbd style={shortcutKey}>{binding.keys}</kbd>
              <span>{binding.description}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
