/**
 * The playable table: the single React tree that reconstructs the whole UI from
 * the store's latest {@link GameView} (plus its derived pending prompt).
 *
 * - Pixi canvas draws battlefield bands + hand (ADR 0003).
 * - DOM islands render the prompt banner, player tiles, the interactive overlay,
 *   and the action bar.
 * - Interactivity is driven entirely by `valid_actions[]`; choosing an action
 *   sends `ChooseAction` through the store, and the UI then rebuilds purely from
 *   the resulting GameView. The only client-side state is ephemeral selection
 *   (nothing load-bearing across messages — the reconnect/replay invariant).
 *
 * Targeting mode (ADR 0009 §Client): choosing an action that carries
 * `requirements` enters a data-driven targeting flow. The client walks the
 * requirement slots as a prompt queue, offering only the server-listed candidates
 * (highlighted; everything else dimmed and inert), then submits the whole answer
 * atomically with the action's content-binding token — one `ChooseAction`, never
 * a multi-message handshake. The in-progress selection is ephemeral and discarded
 * on the next view, so the UI stays reconstructable from one GameView + prompt.
 */
import { useEffect, useMemo, useState } from 'react';
import type { EntityId, ValidAction } from '../protocol';
import { selectPendingPrompt, useGameStore } from '../store';
import { publishScene } from '../testHooks';
import { ActionBar } from './ActionBar';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { EntityOverlay } from './EntityOverlay';
import { GameOverOverlay } from './GameOverOverlay';
import { PlayerTiles } from './PlayerTiles';
import { PromptBanner } from './PromptBanner';
import { StackPanel } from './StackPanel';
import {
  buildTableScene,
  DEFAULT_VIEWPORT_WIDTH,
  type RenderedCard,
  type TableScene,
  type TargetingScene,
} from './scene';
import {
  activeCandidates,
  activeRequirement,
  assembleTargets,
  beginTargeting,
  pick,
  requiresTargets,
  type TargetingSession,
} from './targeting';
import {
  activeCandidates as msActiveCandidates,
  activeChosen as msActiveChosen,
  activeSlot as msActiveSlot,
  advance as msAdvance,
  allSlotsSatisfied,
  assembleChoices,
  beginMultiSelect,
  hasOptions,
  isLastSlot,
  isMultiSelect,
  optionsSubmittable,
  toggle as msToggle,
  type MultiSelectSession,
} from './multiSelect';
import { boardWrap, button, main, muted, waitingBar } from './styles';

/**
 * The current logical width the board may wrap within, tracking the window so the
 * battlefield re-wraps on resize (the layout stays a pure function — this only
 * feeds it the live budget). Falls back to {@link DEFAULT_VIEWPORT_WIDTH} where
 * there is no `window` (SSR/tests).
 */
function useViewportWidth(): number {
  const [width, setWidth] = useState(() =>
    typeof window === 'undefined' ? DEFAULT_VIEWPORT_WIDTH : window.innerWidth,
  );
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onResize = (): void => setWidth(window.innerWidth);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return width;
}

/** Find a rendered card anywhere in the scene by entity id. */
function findCard(scene: TableScene, id: EntityId | null): RenderedCard | undefined {
  if (id === null) return undefined;
  for (const band of scene.bands) {
    const hit = band.cards.find((card) => card.entityId === id);
    if (hit) return hit;
  }
  return scene.hand.find((card) => card.entityId === id);
}

export function Table() {
  const view = useGameStore((state) => state.view);
  const choose = useGameStore((state) => state.choose);
  const disconnect = useGameStore((state) => state.disconnect);
  const [selectedId, setSelectedId] = useState<EntityId | null>(null);
  const [targeting, setTargeting] = useState<TargetingSession | null>(null);
  const [multiSelect, setMultiSelect] = useState<MultiSelectSession | null>(null);

  // A fresh view supersedes any in-progress targeting or multi-select: the answer
  // either landed (server's response) or is now stale — most importantly, a changed
  // content-binding `token` invalidates the pending selection. Discarding both here
  // is what keeps the whole selection UI reconstructable from one GameView + prompt
  // (no load-bearing state across messages).
  useEffect(() => {
    setTargeting(null);
    setMultiSelect(null);
  }, [view]);

  const viewportWidth = useViewportWidth();
  const prompt = useMemo(() => selectPendingPrompt(view), [view]);
  // The server names the receiver directly in `view.you`; an older server may
  // omit it (empty), which we treat as "unknown".
  const localId = view?.you || undefined;
  const scene = useMemo(() => {
    if (!view) return null;
    // In targeting / multi-select mode the active slot's server candidates drive
    // highlight/dim; a multi-select also marks the already-chosen candidates.
    // Entity selection is suppressed so the only interaction is picking candidates.
    let targetingScene: TargetingScene | undefined;
    if (multiSelect) {
      targetingScene = {
        candidates: msActiveCandidates(multiSelect),
        selected: msActiveChosen(multiSelect),
      };
    } else if (targeting) {
      const activeReq = activeRequirement(targeting);
      if (activeReq) targetingScene = { candidates: activeReq.candidates ?? [] };
    }
    const sel = targeting || multiSelect ? undefined : (selectedId ?? undefined);
    return buildTableScene(view, sel, viewportWidth, targetingScene);
  }, [view, selectedId, viewportWidth, targeting, multiSelect]);

  // Publish the derived scene on the test-only window hook (ADR 0011). A no-op in
  // production builds; the e2e suite reads it to assert what the canvas draws.
  useEffect(() => {
    publishScene(scene);
  }, [scene]);

  if (!view || !scene) {
    // Socket is open (App only mounts the table then) but no frame has arrived
    // yet. Show a live status plus a Disconnect action so this is never a dead
    // screen; it resolves the instant the first GameView lands.
    return (
      <main style={main} data-testid="table-waiting">
        <div style={waitingBar}>
          <span style={muted}>Connected — waiting for first game state…</span>
          <button type="button" style={button} onClick={disconnect} data-testid="disconnect-button">
            Disconnect
          </button>
        </div>
      </main>
    );
  }

  // Game over (issue #141): a terminal view carries `result`. The whole screen is
  // pure render of that latest view — the DOM overlay names the verdict/reason and
  // the interactive prompt/action UI is suppressed (the server sends no actions
  // once the game is over). The final board + tiles stay visible beneath, read-only
  // (no EntityOverlay, so nothing is selectable/targetable). Nothing is load-bearing
  // across messages, so a reconnect that replays this view shows the same screen.
  if (view.result) {
    return (
      <main style={main} data-testid="table-game-over">
        <PlayerTiles view={view} localId={localId} />
        <div style={boardWrap(scene.width, scene.height)}>
          <BattlefieldCanvas scene={scene} />
        </div>
        <GameOverOverlay result={view.result} you={view.you} />
      </main>
    );
  }

  // The selected entity's actions come straight from what the server offered —
  // never recomputed. Suppressed during targeting / multi-select (entity selection
  // is inactive then; the only interaction is picking candidates).
  const selecting = targeting !== null || multiSelect !== null;
  const selectedActions =
    selectedId === null || selecting
      ? []
      : (prompt?.subjectActions ?? []).filter((action) => action.subject?.includes(selectedId));
  const selectedCard = findCard(scene, selectedId);

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

  // Submit an option decision (mulligan keep/take-another) together with the
  // current per-slot selection (e.g. the bottomed cards) in one atomic answer. The
  // rich option/order prompt UX is issue #157; here an option is a submit trigger.
  const chooseOption = (optionId: string): void => {
    if (!multiSelect) return;
    const optionSlot = multiSelect.options[0];
    const extra = optionSlot ? [{ slot: optionSlot.slot, chosen: [optionId] }] : [];
    choose(multiSelect.action, assembleChoices(multiSelect, extra));
    setMultiSelect(null);
  };

  const cancelTargeting = (): void => setTargeting(null);
  const cancelMultiSelect = (): void => setMultiSelect(null);
  const toggleSelect = (id: EntityId): void =>
    setSelectedId((current) => (current === id ? null : id));

  const activeReq = targeting ? activeRequirement(targeting) : null;
  const targetingBanner =
    targeting && activeReq
      ? {
          label: targeting.action.label,
          prompt: activeReq.prompt,
          step: targeting.picks.length + 1,
          total: targeting.action.requirements?.length ?? 0,
        }
      : null;

  const msSlot = multiSelect ? msActiveSlot(multiSelect) : null;
  const multiSelectBanner =
    multiSelect && (msSlot || hasOptions(multiSelect))
      ? {
          label: multiSelect.action.label,
          prompt: msSlot?.prompt ?? multiSelect.options[0]?.prompt ?? '',
          step: multiSelect.active + 1,
          total: multiSelect.slots.length,
          chosen: msActiveChosen(multiSelect).length,
          required: msSlot?.kind === 'count' ? msSlot.count : undefined,
        }
      : null;

  const multiSelectControls = multiSelect
    ? {
        canAdvance: multiSelect.slots.length > 1 && !isLastSlot(multiSelect),
        onAdvance: advanceSlot,
        confirm: hasOptions(multiSelect)
          ? undefined
          : {
              label: 'Confirm',
              enabled: allSlotsSatisfied(multiSelect),
              onConfirm: confirmMultiSelect,
            },
        options: hasOptions(multiSelect)
          ? (multiSelect.options[0]?.options ?? []).map((option) => ({
              id: option.id,
              label: option.label,
            }))
          : undefined,
        optionsEnabled: optionsSubmittable(multiSelect),
        onOption: chooseOption,
        onCancel: cancelMultiSelect,
      }
    : undefined;

  return (
    <main style={main}>
      <PromptBanner
        view={view}
        prompt={prompt}
        targeting={targetingBanner}
        multiSelect={multiSelectBanner}
      />
      <PlayerTiles
        view={view}
        localId={localId}
        targeting={
          targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
        }
      />
      <StackPanel
        view={view}
        targeting={
          targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
        }
      />
      <div style={boardWrap(scene.width, scene.height)}>
        <BattlefieldCanvas scene={scene} />
        <EntityOverlay
          scene={scene}
          selectedId={selectedId}
          targeting={selecting}
          multiSelect={multiSelect !== null}
          onSelect={toggleSelect}
          onChoose={fire}
          onPickTarget={multiSelect ? toggleCandidate : pickTarget}
        />
      </div>
      <ActionBar
        globalActions={selecting ? [] : (prompt?.globalActions ?? [])}
        selectedActions={selectedActions}
        selectedName={selectedCard?.name}
        onChoose={fire}
        onCancelTargeting={targeting ? cancelTargeting : undefined}
        multiSelect={multiSelectControls}
      />
    </main>
  );
}
