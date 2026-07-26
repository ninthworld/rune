/**
 * Spectate mode (ADR 0022, issue #351) on the shipped 2.5D scene plane (ADR 0030,
 * issue #504). A **read-only** table over a {@link SpectatorView}: it rides the
 * exact same live stack players use — {@link LivePlane} stages the seats on
 * `stagePlane`, the `PlaneReconciler` paints the DOM cards, and Pixi renders only
 * the passive effects overlay — with every action affordance removed.
 *
 * A spectator is a non-seated observer; its view carries only public information
 * (no hand, no mana pool, no `valid_actions`). Redaction is enforced upstream by
 * the type — there is no hidden field to accidentally render — so hidden zones come
 * through as card backs / counts exactly as an opponent's do for a seated player.
 * Presenting the {@link SpectatorView} as the receiver-less public {@link GameView}
 * shape (`you: ''`) makes `stagePlane` lay **every seat as an opponent**; with no
 * `valid_actions` and no candidates, the live control layer degrades to inspect +
 * focus + zone-browse only — nothing is selectable, targetable, or submittable.
 *
 * The whole UI reconstructs from the single {@link SpectatorView}, so a spectator
 * that joins mid-game — or reconnects — rebuilds the complete public board from its
 * first frame through the same `rebuild()` / `skipTransitions()` path players use
 * (driven by the store's transport generation).
 */
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { collectArtCards, getArtVersion, noteCards, subscribeArt } from '../card/art/artStore';
import type { EntityId, GameView, PlayerId, SpectatorView } from '../protocol';
import { playerName } from '../playerNames';
import { useGameStore } from '../store';
import { CardInspect } from './CardInspect';
import { GameOverOverlay } from './GameOverOverlay';
import type { BrowsableZone } from './PanelChrome';
import { Rail } from './Rail';
import { TopBar } from './TopBar';
import { ZoneBrowser } from './ZoneBrowser';
import { useReducedMotion } from './hooks/useReducedMotion';
import { useViewport } from './hooks/useViewport';
import { LivePlane } from './live';
import type { LivePlaneInteractionProps } from './live/LivePlaneControls';
import { useSessionMoments } from './live/useSessionMoments';
import { resolveInspect } from './tableView';
import styles from './spectator.module.css';

interface OpenZone {
  playerId: PlayerId;
  zone: BrowsableZone;
}

/**
 * Present a {@link SpectatorView} as the public {@link GameView} shape the live
 * stack consumes. There is **no receiver** (`you: ''`), so `stagePlane` lays out
 * every seat as an opponent; the hand, mana pool, and action list are empty because
 * a spectator has none. Nothing hidden is invented — the spectator view simply has
 * no private state to fill (ADR 0022 redaction already shaped it).
 */
function asPublicView(spec: SpectatorView): GameView {
  return {
    you: '',
    my_hand: [],
    me: { life: 0, library_size: 0 },
    opponents: spec.players,
    battlefield: spec.battlefield,
    stack: spec.stack,
    graveyards: spec.graveyards,
    exile: spec.exile,
    phase: spec.phase,
    turn: spec.turn,
    active_player: spec.active_player,
    seat_order: spec.seat_order,
    mana_pool: [],
    priority_player: spec.priority_player,
    valid_actions: [],
    result: spec.result,
    log: spec.log ?? [],
    stops: [],
    auto_passed: false,
    action_rejected: false,
    player_names: spec.player_names,
    // The public command zone, damage tally, and recast tax carry straight through so
    // a spectator renders the same commander chrome as a seated player (issue #371/#372).
    command: spec.command ?? [],
    commander_damage: spec.commander_damage,
    commander_tax: spec.commander_tax ?? [],
  };
}

/** The read-only spectate table (ADR 0022, issue #351; ADR 0030 plane, issue #504). */
export function SpectatorTable({ view: spec }: { view: SpectatorView }) {
  const sessionEpoch = useGameStore((state) => state.sessionEpoch);
  const leaveGame = useGameStore((state) => state.leaveGame);
  const artVersion = useSyncExternalStore(subscribeArt, getArtVersion);
  const viewport = useViewport();
  const reducedMotion = useReducedMotion();
  const publicView = useMemo(() => asPublicView(spec), [spec]);

  const [focusedSeat, setFocusedSeat] = useState<PlayerId | null>(null);
  const [inspectedId, setInspectedId] = useState<EntityId | null>(null);
  const [browsing, setBrowsing] = useState<OpenZone | null>(null);
  // The §8 session moments (issue #509): a spectator gets the same entry
  // assembly, the same reconnect acknowledgment, and the same recede on the way
  // back to the lobby that a seated player does.
  const { moment, notePresentationMode, leave } = useSessionMoments(reducedMotion, leaveGame);

  useEffect(() => {
    noteCards(collectArtCards(publicView));
  }, [publicView]);

  // A new authoritative frame supersedes every ephemeral presentation choice.
  useEffect(() => {
    setFocusedSeat(null);
    setInspectedId(null);
    setBrowsing(null);
  }, [spec]);

  const compact = viewport.width < 900;
  const inspectTarget = inspectedId !== null ? resolveInspect(publicView, inspectedId) : null;
  const browserData = browsing
    ? {
        zone: browsing.zone,
        owner: playerName(publicView, browsing.playerId),
        cards:
          (browsing.zone === 'graveyard' ? publicView.graveyards : publicView.exile).find(
            (pile) => pile.player_id === browsing.playerId,
          )?.cards ?? [],
      }
    : null;

  // Read-only control surface: no selection, no picking, no candidates. With the
  // public view carrying no `valid_actions`, every staged render degrades to a
  // transparent inspect surface and every crest to a focus control — the board is
  // inspectable and browsable, never actionable.
  const interaction: LivePlaneInteractionProps = {
    selectedId: null,
    picking: false,
    multiSelect: false,
    candidates: [],
    chosen: [],
    playerCandidates: [],
    onActivateEntity: () => {},
    onPickEntity: () => {},
    onPickPlayer: () => {},
    onInspect: setInspectedId,
    onOpenZone: (playerId, zone) => setBrowsing({ playerId, zone }),
    onFocusSeat: setFocusedSeat,
  };

  return (
    <main
      className={styles.shell}
      data-testid="spectator-table"
      data-mode="overview"
      data-composition={compact ? 'compact' : 'full'}
      data-moment={moment ?? undefined}
      data-orienting={moment === 'reconnect' || undefined}
    >
      <header className={styles.top}>
        <TopBar view={publicView} mode="overview" compact={compact} />
      </header>

      <section className={styles.scene} aria-label="Battlefield">
        <LivePlane
          view={publicView}
          staging={{ focusSeat: focusedSeat ?? undefined }}
          quality="standard"
          density="reduced"
          reducedMotion={reducedMotion}
          artVersion={artVersion}
          sessionEpoch={sessionEpoch}
          onMode={notePresentationMode}
          interaction={interaction}
        />
      </section>

      {!compact && (
        <aside className={styles.rail}>
          <Rail view={publicView} onInspect={setInspectedId} />
        </aside>
      )}

      {/* Where the receiver's identity panel and controls would live, a spectator
          shows only a quiet badge — no hand, no dock, nothing to play. */}
      <section className={styles.bottom} aria-label="Spectating">
        <span className={styles.badge} data-testid="spectator-badge">
          Spectating
        </span>
      </section>

      {browserData && (
        <ZoneBrowser
          zone={browserData.zone}
          owner={browserData.owner}
          cards={browserData.cards}
          onInspect={setInspectedId}
          onClose={() => setBrowsing(null)}
        />
      )}
      {inspectTarget && <CardInspect target={inspectTarget} onClose={() => setInspectedId(null)} />}
      {/* The terminal verdict, shown to the spectator with no personal "you" framing. */}
      {/* A spectator watching a finished game needs the same way out (issue #452). */}
      {spec.result && (
        <GameOverOverlay
          result={spec.result}
          you=""
          names={spec.player_names}
          reducedMotion={reducedMotion}
          onLeave={leave}
        />
      )}
    </main>
  );
}
