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

/**
 * The persistent categories of the relationship grammar
 * (`stack-and-relationships.md` §4.3, implementation note IN3).
 *
 * Each is separated from the others by **geometry**, never by hue alone (I5):
 * paths arc and taper, blocker links are a straight doubled stroke with no
 * arrowhead (R8 / D7), and attachments are an elbow bracket with symmetric
 * square terminals that is never an arc (R9 / D6).
 */
export type PersistentCategory =
  | 'targeting-path'
  | 'attack-path'
  | 'blocker-link'
  /** R9 — aura/equipment attachment: `Permanent.attached_to`. */
  | 'attachment-bracket'
  /** R9 — an ability plate's tether back to its source: `StackItem.source`. */
  | 'source-tether';

/**
 * The state a relationship path is in (`stack-and-relationships.md` §4.4,
 * implementation note IN3). Every state is legible **without motion**: the
 * dash pattern, the alpha, and the presence of a stroke carry it, so the draw
 * program stays a pure function of declared state.
 */
export type RelationshipState =
  /** An active targeting slot's live candidate: dashed, crawling. */
  | 'pending'
  /** A slot the player already answered, session not yet submitted: dashed, still. */
  | 'provisional'
  /** Server-stated: solid. */
  | 'confirmed'
  /** Confirmed but another object is focused, or the board is crowded. */
  | 'calmed'
  /** The scalability floor: caps only, no stroke (§4.4, D11). */
  | 'endpoint-only'
  /** The entry is resolving: the stroke retracts source → destination (§6.2). */
  | 'resolving';

/**
 * What kind of thing a relationship's destination is, which selects its
 * endpoint treatment (`stack-and-relationships.md` §5). The **server** decides
 * this — the client classifies a destination only by membership in the view's
 * own lists, never by interpreting text (I1, and gap G6).
 */
export type EndpointKind =
  /** §5.2 — an object on the battlefield: open reticle + inward chevron. */
  | 'card'
  /** §5.3 — a player: a 90° arc on the crest ring (D8). */
  | 'player'
  /** §5.4 — a zone/pile: a square bracket (D9). Dormant until gap G7 lands. */
  | 'zone'
  /** §5.5 — another stack object: an inset reticle inside the slot. */
  | 'stack';

/**
 * One persistent effect — declaratively reconciled by `id`. The four
 * constituents of §4.1 (source cap, path, direction device, destination cap)
 * are derived from these fields alone; a relationship missing one is a bug.
 */
export interface PersistentEffect {
  /** Stable identity across frames (e.g. `path:<actionId>`, `link:<blocker>`). */
  id: string;
  category: PersistentCategory;
  from: EffectAnchor;
  to: EffectAnchor;
  /** Accent token for the stroke. */
  accent: string;
  /**
   * The §4.4 path state. Omitted ⇒ the category's default: `pending` for a
   * targeting path (the live-session shape), `confirmed` for everything else
   * (combat and attachment are server-stated facts, and are therefore static —
   * which is what keeps the zero-idle contract of §8.4 / IN1).
   */
  state?: RelationshipState;
  /** The §5 destination treatment. Omitted ⇒ `card`. */
  endpoint?: EndpointKind;
  /**
   * The destination's 1-based position in the server's target list (§4.5). It
   * is the ordering channel shared with the entry's summary chips and the
   * accessible name, and it is never derived from screen geometry.
   */
  numeral?: number;
  /**
   * §10.3 — the container an **occluded** endpoint clamps to. The caller sets
   * this when the endpoint exists in the view but has no rect (scrolled out of
   * a rail, behind the compact sheet, outside the viewport); the layer then
   * terminates the path at that container's edge and emits an edge indicator
   * instead of retiring the relationship (implementation note IN2). Absent ⇒
   * an unresolvable endpoint retires, the carried behaviour.
   */
  edge?: Rect;
}

/**
 * Which constituent of the relationship grammar an op belongs to
 * (`stack-and-relationships.md` §4.1: source cap, path, direction device,
 * destination cap). It carries no rendering meaning — it is what makes the
 * structural snapshot (ADR 0011) able to assert *the grammar* rather than a
 * bag of lines: "this path tapers source → destination", "this destination wears
 * a bracket and not a reticle", "an endpoint-only relationship is exactly two
 * caps and no stroke".
 */
export type DrawPart =
  /** §5.1 the filled source disc. */
  | 'source'
  /** The relationship's stroke (dashed, solid, or retracting). */
  | 'path'
  /** §4.3 R5 the shared trunk before the fan node. */
  | 'trunk'
  /** §4.3 R5 the hollow fan node at the split. */
  | 'fan'
  /** §5.2–§5.5 the destination cap (reticle, crest arc, zone bracket). */
  | 'cap'
  /** §4.5 the numeral pips carrying target order. */
  | 'numeral'
  /** §4.3 R9 an elbow bracket's square terminal. */
  | 'terminal'
  /** §10.3 the edge indicator of an occluded endpoint. */
  | 'edge';

/**
 * One primitive of a built draw program — the structural-snapshot unit
 * (ADR 0011: assert the drawn structure, not pixels). The Pixi drawer executes
 * these verbatim; tests assert them directly, GPU-free.
 */
export type DrawOp =
  | {
      op: 'segment';
      category: string;
      /** The grammatical constituent this op belongs to; omitted for transients. */
      part?: DrawPart;
      from: { x: number; y: number };
      to: { x: number; y: number };
      color: string;
      width: number;
      alpha: number;
    }
  | {
      op: 'circle';
      category: string;
      /** The grammatical constituent this op belongs to; omitted for transients. */
      part?: DrawPart;
      x: number;
      y: number;
      r: number;
      color: string;
      alpha: number;
      fill: boolean;
    }
  | {
      /**
       * An axis-aligned rectangle — the **square terminal** of the attachment
       * bracket and source tether (§4.3 R9, §5). It exists because "square" is
       * the load-bearing semantic there: a symmetric square terminal is what
       * separates "attached / belongs to" from every circular target cap, and
       * approximating it with four segments would double that kind's op cost
       * against the §8.1 accounting.
       */
      op: 'rect';
      category: string;
      /** The grammatical constituent this op belongs to. */
      part?: DrawPart;
      x: number;
      y: number;
      w: number;
      h: number;
      color: string;
      alpha: number;
      fill: boolean;
    };
