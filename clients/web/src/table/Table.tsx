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
 */
import { useMemo, useState } from 'react';
import type { EntityId } from '../protocol';
import { selectPendingPrompt, useGameStore } from '../store';
import { ActionBar } from './ActionBar';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { EntityOverlay } from './EntityOverlay';
import { PlayerTiles } from './PlayerTiles';
import { PromptBanner } from './PromptBanner';
import { buildTableScene, type RenderedCard, type TableScene } from './scene';
import { boardWrap, main, muted } from './styles';

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

  const prompt = useMemo(() => selectPendingPrompt(view), [view]);
  // The server names the receiver directly in `view.you`; an older server may
  // omit it (empty), which we treat as "unknown".
  const localId = view?.you || undefined;
  const scene = useMemo(
    () => (view ? buildTableScene(view, selectedId ?? undefined) : null),
    [view, selectedId],
  );

  if (!view || !scene) {
    return (
      <main style={main}>
        <span style={muted}>Waiting for game state…</span>
      </main>
    );
  }

  // The selected entity's actions come straight from what the server offered —
  // never recomputed. If the selection no longer exists in the new view, this is
  // empty and the echo disappears (state stays non-load-bearing).
  const selectedActions =
    selectedId === null
      ? []
      : (prompt?.subjectActions ?? []).filter((action) => action.subject?.includes(selectedId));
  const selectedCard = findCard(scene, selectedId);

  const fire = (actionId: string): void => {
    choose(actionId);
    setSelectedId(null);
  };
  const toggleSelect = (id: EntityId): void =>
    setSelectedId((current) => (current === id ? null : id));

  return (
    <main style={main}>
      <PromptBanner view={view} prompt={prompt} />
      <PlayerTiles view={view} localId={localId} />
      <div style={boardWrap(scene.width, scene.height)}>
        <BattlefieldCanvas scene={scene} />
        <EntityOverlay
          scene={scene}
          selectedId={selectedId}
          onSelect={toggleSelect}
          onChoose={fire}
        />
      </div>
      <ActionBar
        globalActions={prompt?.globalActions ?? []}
        selectedActions={selectedActions}
        selectedName={selectedCard?.name}
        onChoose={fire}
      />
    </main>
  );
}
