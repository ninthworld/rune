/**
 * Spectate mode (ADR 0022, issue #351): a **read-only** table over a
 * {@link SpectatorView}, in the fixed shell (ADR 0023).
 *
 * A spectator is a non-seated observer; its view carries only public information (no
 * hand, no mana pool, no `valid_actions`), so this shell reuses the shared panel /
 * stack / log renderers but drops the hand, the action dock, and every interactive
 * affordance. Redaction is enforced upstream by the type — there is no hidden field
 * to accidentally render — so this component's only job is to *not* offer
 * interaction: cards are inspectable (peek / pin) and public zones are browsable,
 * but nothing is selectable, targetable, or submittable.
 *
 * With no receiver, every seat lays out as a bounded player panel (the scene
 * builder folds the receiver's panel frame into the pool), and the bottom shell
 * shows only a quiet "Spectating" badge.
 *
 * The whole UI reconstructs from the single {@link SpectatorView}, so a spectator that
 * joins mid-game (or reconnects) renders the complete public board from its first
 * frame with no history.
 */
import { useEffect, useMemo, useState } from 'react';
import type { EntityId, GameView, PlayerId, SpectatorView } from '../protocol';
import { playerName } from '../playerNames';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { CardInspect, type InspectTarget } from './CardInspect';
import { EntityOverlay } from './EntityOverlay';
import { GameOverOverlay } from './GameOverOverlay';
import { PanelChrome, type BrowsableZone } from './PanelChrome';
import { Rail } from './Rail';
import { TopBar } from './TopBar';
import { ZoneBrowser } from './ZoneBrowser';
import { buildTableScene, type SceneGeometry } from './scene';
import { layout, type Viewport } from './layout';
import { regionBox, sceneBox, shellBox } from './styles';
import s from './chrome.module.css';

const noop = (): void => {};

/** Measured viewport, tracking window resizes (the read-only analogue of the table's
 * own hook). A spectator has no action dock, so pointer precision does not matter — it
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
 * Present a {@link SpectatorView} as the public {@link GameView} shape the shared
 * renderers consume. There is **no receiver** (`you: ''`), so `buildTableScene` lays
 * out every seat as a bounded panel (no local band) and the chrome shows every
 * player; the hand, mana pool, and action list are empty because a spectator has
 * none. Nothing hidden is invented — the spectator view simply has no private state
 * to fill.
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

/**
 * Re-carve the shell's `you` frame with a zone-piles column before the scene lays
 * seats into it. The full composition's `you` frame parks no piles (a *receiver's*
 * piles live in the bottom shell's identity panel) — but a spectator shows only the
 * badge there, so the seat the scene folds into that frame would lose its
 * library/graveyard/exile everywhere. Give it the same column the opponent panels
 * get, so every seat's public piles stay findable on the board. No-op when the
 * frame already parks piles (the compact composition).
 */
function withPilesColumn(geometry: SceneGeometry): SceneGeometry {
  if (geometry.you.piles.w > 0) return geometry;
  const pilesW = geometry.opponents[0]?.piles.w ?? 60;
  const { rect, header, content } = geometry.you;
  return {
    ...geometry,
    you: {
      rect,
      header,
      content: { ...content, w: Math.max(0, content.w - pilesW) },
      piles: {
        x: rect.x + rect.w - pilesW,
        y: rect.y + header.h,
        w: pilesW,
        h: Math.max(0, rect.h - header.h),
      },
    },
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
  // Every seat is a player panel on a spectator table: the shell carves N-1
  // opponent frames plus the receiver frame, and the scene builder (no receiver)
  // folds that frame into the pool, so all N seats get a bounded panel.
  const playerCount = Math.max(1, publicView.opponents.length);
  const shell = useMemo(() => layout(viewport, playerCount), [viewport, playerCount]);
  const compact = shell.composition === 'compact';
  const scene = useMemo(
    () => buildTableScene(publicView, undefined, withPilesColumn(shell.scene)),
    [publicView, shell],
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
      data-composition={shell.composition}
      style={shellBox(viewport.width, viewport.height)}
    >
      <div style={regionBox(r.topBar.rect)}>
        <TopBar view={publicView} mode="overview" compact={compact} />
      </div>
      <div style={regionBox(r.canvas.rect)} className={s.regionCanvas}>
        <div style={sceneBox(scene.width, scene.height)}>
          <BattlefieldCanvas scene={scene} isolatedId={null} />
          <PanelChrome
            view={publicView}
            scene={scene}
            onOpenZone={(playerId, zone) => setBrowsing({ playerId, zone })}
          />
          {/* Read-only overlay: no select/target handlers, so the board is
              inspect-only. Every card still hosts the peek/pin inspect gestures. */}
          <EntityOverlay
            scene={scene}
            selectedId={null}
            targeting={false}
            pointer={viewport.pointer}
            onSelect={noop}
            onPickTarget={noop}
            onPeek={setPeekId}
            onPinInspect={setInspectedId}
          />
        </div>
      </div>
      {!compact && (
        <div style={regionBox(r.rail.rect)} className={s.regionRail}>
          <Rail view={publicView} onInspect={setInspectedId} />
        </div>
      )}
      {/* Where the receiver's identity panel would live, a spectator shows only a
          badge — no hand, no action dock, nothing to play. */}
      <div style={regionBox(r.mePanel.rect)}>
        <div className={s.spectatorBadge} data-testid="spectator-badge">
          Spectating
        </div>
      </div>
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
