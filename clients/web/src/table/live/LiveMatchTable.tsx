/**
 * Production composition root for the ADR 0030 2.5D match surface.
 *
 * The latest complete GameView feeds every layer: environment + staged DOM
 * plane + passive effects + the existing screen-space chrome. This first Phase
 * 2 slice intentionally leaves plane/hand input inert; issue #491 owns the
 * interaction state machine and event routing. Global actions and existing
 * chrome controls remain direct echoes of server-offered actions.
 */
import { useEffect, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { collectArtCards, getArtVersion, noteCards, subscribeArt } from '../../card/art/artStore';
import { CardFace } from '../../card/dom';
import type { EntityId, PlayerId } from '../../protocol';
import { playerName } from '../../playerNames';
import { selectPendingPrompt, useGameStore } from '../../store';
import { publishPlane, publishScene, publishView } from '../../testHooks';
import { ActionDock } from '../ActionDock';
import { ArtSettings } from '../ArtSettings';
import { CardInspect } from '../CardInspect';
import { GameOverOverlay } from '../GameOverOverlay';
import { MePanel } from '../MePanel';
import type { BrowsableZone } from '../PanelChrome';
import { PromptStrip } from '../PromptStrip';
import { Rail } from '../Rail';
import { RejectionToast } from '../RejectionToast';
import { ShortcutHelp } from '../ShortcutHelp';
import { TopBar, type RailSheet } from '../TopBar';
import { ZoneBrowser } from '../ZoneBrowser';
import type { EffectDensity, EffectQuality } from '../effects';
import { useReducedMotion } from '../hooks/useReducedMotion';
import { useViewport } from '../hooks/useViewport';
import { domCardArt, handDisplayData } from '../planeDisplayData';
import { buildShortcutBindings, demandsDecision, resolveInspect } from '../tableView';
import { LivePlane } from './LivePlane';
import styles from './live-match.module.css';

/** Tunable presentation inputs. Preferences can replace these defaults later. */
export interface LiveMatchTableProps {
  quality?: EffectQuality;
  density?: EffectDensity;
}

interface OpenZone {
  playerId: PlayerId;
  zone: BrowsableZone;
}

/** Render a real personalized match on the 2.5D scene stack. */
export function LiveMatchTable({ quality = 'standard', density = 'reduced' }: LiveMatchTableProps) {
  const view = useGameStore((state) => state.view);
  const choose = useGameStore((state) => state.choose);
  const setStops = useGameStore((state) => state.setStops);
  const disconnect = useGameStore((state) => state.disconnect);
  const rejectionNonce = useGameStore((state) => state.rejectionNonce);
  const artVersion = useSyncExternalStore(subscribeArt, getArtVersion);
  const viewport = useViewport();
  const reducedMotion = useReducedMotion();
  const [highlightedId, setHighlightedId] = useState<EntityId | null>(null);
  const [inspectedId, setInspectedId] = useState<EntityId | null>(null);
  const [browsing, setBrowsing] = useState<OpenZone | null>(null);
  const [railSheet, setRailSheet] = useState<RailSheet | null>(null);
  const [showHelp, setShowHelp] = useState(false);
  const [showArtSettings, setShowArtSettings] = useState(false);

  useEffect(() => {
    if (view) noteCards(collectArtCards(view));
  }, [view]);

  // A new complete view supersedes every ephemeral presentation choice.
  useEffect(() => {
    setHighlightedId(null);
    setInspectedId(null);
    setBrowsing(null);
    setRailSheet(null);
  }, [view]);

  useEffect(() => {
    publishScene(null);
    return () => {
      publishPlane(null);
      publishView(null);
    };
  }, []);

  useEffect(() => {
    publishView(view);
  }, [view]);

  if (!view) {
    return (
      <main className={styles.waiting} data-testid="table-waiting">
        <span>Connected — waiting for first game state…</span>
        <button type="button" onClick={disconnect} data-testid="disconnect-button">
          Disconnect
        </button>
      </main>
    );
  }

  const compact = viewport.width < 900;
  const prompt = selectPendingPrompt(view);
  const mode = demandsDecision(view) ? 'focus' : 'overview';
  const localId = view.you || undefined;
  const inspectTarget = inspectedId ? resolveInspect(view, inspectedId) : null;
  const browserData = browsing
    ? {
        title: `${playerName(view, browsing.playerId)} — ${
          browsing.zone === 'graveyard' ? 'Graveyard' : 'Exile'
        }`,
        cards:
          (browsing.zone === 'graveyard' ? view.graveyards : view.exile).find(
            (pile) => pile.player_id === browsing.playerId,
          )?.cards ?? [],
      }
    : null;
  const passOffered = view.valid_actions.some((action) => action.type === 'pass_priority');
  const shortcuts = buildShortcutBindings(passOffered);

  const highlight = (id: EntityId): void =>
    setHighlightedId((current) => (current === id ? null : id));
  const openZone = (playerId: PlayerId, zone: BrowsableZone): void =>
    setBrowsing({ playerId, zone });

  const rail = (
    <Rail
      view={view}
      onInspect={setInspectedId}
      onHighlight={highlight}
      highlightedId={highlightedId}
    />
  );

  return (
    <main
      className={styles.shell}
      data-testid="live-match-table"
      data-mode={mode}
      data-composition={compact ? 'compact' : 'full'}
    >
      <header className={styles.top}>
        <TopBar
          view={view}
          mode={mode}
          localId={localId}
          compact={compact}
          onSetStops={setStops}
          onOpenSheet={compact ? setRailSheet : undefined}
          concede={view.valid_actions.find((action) => action.type === 'concede')}
          onChoose={choose}
          onShowShortcuts={() => setShowHelp(true)}
          onShowArtSettings={() => setShowArtSettings(true)}
        />
      </header>

      <section className={styles.scene} aria-label="Battlefield">
        <LivePlane
          view={view}
          staging={{
            focusSeat:
              highlightedId !== null &&
              view.opponents.some((opponent) => opponent.player_id === highlightedId)
                ? highlightedId
                : undefined,
            selectedId: highlightedId ?? undefined,
          }}
          quality={quality}
          density={density}
          reducedMotion={reducedMotion}
          artVersion={artVersion}
          onPlane={publishPlane}
        />
      </section>

      {!compact && <aside className={styles.rail}>{rail}</aside>}

      <section className={styles.bottom} aria-label="Player controls">
        <div className={styles.identity}>
          <MePanel
            view={view}
            localId={localId}
            condensed
            onOpenZone={openZone}
            highlightedId={highlightedId}
          />
        </div>
        <div className={styles.hand} data-testid="live-hand">
          {view.my_hand.map((card, index) => (
            <div
              key={card.id}
              className={styles.handCard}
              data-testid={`live-hand-card-${card.id}`}
              style={
                {
                  '--hand-left':
                    view.my_hand.length <= 1
                      ? '50%'
                      : `${10 + (80 * index) / (view.my_hand.length - 1)}%`,
                  '--hand-angle': `${Math.max(
                    -8,
                    Math.min(8, (index - (view.my_hand.length - 1) / 2) * 2),
                  )}deg`,
                } as CSSProperties
              }
            >
              <CardFace data={handDisplayData(view, card)} tier="hand" art={domCardArt(card)} />
            </div>
          ))}
        </div>
        <div className={styles.decisions}>
          <PromptStrip view={view} prompt={prompt} />
          <ActionDock
            globalActions={(prompt?.globalActions ?? []).filter(
              (action) => action.type !== 'concede',
            )}
            selectedActions={[]}
            onChoose={choose}
            waiting={prompt === null}
            deadline={prompt?.deadline}
          />
        </div>
      </section>

      {railSheet && (
        <div className={styles.sheetBackdrop} data-testid={`rail-sheet-${railSheet}`}>
          <div className={styles.sheet}>
            <button
              type="button"
              className={styles.close}
              aria-label="Close"
              data-testid="rail-sheet-close"
              onClick={() => setRailSheet(null)}
            >
              ×
            </button>
            {rail}
          </div>
        </div>
      )}
      {browserData && (
        <ZoneBrowser
          title={browserData.title}
          cards={browserData.cards}
          onInspect={setInspectedId}
          onClose={() => setBrowsing(null)}
        />
      )}
      {inspectTarget && <CardInspect target={inspectTarget} onClose={() => setInspectedId(null)} />}
      {showHelp && <ShortcutHelp bindings={shortcuts} onClose={() => setShowHelp(false)} />}
      {showArtSettings && <ArtSettings onClose={() => setShowArtSettings(false)} />}
      {view.result && (
        <GameOverOverlay result={view.result} you={view.you} names={view.player_names} />
      )}
      <RejectionToast nonce={rejectionNonce} />
    </main>
  );
}
