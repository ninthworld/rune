/**
 * Spectate mode (ADR 0022, issue #351): a **read-only** table over a
 * {@link SpectatorView}.
 *
 * A spectator is a non-seated observer; its view carries only public information (no
 * hand, no mana pool, no `valid_actions`), so this shell reuses the shared board /
 * stack / log / HUD renderers but drops the hand row, the action tray, and every
 * interactive affordance. Redaction is enforced upstream by the type — there is no
 * hidden field to accidentally render — so this component's only job is to *not*
 * offer interaction: cards are inspectable (peek / pin) and public zones are
 * browsable, but nothing is selectable, targetable, or submittable.
 *
 * The whole UI reconstructs from the single {@link SpectatorView}, so a spectator that
 * joins mid-game (or reconnects) renders the complete public board from its first
 * frame with no history.
 */
import { useEffect, useMemo, useState } from 'react';
import type { EntityId, GameView, PlayerId, SpectatorView } from '../protocol';
import { playerName } from '../playerNames';
import { RuneMark } from '../chrome/RuneMark';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { CardInspect, type InspectTarget } from './CardInspect';
import { EntityOverlay } from './EntityOverlay';
import { GameOverOverlay } from './GameOverOverlay';
import { OpponentHud } from './PlayerHud';
import { PhaseIndicator } from './PhaseIndicator';
import { Rail } from './Rail';
import { TableGeography, type BrowsableZone } from './TableGeography';
import { ZoneBrowser } from './ZoneBrowser';
import { buildTableScene } from './scene';
import { battlefieldWidth, layout, type Viewport } from './layout';
import { regionBox, sceneBox, shellBox } from './styles';
import s from './chrome.module.css';

const noop = (): void => {};

/** Measured viewport, tracking window resizes (the read-only analogue of the table's
 * own hook). A spectator has no action tray, so pointer precision does not matter — it
 * defaults to `fine`. */
function useViewport(): Required<Viewport> {
  const read = (): Required<Viewport> =>
    typeof window === 'undefined'
      ? { width: 1280, height: 800, pointer: 'fine' }
      : { width: window.innerWidth, height: window.innerHeight, pointer: 'fine' };
  const [viewport, setViewport] = useState(read);
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onResize = (): void => setViewport(read());
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return viewport;
}

/**
 * Present a {@link SpectatorView} as the public {@link GameView} shape the shared board
 * renderers consume. There is **no receiver** (`you: ''`), so `buildTableScene` lays out
 * every seat as an opponent band (no local band) and the HUD shows every player; the
 * hand, mana pool, and action list are empty because a spectator has none. Nothing
 * hidden is invented — the spectator view simply has no private state to fill.
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
  };
}

/** Resolve an entity id to its inspect payload — a public battlefield permanent or a
 * card in a public graveyard/exile pile. A spectator has no hand to inspect. */
function resolveInspect(view: GameView, id: EntityId): InspectTarget | null {
  for (const perm of view.battlefield) {
    if (perm.id === id) {
      const host =
        perm.attached_to !== undefined
          ? view.battlefield.find((p) => p.id === perm.attached_to)
          : undefined;
      const attachments = view.battlefield
        .filter((p) => p.attached_to === perm.id)
        .map((p) => ({ id: p.id, name: p.card.name }));
      return {
        kind: 'card',
        card: perm.card,
        tapped: perm.tapped,
        counters: perm.counters,
        attachedTo: host ? { id: host.id, name: host.card.name } : undefined,
        attachments: attachments.length > 0 ? attachments : undefined,
      };
    }
  }
  for (const pile of [...view.graveyards, ...view.exile]) {
    for (const card of pile.cards) if (card.id === id) return { kind: 'card', card };
  }
  return null;
}

/** The read-only spectate table (ADR 0022, issue #351). */
export function SpectatorTable({ view: spec }: { view: SpectatorView }) {
  const viewport = useViewport();
  const publicView = useMemo(() => asPublicView(spec), [spec]);
  // Every seat is a player area on a spectator table; there is no bottom-anchored
  // receiver, so the HUD reflows for the full seat count.
  const playerCount = Math.max(1, publicView.opponents.length);
  const shell = useMemo(() => layout(viewport, 'overview', playerCount), [viewport, playerCount]);
  const battlefieldW = battlefieldWidth(shell);
  const battlefieldH = shell.regions.battlefield.rect.h;
  const sceneScale = shell.sceneScale;
  const scene = useMemo(
    () =>
      buildTableScene(publicView, undefined, battlefieldW, undefined, {
        scale: sceneScale,
        minHeight: battlefieldH,
      }),
    [publicView, battlefieldW, battlefieldH, sceneScale],
  );

  const [inspectedId, setInspectedId] = useState<EntityId | null>(null);
  const [peekId, setPeekId] = useState<EntityId | null>(null);
  const [browsing, setBrowsing] = useState<{ playerId: PlayerId; zone: BrowsableZone } | null>(
    null,
  );

  const previewId = inspectedId ?? peekId;
  const inspectTarget = previewId !== null ? resolveInspect(publicView, previewId) : null;
  const browserData = browsing
    ? {
        title: `${playerName(publicView, browsing.playerId)} — ${
          browsing.zone === 'graveyard' ? 'Graveyard' : 'Exile'
        }`,
        cards:
          (browsing.zone === 'graveyard' ? publicView.graveyards : publicView.exile).find(
            (pile) => pile.player_id === browsing.playerId,
          )?.cards ?? [],
      }
    : null;

  const r = shell.regions;
  return (
    <main
      className={s.shell}
      data-testid="spectator-table"
      data-mode="overview"
      style={shellBox(viewport.width, viewport.height)}
    >
      <div className={s.regionIndicator} style={regionBox(r.indicator.rect)}>
        <PhaseIndicator view={publicView} mode="overview" />
      </div>
      <div className={s.regionHud} style={regionBox(r.opponentHud.rect)}>
        <OpponentHud view={publicView} />
      </div>
      {/* Where the receiver's dock and hand would live, a spectator shows only a badge —
          no hand row, no action tray, nothing to play. */}
      <div className={s.regionLocalDock} style={regionBox(r.localDock.rect)}>
        <div className={s.spectatorBadge} data-testid="spectator-badge">
          Spectating
        </div>
      </div>
      <div className={s.regionBattlefield} style={regionBox(r.battlefield.rect)}>
        <div style={sceneBox(scene.width, scene.height)}>
          {/* The table surface's faint rune motif, under the transparent canvas. */}
          <div className={s.tableMotif} aria-hidden="true">
            <RuneMark size={420} />
          </div>
          <BattlefieldCanvas scene={scene} isolatedId={null} />
          <TableGeography
            scene={scene}
            onOpenZone={(playerId, zone) => setBrowsing({ playerId, zone })}
          />
          {/* Read-only overlay: no select/target/choose handlers, so the board is
              inspect-only. Every card still hosts the peek/pin inspect gestures. */}
          <EntityOverlay
            scene={scene}
            selectedId={null}
            targeting={false}
            pointer={viewport.pointer}
            onSelect={noop}
            onChoose={noop}
            onPickTarget={noop}
            onPeek={setPeekId}
            onPinInspect={setInspectedId}
          />
        </div>
      </div>
      <Rail
        view={publicView}
        rect={r.rail.rect}
        collapsed={shell.railCollapsed}
        onInspect={setInspectedId}
      />
      {browserData && (
        <ZoneBrowser
          title={browserData.title}
          cards={browserData.cards}
          onInspect={setInspectedId}
          onClose={() => setBrowsing(null)}
        />
      )}
      {inspectTarget && (
        <CardInspect
          target={inspectTarget}
          onClose={() => {
            setInspectedId(null);
            setPeekId(null);
          }}
          transient={inspectedId === null}
        />
      )}
      {/* The terminal verdict, shown to the spectator with no personal "you" framing. */}
      {spec.result && <GameOverOverlay result={spec.result} you="" names={spec.player_names} />}
    </main>
  );
}
