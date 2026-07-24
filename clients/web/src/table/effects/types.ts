import type { Rect } from '../scene';

/**
 * Effect quality levels (presentation-budgets §Quality levels). Quality sizes
 * the particle pool; it never degrades the scene itself.
 */
export type EffectQuality = 'high' | 'standard' | 'lite';

/**
 * The particle-pool caps per quality level — binding numbers from
 * `presentation-budgets.md` §Performance ("Particle caps per level: High ≤ 400
 * live, Standard ≤ 150, Lite ≤ 40"). At Lite the vocabulary degrades to
 * pulses and edge flashes only (no particle spawns), so the cap is headroom.
 */
export const PARTICLE_CAP: Record<EffectQuality, number> = {
  high: 400,
  standard: 150,
  lite: 40,
};

/** Maximum simultaneous transient invocations (rings/flashes included). */
export const TRANSIENT_CAP: Record<EffectQuality, number> = {
  high: 64,
  standard: 32,
  lite: 8,
};

/**
 * The effect-density control, available **independently of the quality level**
 * (presentation-budgets §Quality levels): a straight multiplier on spawn
 * counts. `minimal` keeps pulses/paths but spawns no particles.
 */
export type EffectDensity = 'full' | 'reduced' | 'minimal';

/** Spawn-count multiplier per density setting (`reduced` ≈ 40%, per budgets). */
export const DENSITY_SCALE: Record<EffectDensity, number> = {
  full: 1,
  reduced: 0.4,
  minimal: 0,
};

/**
 * An effect endpoint: either a **live reference** resolved through the layer's
 * rect source at draw time (an entity id or a crest key — this is what lets
 * endpoints track reconciler motion), or a fixed rect for one-shot points.
 */
export type EffectAnchor = { ref: string } | { rect: Rect };

/**
 * Generic transient categories: play once, retire themselves.
 *
 * `off-focus-ping` carries the layout-model's "off-focus activity is never
 * silent" channel (issue #501): a quiet rune ping at an acting non-focused
 * seat's crest cluster — or its summary tile on compact geometry, which shares
 * the same `seat:<id>` anchor.
 */
export type TransientCategory =
  | 'impact'
  | 'damage'
  | 'healing'
  | 'resolution'
  | 'cast'
  | 'counter'
  | 'death'
  | 'draw'
  | 'counter-change'
  | 'battlefield-entry'
  | 'flow'
  | 'off-focus-ping';

/**
 * One transient invocation — a category plus parameters the client already has
 * (asset-pipeline §The generic effect taxonomy: **data-driven categories keyed
 * to game events, never bespoke per card**). Anything without richer metadata
 * gets its category's default rendering.
 */
export interface TransientInvocation {
  category: TransientCategory;
  /** Where the effect lands (the object/crest the state change hit). */
  target: EffectAnchor;
  /** Accent token — a semantic hue, seat accent, or frame color. */
  accent: string;
  /** Relative size of the moment (e.g. damage dealt); scales counts/radius. */
  magnitude?: number;
}

/** The persistent v1 categories: live while the interaction is pending. */
export type PersistentCategory = 'targeting-path' | 'attack-path' | 'blocker-link';

/**
 * One persistent effect — declaratively reconciled by `id`: targeting/attack
 * paths (dash-crawl bezier terminating at a crest or object) and the carried
 * doubled-stroke blocker link.
 */
export interface PersistentEffect {
  /** Stable identity across frames (e.g. `path:<actionId>`, `link:<blocker>`). */
  id: string;
  category: PersistentCategory;
  from: EffectAnchor;
  to: EffectAnchor;
  /** Accent token for the stroke. */
  accent: string;
}

/**
 * One primitive of a built draw program — the structural-snapshot unit
 * (ADR 0011: assert the drawn structure, not pixels). The Pixi drawer executes
 * these verbatim; tests assert them directly, GPU-free.
 */
export type DrawOp =
  | {
      op: 'segment';
      category: string;
      from: { x: number; y: number };
      to: { x: number; y: number };
      color: string;
      width: number;
      alpha: number;
    }
  | {
      op: 'circle';
      category: string;
      x: number;
      y: number;
      r: number;
      color: string;
      alpha: number;
      fill: boolean;
    };
