/**
 * The table's ephemeral selection state machine and action-submission wiring
 * (issues #009/#157/#347, ADR 0025). Owns the only client-side state the table
 * keeps — the in-progress selection (`selectedId`), targeting session, and
 * multi-select session — none of it load-bearing across messages (the table's
 * view-reset effect clears the sessions on every fresh view, keeping the whole UI
 * reconstructable from one GameView + prompt).
 *
 * Every handler here submits an action the server already offered — the gestures
 * only change how an action is reached, never what is legal (AGENTS.md hard rule).
 * The scene-dependent direct-activation gesture stays in the composition root
 * because it reads the rendered scene; everything a submission needs is either
 * ephemeral state owned here or the injected `choose`.
 */
import { useState } from 'react';
import type { EntityId, TargetChoice, ValidAction } from '../../protocol';
import {
  assembleTargets,
  beginTargeting,
  pick,
  requiresTargets,
  type TargetingSession,
} from '../targeting';
import {
  advance as msAdvance,
  assembleChoices,
  beginMultiSelect,
  isMultiSelect,
  moveInActiveSlot as msMove,
  toggle as msToggle,
  type MultiSelectSession,
} from '../multiSelect';

type ChooseFn = (action: ValidAction, targets?: TargetChoice[]) => void;

export interface TableInteractions {
  selectedId: EntityId | null;
  setSelectedId: React.Dispatch<React.SetStateAction<EntityId | null>>;
  targeting: TargetingSession | null;
  setTargeting: React.Dispatch<React.SetStateAction<TargetingSession | null>>;
  multiSelect: MultiSelectSession | null;
  setMultiSelect: React.Dispatch<React.SetStateAction<MultiSelectSession | null>>;
  fire: (action: ValidAction) => void;
  fireOnTarget: (action: ValidAction, target: EntityId) => void;
  pickTarget: (entityId: EntityId) => void;
  toggleCandidate: (entityId: EntityId) => void;
  pickDefender: (playerId: EntityId) => void;
  advanceSlot: () => void;
  confirmMultiSelect: () => void;
  moveOrder: (entityId: EntityId, direction: -1 | 1) => void;
  chooseOption: (optionId: string) => void;
  cancelTargeting: () => void;
  cancelMultiSelect: () => void;
}

export function useTableInteractions(choose: ChooseFn): TableInteractions {
  const [selectedId, setSelectedId] = useState<EntityId | null>(null);
  const [targeting, setTargeting] = useState<TargetingSession | null>(null);
  const [multiSelect, setMultiSelect] = useState<MultiSelectSession | null>(null);

  // Fire an action: a multi-select declaration (combat / bottoming) opens the
  // toggle-and-confirm flow; a single-target action opens targeting mode; a plain
  // action is submitted immediately (token echoed, no targets).
  const fire = (action: ValidAction): void => {
    if (isMultiSelect(action)) {
      setSelectedId(null);
      setTargeting(null);
      setMultiSelect(beginMultiSelect(action));
      return;
    }
    if (requiresTargets(action)) {
      setSelectedId(null);
      setMultiSelect(null);
      setTargeting(beginTargeting(action));
      return;
    }
    choose(action);
    setSelectedId(null);
  };

  // Fire a targeted action with its first target already chosen — the drag-to-play
  // drop on a candidate (blueprint §Interaction model): cast + first target in one
  // gesture. A single-slot spell submits atomically right here; a multi-slot one
  // continues in the ordinary targeting flow for its remaining slots. The dropped
  // target is always one of the server-enumerated slot-0 candidates (the overlay
  // only offers those), and `pick` re-checks it against the session's active slot.
  const fireOnTarget = (action: ValidAction, target: EntityId): void => {
    if (!requiresTargets(action)) {
      fire(action);
      return;
    }
    setSelectedId(null);
    setMultiSelect(null);
    const advanced = pick(beginTargeting(action), target);
    const targets = assembleTargets(advanced);
    if (targets !== null) {
      choose(advanced.action, targets);
      setTargeting(null);
    } else {
      setTargeting(advanced);
    }
  };

  // Pick a target for the active slot. When the last slot is filled, assemble and
  // submit the whole answer atomically (action token + one choice per slot).
  const pickTarget = (entityId: EntityId): void => {
    if (!targeting) return;
    const advanced = pick(targeting, entityId);
    const targets = assembleTargets(advanced);
    if (targets !== null) {
      choose(advanced.action, targets);
      setTargeting(null);
    } else {
      setTargeting(advanced);
    }
  };

  // Toggle a candidate into (or out of) the active multi-select slot. Nothing is
  // submitted until the player confirms (or picks an option).
  const toggleCandidate = (entityId: EntityId): void => {
    if (!multiSelect) return;
    setMultiSelect(msToggle(multiSelect, entityId));
  };

  // Assign a defending player to the attacker of the active `defender` slot (issue
  // #347), then advance to the next declared attacker awaiting a target. A defender is
  // a single choice, so the pick replaces any prior one; after the last attacker the
  // advance clamps and Confirm submits the whole declaration atomically.
  const pickDefender = (playerId: EntityId): void => {
    if (!multiSelect) return;
    setMultiSelect((prev) => (prev ? msAdvance(msToggle(prev, playerId)) : prev));
  };

  // Advance to the next walked slot (per-attacker blocker assignment).
  const advanceSlot = (): void => {
    if (!multiSelect) return;
    setMultiSelect(msAdvance(multiSelect));
  };

  // Confirm the whole selection atomically (used when there is no option prompt).
  const confirmMultiSelect = (): void => {
    if (!multiSelect) return;
    choose(multiSelect.action, assembleChoices(multiSelect));
    setMultiSelect(null);
  };

  // Move an item one step within the active `order` slot (issue #157). Nothing is
  // submitted until the player confirms — reordering only edits the pending answer.
  const moveOrder = (entityId: EntityId, direction: -1 | 1): void => {
    if (!multiSelect) return;
    setMultiSelect(msMove(multiSelect, entityId, direction));
  };

  // Submit an option decision (the sheet's modal picker, e.g. mulligan keep/take-
  // another) together with the current per-slot selection (e.g. the bottomed cards)
  // in one atomic answer, keyed by the option slot the server posed.
  const chooseOption = (optionId: string): void => {
    if (!multiSelect) return;
    const optionSlot = multiSelect.options[0];
    const extra = optionSlot ? [{ slot: optionSlot.slot, chosen: [optionId] }] : [];
    choose(multiSelect.action, assembleChoices(multiSelect, extra));
    setMultiSelect(null);
  };

  const cancelTargeting = (): void => setTargeting(null);
  const cancelMultiSelect = (): void => setMultiSelect(null);

  return {
    selectedId,
    setSelectedId,
    targeting,
    setTargeting,
    multiSelect,
    setMultiSelect,
    fire,
    fireOnTarget,
    pickTarget,
    toggleCandidate,
    pickDefender,
    advanceSlot,
    confirmMultiSelect,
    moveOrder,
    chooseOption,
    cancelTargeting,
    cancelMultiSelect,
  };
}
