/**
 * What an open decision looks like — **one** model, derived once (issue #567,
 * under [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * ## The defect this module exists to remove
 *
 * A decision used to be assembled three times over. `LiveMatchTable` built a
 * `TargetingBanner`/`MultiSelectBanner` for `PromptStrip`, which drew the
 * question; `DecisionSheet` re-read the same session and drew the same question
 * again above its option buttons; and `DecisionPlaque` rendered from a third
 * derivation beside them. For a forced mulligan all three were on screen at once,
 * and the third had no controls at all — with named options in play its confirm
 * was undefined and, being forced, so was its cancel, so it drew a title and
 * nothing else. Three surfaces asking one question, one of which could not answer
 * it.
 *
 * There is now one surface ({@link DecisionArea}) and one derivation (here). The
 * sentence, the progress, the count, the rows, the named choices, and the
 * controls all come out of a single function, so they cannot disagree and cannot
 * be drawn twice.
 *
 * ## What it refuses to decide
 *
 * Everything below is a read of the session the player already built over the
 * server's own `requirements`/`prompts`: labels are `action.label` and slot
 * `prompt` verbatim, counts are the server's `count`, and a named choice is
 * closed only when {@link optionSubmittable} says the slots that option *itself*
 * declares it `requires` are unanswered. No legality, cost, or effect is
 * computed, nothing is ranked, and no word is invented — the one strings this
 * module composes are the progress and count *frames* ("Target 2 of 3"), which
 * are counts of the server's own slots.
 *
 * It is pure and total over its inputs so the whole surface rebuilds from one
 * `GameView` plus the pending prompt, exactly as the hard rule requires.
 */
import type { EntityId, GameView, PlayerId, ValidAction } from '../../protocol';
import {
  activeAttacker as msActiveAttacker,
  activeCandidates as msActiveCandidates,
  activeChosen as msActiveChosen,
  activeSlot as msActiveSlot,
  allSlotsSatisfied,
  hasOptions,
  isLastSlot,
  optionBlockers,
  type MultiSelectSession,
  type MultiSelectSlot,
} from '../multiSelect';
import { activeCandidates as targetCandidates, activeRequirement } from '../targeting';
import type { TargetingSession } from '../targeting';
import { cardNameOf, isOnCanvas } from '../tableView';
import { confirmDisabledReason } from './confirmReason';

/** One row of a list-answered slot: an `order` item or a non-board candidate. */
export interface DecisionRow {
  id: EntityId;
  /** The card's name, or the id when this view carries no card for it. */
  label: string;
  /** Whether the row is currently chosen (`select` rows only). */
  chosen: boolean;
}

/** The rows of a slot answered in the surface rather than on the board. */
export interface DecisionRows {
  /** `select` toggles candidates; `order` arranges them with ↑/↓ per row. */
  mode: 'select' | 'order';
  /** The originating zone, for display context (`"graveyard"`, `"library"`). */
  zone?: string;
  items: DecisionRow[];
}

/** One named choice of an `option` prompt (a mulligan's keep / take another). */
export interface DecisionChoice {
  id: string;
  /** The server's `label`, printed verbatim. */
  label: string;
  /**
   * The server-stated reason this choice is not available yet, or `undefined`
   * when it is. The ONE disablement the language permits (§3.2, D14, GAP-4) is
   * a `PromptOption.requires` whose slots are unanswered, and it is a string so
   * a control cannot be greyed without printing why.
   */
  disabledReason?: string;
}

/**
 * The numeric control of an active `number` slot (issue #554) — the value of X,
 * a count of counters, one share of a divided effect.
 *
 * Every bound is the server's own, carried through untouched; the surface offers
 * exactly `min`..`max` and computes no legality of its own. `value` is the
 * session's current answer, which opens pre-filled at `min` so the decision is
 * submittable without touching the control.
 */
export interface DecisionNumber {
  prompt: string;
  min: number;
  max: number;
  value: number;
}

/** The surface's own model: everything drawn, in the order it is drawn. */
export interface DecisionSurface {
  /** `targeting` auto-submits on the last pick; `multiSelect` confirms. */
  kind: 'targeting' | 'multiSelect';
  /** The action's server `label`, verbatim — the surface's title. */
  title: string;
  /** The active slot's server prompt — the sentence the strip used to carry. */
  prompt: string;
  /** "Target 2 of 3" / "Step 2 of 3", when the action walks more than one slot. */
  progress?: string;
  /** "1 of 2 selected", for a selection slot that has a running count. */
  count?: string;
  /** Seconds left on the server clock, when one is running. */
  deadline?: number;
  /** The rows of a list-answered slot, when the active slot is one. */
  rows?: DecisionRows;
  /** The numeric control, when the active slot is a `number` slot (#554). */
  number?: DecisionNumber;
  /** The named choices of an `option` prompt, when the decision poses one. */
  choices?: DecisionChoice[];
  /** The confirm control's reason it is closed, when it renders closed. */
  confirmDisabledReason?: string;
  /** Whether the surface offers a confirm at all (targeting never does). */
  confirm: boolean;
  /** Whether a later walked slot exists, so "Next" renders. */
  advance: boolean;
  /** Whether a local pick can be retracted, so "Undo" renders. */
  undo: boolean;
  /**
   * Whether the decision may be abandoned. `false` for a decision the view
   * forces (§8/D19): there is no neutral state to return to, so the answer is
   * the only way out, and a cancel that instantly re-opens itself is worse than
   * none (#451).
   */
  cancel: boolean;
}

/** What the plane needs to know about the open decision, derived alongside it. */
export interface DecisionStaging {
  /** Whether any decision session is open at all. */
  selecting: boolean;
  /** Entity candidates the player picks on the board. */
  candidates: EntityId[];
  /** Seat candidates the player picks on a crest (a `defend_` slot). */
  playerCandidates: (EntityId | PlayerId)[];
  /** The active slot's already-chosen ids. */
  chosen: EntityId[];
  /** Whether the active slot asks whom an attacker attacks (#457). */
  assigningDefender: boolean;
  /** The attacker that `defend_` slot is keyed by, when one is active. */
  routedAttacker: EntityId | null;
}

/** The sessions the shell is holding, and whether the view forces the decision. */
export interface DecisionSessions {
  targeting: TargetingSession | null;
  multiSelect: MultiSelectSession | null;
  /** The action the view leaves no way around (`tableView.forcedDecision`). */
  forced: ValidAction | null;
  /** Seconds left on the server clock (`PendingPrompt.deadline`). */
  deadline?: number;
  /** Whether a local target pick can be retracted (§8's one-step UNDO). */
  canRetract?: boolean;
}

/** Both halves of the derivation: what the surface draws, what the plane stages. */
export interface DecisionPresentation {
  surface: DecisionSurface | null;
  staging: DecisionStaging;
}

const IDLE: DecisionStaging = {
  selecting: false,
  candidates: [],
  playerCandidates: [],
  chosen: [],
  assigningDefender: false,
  routedAttacker: null,
};

/**
 * Whether the active slot is answered in the surface's own row list rather than
 * on the board: an `order` slot always is (there is nothing to arrange in
 * place), and a `select_from_zone` is whenever its candidates are not staged —
 * a graveyard or library pick has nothing on the board to click.
 *
 * A `defender` slot never is: its candidates are seats, picked on their crests.
 */
function listAnswered(view: GameView, slot: MultiSelectSlot | null): boolean {
  if (!slot || slot.kind === 'defender' || slot.kind === 'number') return false;
  return slot.kind === 'order' || !slot.candidates.some((id) => isOnCanvas(view, id));
}

/**
 * The numeric control for `slot`, or `undefined` when the active slot is not a
 * `number` one. A `number` slot has no candidates at all, so it is answered by
 * neither the board nor a row list — it is the one slot kind that brings its own
 * control (issue #554).
 *
 * The value is read from the session's chosen answer, which the slot opened
 * pre-filled at the server's minimum; `min` stands in for a missing bound and
 * `max` for a missing ceiling, matching `setActiveNumber`'s own clamp, so a
 * malformed prompt degrades to a single legal value rather than an open range.
 */
function numberFor(slot: MultiSelectSlot | null, chosen: string[]): DecisionNumber | undefined {
  if (!slot || slot.kind !== 'number') return undefined;
  const min = slot.min ?? 0;
  const value = Number(chosen[0] ?? min);
  return {
    prompt: slot.prompt,
    min,
    max: slot.max ?? min,
    value: Number.isFinite(value) ? value : min,
  };
}

/** The rows a list-answered slot shows: the arranged items, or the candidates. */
function rowsFor(view: GameView, session: MultiSelectSession, slot: MultiSelectSlot): DecisionRows {
  const chosen = msActiveChosen(session);
  const ids = slot.kind === 'order' ? chosen : slot.candidates;
  return {
    mode: slot.kind === 'order' ? 'order' : 'select',
    zone: slot.zone,
    items: ids.map((id) => ({ id, label: cardNameOf(view, id), chosen: chosen.includes(id) })),
  };
}

/** A slot's running count, for the kinds where "how many so far" means anything. */
function countFor(slot: MultiSelectSlot | null, chosen: number): string | undefined {
  if (!slot || (slot.kind !== 'count' && slot.kind !== 'subset')) return undefined;
  return slot.count === undefined ? `${chosen} selected` : `${chosen} of ${slot.count} selected`;
}

/**
 * Derive the one decision presentation for a view and the sessions open over it.
 *
 * Returns a `null` surface when nothing is being decided — the surface is
 * contextual and otherwise absent, which is ADR 0032's rule, and is why there is
 * no neutral form of it to keep in sync with the phase plaque.
 */
export function deriveDecision(view: GameView, sessions: DecisionSessions): DecisionPresentation {
  const { targeting, multiSelect, forced } = sessions;

  if (multiSelect) {
    const slot = msActiveSlot(multiSelect);
    const defender = slot?.kind === 'defender';
    const rows = listAnswered(view, slot) && slot ? rowsFor(view, multiSelect, slot) : undefined;
    const options = hasOptions(multiSelect) ? multiSelect.options[0] : undefined;
    const chosen = msActiveChosen(multiSelect);
    // A decision that poses named choices is answered by pressing one of them,
    // so it has no separate confirm; every other multi-select does.
    const confirm = options === undefined;

    return {
      surface: {
        kind: 'multiSelect',
        title: multiSelect.action.label,
        prompt: slot?.prompt ?? options?.prompt ?? '',
        progress:
          multiSelect.slots.length > 1
            ? `Step ${multiSelect.active + 1} of ${multiSelect.slots.length}`
            : undefined,
        count: countFor(slot, chosen.length),
        deadline: sessions.deadline,
        rows,
        number: numberFor(slot, chosen),
        choices: options?.options.map((option) => {
          // The server's own words for why this choice is closed: the prompts of
          // the slots it is still waiting on. Never a client-invented reason —
          // without one the control may not render disabled at all (D14).
          const blocked = optionBlockers(multiSelect, option);
          return {
            id: option.id,
            label: option.label,
            disabledReason: confirmDisabledReason(
              blocked.length === 0,
              blocked.map((slot) => slot.prompt).join('; '),
            ),
          };
        }),
        confirm,
        confirmDisabledReason: confirm
          ? confirmDisabledReason(allSlotsSatisfied(multiSelect), slot?.prompt)
          : undefined,
        advance: multiSelect.slots.length > 1 && !isLastSlot(multiSelect),
        undo: false,
        cancel: forced === null,
      },
      staging: {
        selecting: true,
        // Rows and crests take their picks out of the board's hands: a slot
        // answered in the list, or by a seat, lights no board candidate.
        candidates: rows !== undefined || defender ? [] : msActiveCandidates(multiSelect),
        playerCandidates: defender ? msActiveCandidates(multiSelect) : [],
        chosen,
        assigningDefender: defender,
        routedAttacker: defender ? msActiveAttacker(multiSelect) : null,
      },
    };
  }

  if (targeting) {
    const requirement = activeRequirement(targeting);
    const total = targeting.action.requirements?.length ?? 0;
    const candidates = targetCandidates(targeting);
    return {
      surface: requirement
        ? {
            kind: 'targeting',
            title: targeting.action.label,
            prompt: requirement.prompt,
            progress: total > 1 ? `Target ${targeting.picks.length + 1} of ${total}` : undefined,
            deadline: sessions.deadline,
            // §4.2 rule 2: the last pick submits, so there is nothing to confirm.
            confirm: false,
            advance: false,
            undo: sessions.canRetract === true,
            cancel: forced === null,
          }
        : null,
      staging: {
        selecting: true,
        candidates,
        // A seat is a candidate exactly when the server listed it as one.
        playerCandidates: candidates.filter(
          (id) => id === view.you || view.opponents.some((seat) => seat.player_id === id),
        ),
        chosen: [],
        assigningDefender: false,
        routedAttacker: null,
      },
    };
  }

  return { surface: null, staging: IDLE };
}
