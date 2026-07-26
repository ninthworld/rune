/**
 * The create-table setup, and the host's **Edit Table** (issue #546; the approved
 * `docs/ui-concepts/rune-pregame-create-game.jpg` baseline).
 *
 * A **destination, not an inline form**: the lobby no longer embeds it, because
 * a create form living permanently beside the open-games list is exactly the
 * competing-secondary-path problem the baseline removes. It is one plaque on the
 * same arena, over the same environment, reached from CREATE GAME and left by
 * Cancel — the shared world never changes.
 *
 * **One surface serves both creating and editing.** Passing {@link CreateGameProps.editing}
 * a room's current config is what turns it into Edit Table: same fields, same
 * layout, same Advanced disclosure, different title and different command. There
 * is deliberately no second edit interface, because a table's configuration is one
 * set of decisions and a player should not have to learn it twice — and because
 * `update_room` carries a *whole* `RoomConfig` (ADR 0012's #546 amendment), which
 * is exactly the shape this form already produces.
 *
 * Only the essential decisions are on the face — name, format, seats, visibility,
 * the four the baseline draws; everything else is behind Advanced.
 *
 * Nothing here computes legality. The seat stepper is clamped to the protocol's own
 * `2..=8` range purely so the control has ends, but which counts a *format* allows,
 * whether a name is acceptable, and whether a shrink would evict a seated player are
 * all the server's to decide: it answers with the current view plus a structured
 * `lobby_error` the place renders. The surface is mounted only while the server
 * advertises the command it sends.
 */
import { useEffect, useRef, useState } from 'react';
import {
  createRoomCommand,
  updateRoomCommand,
  type CatalogFormat,
  type RoomConfig,
  type RoomVisibility,
} from '../protocol';
import { useGameStore } from '../store';
import { ControlButton } from '../table/controls';
import { Plaque } from './MenuFrame';
import { GAME_SETUPS, SEAT_COUNTS, setupLabel } from './gameSetups';
import p from './styles';

/** The protocol's own inclusive seat range (`RoomConfig.seats` is `2..=8`). */
const MIN_SEATS = SEAT_COUNTS[0];
const MAX_SEATS = SEAT_COUNTS[SEAT_COUNTS.length - 1]!;

/** The visibility choices the protocol defines, with the words the UI shows. */
const VISIBILITIES: readonly { readonly id: RoomVisibility; readonly label: string }[] = [
  { id: 'public', label: 'Public — listed in Open Games' },
  { id: 'private', label: 'Private — reachable only by its id' },
];

/** A pre-fill for the setup, raised by the last-match ribbon's Play again. */
export interface CreatePrefill {
  setupId?: string;
  seats?: number;
  /** Makes a repeat of the same request observable. */
  nonce: number;
}

/** The advertised format's public deck rules, as display-only lines. */
function formatRuleLines(format: CatalogFormat): string[] {
  const lines = [`Seats ${format.min_seats}–${format.max_seats}`];
  lines.push(
    format.max_deck_size === undefined
      ? `At least ${format.min_deck_size} cards`
      : format.min_deck_size === format.max_deck_size
        ? `Exactly ${format.min_deck_size} cards`
        : `${format.min_deck_size}–${format.max_deck_size} cards`,
  );
  lines.push(
    format.max_copies === undefined
      ? 'No copy limit'
      : `Up to ${format.max_copies} copies of a card`,
  );
  if (format.requires_commander) lines.push('A commander must be designated');
  return lines;
}

export interface CreateGameProps {
  /** Pre-fill raised from elsewhere in the lobby (Play again). Create only. */
  prefill?: CreatePrefill | null;
  /**
   * The existing table's config when the **host** reopened this surface as Edit
   * Table. Its presence is the whole of the difference between the two modes: the
   * fields seed from it and the primary sends `update_room` instead of
   * `create_room`.
   */
  editing?: RoomConfig;
  /**
   * Leave the setup and return to where it was opened from. The setup is only ever
   * mounted while the server advertises the command it would send, so it derives no
   * legality itself.
   */
  onCancel: () => void;
}

export function CreateGame({ prefill, editing, onCancel }: CreateGameProps) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const catalog = useGameStore((state) => state.catalog);
  const [setupId, setSetupId] = useState(
    editing?.game_setup ?? prefill?.setupId ?? GAME_SETUPS[0]!.id,
  );
  const [seats, setSeats] = useState<number>(
    editing?.seats ?? prefill?.seats ?? GAME_SETUPS[0]!.seats,
  );
  const [name, setName] = useState(editing?.name ?? '');
  const [visibility, setVisibility] = useState<RoomVisibility>(editing?.visibility ?? 'public');
  const headingRef = useRef<HTMLHeadingElement>(null);
  const lastNonce = useRef(prefill?.nonce ?? 0);

  // Honour a later pre-fill and move focus to the setup, so the keyboard follows
  // the eye. Ephemeral form input: nothing here is load-bearing.
  useEffect(() => {
    if (prefill == null || prefill.nonce === lastNonce.current) return;
    lastNonce.current = prefill.nonce;
    if (prefill.setupId !== undefined) setSetupId(prefill.setupId);
    if (prefill.seats !== undefined) setSeats(prefill.seats);
    headingRef.current?.focus();
  }, [prefill]);

  const format = catalog?.formats.find((entry) => entry.game_setup === setupId);
  const stepSeats = (delta: number): void =>
    setSeats((current) => Math.min(MAX_SEATS, Math.max(MIN_SEATS, current + delta)));

  // The whole config this form describes — the shape both commands carry. A blank
  // name is sent as *absent*, matching the server's own normalization, so an
  // unnamed table stays labelled by its format rather than by an empty string.
  const submit = (): void => {
    const trimmed = name.trim();
    const config: RoomConfig = { seats, game_setup: setupId, visibility };
    if (trimmed.length > 0) config.name = trimmed;
    sendLobby(editing === undefined ? createRoomCommand(config) : updateRoomCommand(config));
    // Editing stays on the same room, so the surface hands the arena back; the next
    // `LobbyView` is what says whether the change took. Creating lands in a new room,
    // which swaps the place out from under this component on its own.
    if (editing !== undefined) onCancel();
  };

  return (
    <Plaque className={p.formPlaque} faceClass={p.formFace} testId="create-room">
      <h2 className={p.formTitle} data-place-heading tabIndex={-1} ref={headingRef}>
        {editing === undefined ? 'Create Game' : 'Edit Table'}
      </h2>

      <label className={p.field}>
        <span className={p.fieldLabel}>Table name</span>
        <input
          className={p.input}
          type="text"
          autoComplete="off"
          spellCheck={false}
          maxLength={32}
          // The placeholder is the honest default: leave it blank and the table is
          // labelled by its format, exactly as every table was before names existed.
          placeholder={setupLabel(setupId)}
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
          }}
          data-testid="create-table-name"
          aria-label="Table name"
        />
      </label>

      <label className={p.field}>
        <span className={p.fieldLabel}>Format</span>
        <select
          className={p.select}
          value={setupId}
          onChange={(event) => {
            const next = event.target.value;
            setSetupId(next);
            // Picking a format pre-fills the seat count it is designed for.
            const designed = GAME_SETUPS.find((option) => option.id === next)?.seats;
            if (designed !== undefined) setSeats(designed);
          }}
          data-testid="game-setup-select"
        >
          {GAME_SETUPS.map((option) => (
            <option key={option.id} value={option.id} data-testid={`game-setup-${option.id}`}>
              {option.label}
            </option>
          ))}
        </select>
      </label>

      <div className={p.field}>
        <span className={p.fieldLabel} id="create-seats-label">
          Seats
        </span>
        <div className={p.stepper} role="group" aria-labelledby="create-seats-label">
          <button
            type="button"
            className={p.stepperButton}
            onClick={() => stepSeats(-1)}
            aria-label="One fewer seat"
            data-testid="seat-count-decrease"
          >
            −
          </button>
          <span className={p.stepperValue} aria-live="polite" data-testid="seat-count">
            {seats}
          </span>
          <button
            type="button"
            className={p.stepperButton}
            onClick={() => stepSeats(1)}
            aria-label="One more seat"
            data-testid="seat-count-increase"
          >
            +
          </button>
        </div>
      </div>

      <label className={p.field}>
        <span className={p.fieldLabel}>Visibility</span>
        <select
          className={p.select}
          value={visibility}
          onChange={(event) => setVisibility(event.target.value as RoomVisibility)}
          data-testid="create-visibility"
        >
          {VISIBILITIES.map((option) => (
            <option key={option.id} value={option.id} data-testid={`visibility-${option.id}`}>
              {option.label}
            </option>
          ))}
        </select>
      </label>

      <details className={p.disclosure} data-testid="create-advanced">
        <summary className={p.disclosureSummary}>Advanced</summary>
        <div className={p.disclosureBody}>
          {format !== undefined ? (
            <ul className={p.formRules} data-testid="create-advanced-rules">
              {formatRuleLines(format).map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          ) : (
            <span className={p.muted}>This server has not advertised the format’s rules.</span>
          )}
          {/* The variant rules and permissions the baseline files under Advanced are
              the format's own, and they are the server's to state — a room carries a
              format id, not a rules override. Said plainly rather than drawn as
              controls that would change nothing. */}
          <span className={p.muted}>
            Variant rules come with the format and are not overridable per table, and this server
            carries no per-table permissions. A private table is not listed in Open Games; anyone
            you send its id to can still join it.
          </span>
        </div>
      </details>

      <div className={p.formActions}>
        <span className={p.fit}>
          <ControlButton
            variant="secondary"
            label="Cancel"
            onPress={onCancel}
            testId="create-room-cancel"
          />
        </span>
        {/* The one blue primary of this state (§4.1). */}
        <span className={p.fit}>
          <ControlButton
            variant="primaryCompact"
            label={editing === undefined ? 'Create Table' : 'Save Table'}
            onPress={submit}
            testId="create-room-button"
          />
        </span>
      </div>
    </Plaque>
  );
}
