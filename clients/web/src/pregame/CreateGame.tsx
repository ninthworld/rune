/**
 * The create-table setup (issue #546; the approved
 * `docs/ui-concepts/rune-pregame-create-game.jpg` baseline).
 *
 * A **destination, not an inline form**: the lobby no longer embeds it, because
 * a create form living permanently beside the open-games list is exactly the
 * competing-secondary-path problem the baseline removes. It is one plaque on the
 * same arena, over the same environment, reached from CREATE GAME and left by
 * Cancel — the shared world never changes.
 *
 * Only the essential decisions are on the face; everything else is behind
 * Advanced.
 *
 * ## Where the baseline and the shipped contract disagree
 *
 * The baseline draws four editable decisions: table name, format, seats, and
 * visibility. The lobby protocol carries **two**: `RoomConfig` is
 * `{ seats, game_setup }` and nothing else (`protocol/lobby.ts`), the directory
 * summary has no name field, and every room in the registry is listed publicly.
 * A client may not invent the other two — a table name it made up would exist on
 * this screen and nowhere else, and a "Private" toggle would be a claim about
 * server behaviour the server never made.
 *
 * So the two uncarried rows are drawn as the **facts they are** — a table is
 * named by its format, and every table on this server is public — rather than as
 * controls that quietly do nothing. Advanced says so in as many words. Making
 * them real is a protocol change and is out of scope for a visual pass; this is
 * reported rather than papered over.
 */
import { useEffect, useRef, useState } from 'react';
import { createRoomCommand, type CatalogFormat } from '../protocol';
import { useGameStore } from '../store';
import { ControlButton } from '../table/controls';
import { Plaque } from './MenuFrame';
import { GAME_SETUPS, SEAT_COUNTS, setupLabel } from './gameSetups';
import p from './styles';

/** The protocol's own inclusive seat range (`RoomConfig.seats` is `2..=8`). */
const MIN_SEATS = SEAT_COUNTS[0];
const MAX_SEATS = SEAT_COUNTS[SEAT_COUNTS.length - 1]!;

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
  /** Pre-fill raised from elsewhere in the lobby (Play again). */
  prefill?: CreatePrefill | null;
  /**
   * Leave the setup and return to open games. The setup is only ever mounted
   * while the server advertises `create_room`, so it derives no legality itself.
   */
  onCancel: () => void;
}

export function CreateGame({ prefill, onCancel }: CreateGameProps) {
  const sendLobby = useGameStore((state) => state.sendLobby);
  const catalog = useGameStore((state) => state.catalog);
  const [setupId, setSetupId] = useState(prefill?.setupId ?? GAME_SETUPS[0]!.id);
  const [seats, setSeats] = useState<number>(prefill?.seats ?? GAME_SETUPS[0]!.seats);
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

  return (
    <Plaque className={p.formPlaque} faceClass={p.formFace} testId="create-room">
      <h2 className={p.formTitle} data-place-heading tabIndex={-1} ref={headingRef}>
        Create Game
      </h2>

      {/* A room is named by its format on the wire; see the file header. */}
      <div className={p.field}>
        <span className={p.fieldLabel}>Table name</span>
        <span className={p.identityName} data-testid="create-table-name">
          {setupLabel(setupId)}
        </span>
      </div>

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

      {/* Every room in the registry is listed in the public directory. */}
      <div className={p.field}>
        <span className={p.fieldLabel}>Visibility</span>
        <span className={p.identityName} data-testid="create-visibility">
          Public
        </span>
      </div>

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
          <span className={p.muted}>
            This server’s lobby carries a seat count and a format and nothing else: tables are named
            by their format, every table is listed publicly, and variant rules and permissions are
            not configurable yet.
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
            label="Create Table"
            onPress={() => sendLobby(createRoomCommand({ seats, game_setup: setupId }))}
            testId="create-room-button"
          />
        </span>
      </div>
    </Plaque>
  );
}
