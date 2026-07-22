import { normalizeGameView } from '../wire';
import type { GameView } from '../protocol';
import {
  buildTableScene,
  defaultSceneGeometry,
  type PanelFrame,
  type SceneGeometry,
  type TableScene,
  type TargetingScene,
} from './scene';

/** The default carved geometry for a duel (the same carve the live shell makes). */
export const GEO = defaultSceneGeometry();
/** The carved geometry for a four-seat table (three opponent panels). */
export const GEO4 = defaultSceneGeometry(4);

/** Build a scene against the default duel geometry (most tests' shell). */
export function build(view: GameView, selectedId?: string, targeting?: TargetingScene): TableScene {
  return buildTableScene(view, selectedId, GEO, targeting);
}

/**
 * A synthetic single-panel geometry whose receiver content area is exactly
 * `contentW` wide — for the wrap tests, where the interesting variable is the
 * row budget, not the whole shell.
 */
export function panelGeometry(contentW: number): SceneGeometry {
  const frame = (y: number): PanelFrame => ({
    rect: { x: 0, y, w: contentW + 32, h: 900 },
    header: { x: 0, y, w: contentW + 32, h: 0 },
    content: { x: 16, y, w: contentW, h: 900 },
    piles: { x: contentW + 32, y, w: 0, h: 0 },
  });
  return {
    width: contentW + 32,
    height: 2000,
    opponents: [frame(0)],
    you: frame(950),
    hand: { x: 16, y: 1900, w: contentW, h: 100 },
    tiers: { you: 'field', opp: 'support' },
    handFan: false,
  };
}

/** A minimal permanent spec for the type-grouped-band tests (issue #318). */
export interface PermSpec {
  id: string;
  type_line: string;
  tapped?: boolean;
  controller?: string;
  name?: string;
  power?: string;
  toughness?: string;
  /** The host this permanent is attached to (issue #333), for clustering tests. */
  attached_to?: string;
}

/** A `GameView` with `p1` local, holding the given permanents (issue #318). */
export function permBoard(perms: PermSpec[], validActions: GameView['valid_actions'] = []): GameView {
  return normalizeGameView({
    you: 'p1',
    my_hand: [],
    opponents: [{ player_id: 'p2', hand_size: 0, life: 20, library_size: 40 }],
    battlefield: perms.map((p) => ({
      id: p.id,
      controller: p.controller ?? 'p1',
      owner: p.controller ?? 'p1',
      tapped: p.tapped,
      attached_to: p.attached_to,
      card: {
        id: p.id,
        name: p.name ?? p.id,
        type_line: p.type_line,
        power: p.power,
        toughness: p.toughness,
      },
    })),
    phase: 'precombat_main',
    valid_actions: validActions,
  });
}

/** A `GameView` whose battlefield holds `perController` permanents for each id. */
export function boardView(controllers: string[], perController: number): GameView {
  const battlefield = controllers.flatMap((controller) =>
    Array.from({ length: perController }, (_, i) => ({
      id: `${controller}_perm_${i}`,
      controller,
      owner: controller,
      card: {
        id: `${controller}_perm_${i}`,
        name: `Servo ${i}`,
        type_line: 'Artifact Creature — Servo',
        power: '1',
        toughness: '1',
      },
    })),
  );
  return normalizeGameView({
    you: controllers[0],
    my_hand: [],
    opponents: controllers.slice(1).map((player_id) => ({
      player_id,
      hand_size: 0,
      life: 20,
      library_size: 40,
    })),
    battlefield,
    phase: 'precombat_main',
    valid_actions: [],
  });
}

/** Every rendered card in the scene (all bands + hand), position included. */
export function allCards(scene: TableScene) {
  return [...scene.bands.flatMap((b) => b.cards), ...scene.hand];
}
