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
import { ActionBar } from './ActionBar';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { EntityOverlay } from './EntityOverlay';
import { PlayerTiles } from './PlayerTiles';
import { PromptBanner } from './PromptBanner';
import {
  buildTableScene,
  DEFAULT_VIEWPORT_WIDTH,
  type RenderedCard,
  type TableScene,
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
import { boardWrap, main, muted } from './styles';

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
  const [selectedId, setSelectedId] = useState<EntityId | null>(null);
  const [targeting, setTargeting] = useState<TargetingSession | null>(null);

  // A fresh view supersedes any in-progress targeting: the answer either landed
  // (server's response) or is now stale. Discarding it here is what keeps the
  // targeting UI reconstructable from one GameView + prompt (no load-bearing
  // state across messages).
  useEffect(() => {
    setTargeting(null);
  }, [view]);

  const viewportWidth = useViewportWidth();
  const prompt = useMemo(() => selectPendingPrompt(view), [view]);
  // The server names the receiver directly in `view.you`; an older server may
  // omit it (empty), which we treat as "unknown".
  const localId = view?.you || undefined;
  const scene = useMemo(() => {
    if (!view) return null;
    // In targeting mode the active slot's server candidates drive highlight/dim;
    // selection is suppressed so the only interaction is picking a target.
    const activeReq = targeting ? activeRequirement(targeting) : null;
    const targetingScene = activeReq ? { candidates: activeReq.candidates ?? [] } : undefined;
    const sel = targeting ? undefined : (selectedId ?? undefined);
    return buildTableScene(view, sel, viewportWidth, targetingScene);
  }, [view, selectedId, viewportWidth, targeting]);

  if (!view || !scene) {
    return (
      <main style={main}>
        <span style={muted}>Waiting for game state…</span>
      </main>
    );
  }

  // The selected entity's actions come straight from what the server offered —
  // never recomputed. Suppressed during targeting (selection is inactive then).
  const selectedActions =
    selectedId === null || targeting !== null
      ? []
      : (prompt?.subjectActions ?? []).filter((action) => action.subject?.includes(selectedId));
  const selectedCard = findCard(scene, selectedId);

  // Fire an action: a multi-step (targeted) action enters targeting mode; a plain
  // action is submitted immediately (token echoed, no targets).
  const fire = (action: ValidAction): void => {
    if (requiresTargets(action)) {
      setSelectedId(null);
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

  const cancelTargeting = (): void => setTargeting(null);
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

  return (
    <main style={main}>
      <PromptBanner view={view} prompt={prompt} targeting={targetingBanner} />
      <PlayerTiles
        view={view}
        localId={localId}
        targeting={
          targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined
        }
      />
      <div style={boardWrap(scene.width, scene.height)}>
        <BattlefieldCanvas scene={scene} />
        <EntityOverlay
          scene={scene}
          selectedId={selectedId}
          targeting={targeting !== null}
          onSelect={toggleSelect}
          onChoose={fire}
          onPickTarget={pickTarget}
        />
      </div>
      <ActionBar
        globalActions={targeting ? [] : (prompt?.globalActions ?? [])}
        selectedActions={selectedActions}
        selectedName={selectedCard?.name}
        onChoose={fire}
        onCancelTargeting={targeting ? cancelTargeting : undefined}
      />
    </main>
  );
}
