/**
 * The game-over screen (issue #141): pure render of a terminal {@link GameView} that
 * carries `result`. The DOM overlay names the verdict/reason; the interactive
 * prompt/action UI is suppressed (the server sends no actions once the game is
 * over). The final board + panels stay visible beneath, read-only. Regions dock
 * exactly where they do during play (ADR 0023: regions never reorder between
 * states) — this is the same shell geometry as {@link Table}, only inert.
 */
import type { EntityId, GameResult, GameView, PlayerId } from '../protocol';
import type { BrowsableZone } from './PanelChrome';
import type { TableLayout, Viewport } from './layout';
import type { TableScene } from './scene';
import { BattlefieldCanvas } from './BattlefieldCanvas';
import { EntityOverlay } from './EntityOverlay';
import { GameOverOverlay } from './GameOverOverlay';
import { MePanel } from './MePanel';
import { PanelChrome } from './PanelChrome';
import { Rail } from './Rail';
import { TopBar } from './TopBar';
import { regionBox, sceneBox, shellBox } from './styles';
import s from './chrome.module.css';

interface Props {
  view: GameView;
  /** The terminal result, narrowed by the caller (rendered only when present). */
  result: GameResult;
  scene: TableScene;
  shell: TableLayout;
  viewport: Required<Viewport>;
  localId: PlayerId | undefined;
  highlightedId: EntityId | null;
  reducedMotion: boolean;
  onOpenZone: (playerId: PlayerId, zone: BrowsableZone) => void;
  onInspect: React.Dispatch<React.SetStateAction<EntityId | null>>;
  onPeek: React.Dispatch<React.SetStateAction<EntityId | null>>;
  onHighlight: (id: EntityId) => void;
  overlays: React.ReactNode;
}

export function GameOverTable({
  view,
  result,
  scene,
  shell,
  viewport,
  localId,
  highlightedId,
  reducedMotion,
  onOpenZone,
  onInspect,
  onPeek,
  onHighlight,
  overlays,
}: Props) {
  const r = shell.regions;
  const compact = shell.composition === 'compact';
  return (
    <main
      className={s.shell}
      data-testid="table-game-over"
      data-mode="overview"
      data-composition={shell.composition}
      style={shellBox(viewport.width, viewport.height)}
    >
      <div style={regionBox(r.topBar.rect)}>
        <TopBar view={view} mode="overview" localId={localId} compact={compact} />
      </div>
      <div style={regionBox(r.canvas.rect)} className={s.regionCanvas}>
        <div style={sceneBox(scene.width, scene.height)}>
          <BattlefieldCanvas scene={scene} isolatedId={highlightedId} />
          <PanelChrome
            view={view}
            scene={scene}
            onOpenZone={onOpenZone}
            highlightedId={highlightedId}
            reducedMotion={reducedMotion}
          />
          {/* Read-only game-over board: no select/target interaction (the server
              offers no actions once the game is over), but every card stays
              inspectable, so the overlay renders inspect handles only (#261). */}
          <EntityOverlay
            scene={scene}
            selectedId={null}
            targeting={false}
            pointer={viewport.pointer}
            onSelect={() => {}}
            onPickTarget={() => {}}
            onPeek={onPeek}
            onPinInspect={onInspect}
          />
        </div>
      </div>
      {!compact && (
        <div style={regionBox(r.rail.rect)} className={s.regionRail}>
          <Rail
            view={view}
            onInspect={onInspect}
            onHighlight={onHighlight}
            highlightedId={highlightedId}
          />
        </div>
      )}
      <div style={regionBox(r.mePanel.rect)}>
        <MePanel view={view} localId={localId} condensed={compact} onOpenZone={onOpenZone} />
      </div>
      <GameOverOverlay result={result} you={view.you} names={view.player_names} />
      {overlays}
    </main>
  );
}
