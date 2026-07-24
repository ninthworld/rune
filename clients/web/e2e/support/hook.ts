/**
 * Minimal structural mirror of the client's read-only test hook
 * (`clients/web/src/testHooks.ts`, ADR 0011) for use inside `page.evaluate`.
 *
 * Only the fields this suite reads are declared. It is a *mirror*, not an
 * import: the e2e package is deliberately separate from the client package, and
 * importing the client's own types would drag Vite's `import.meta.env` typing
 * into a plain Node/Playwright build for no benefit.
 *
 * Everything here is **read-only derived render data**. The suite reads the
 * server's `valid_actions` to decide *which* real control to click; it never
 * submits an action through the hook, and it never computes legality itself
 * (`AGENTS.md`: zero game logic in the client — the same rule binds its tests).
 */

/** One server-offered action, as the view advertises it. */
export interface HookAction {
  /** Opaque action id the client echoes back. */
  id: string;
  /** Action kind, e.g. `play_land`, `pass_priority`. */
  type: string;
  /** Server-authored label — what the rendered control reads. */
  label: string;
  /** Entity ids this action hangs off, when it is entity-subject. */
  subject?: string[];
}

/** The subset of `GameView` the smoke spec inspects. */
export interface HookView {
  /** The receiving seat's player id. */
  you: string;
  /** Server-owned turn number (the client never counts turns). */
  turn: number;
  /** Current step. */
  phase: string;
  /** The seat whose turn it is. */
  active_player: string;
  /** The only source of interactivity. */
  valid_actions: HookAction[];
  /** The receiver's hand. */
  my_hand: Array<{ id: string; name: string }>;
  /**
   * Every permanent in play. Note a permanent's entity id is **not** the id the
   * same card carried in hand — zone changes mint a new object — so the suite
   * identifies a freshly played permanent by controller + face name, never by
   * carrying the hand id across the zone change.
   */
  battlefield: Array<{ id: string; controller: string; card: { name: string } }>;
}

/** One individually addressable object staged on the scene plane. */
export interface HookRender {
  /** The render's representative entity id. */
  entityId: string;
  /** Every permanent this render stands for (>1 only when folded ×N). */
  memberIds: string[];
}

/** The subset of `StagedPlane` the smoke spec inspects. */
export interface HookPlane {
  /** The receiver's own band — the "local band" a played permanent lands in. */
  receiver?: { seat: string; renders: HookRender[] };
  /** The opposing board. */
  farSide?: { seat: string; renders: HookRender[] };
}

declare global {
  interface Window {
    /** Present only in test/preview builds; see `clients/web/src/testHooks.ts`. */
    __RUNE_TEST__?: {
      view: HookView | null;
      plane: HookPlane | null;
    };
  }
}
