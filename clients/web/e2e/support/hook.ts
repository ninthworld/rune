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

/** One target slot of a multi-step action, as the server advertises it. */
export interface HookRequirement {
  /** Opaque slot id the answer keys back to (`attackers`, `defend_<id>`, …). */
  slot: string;
  /** Human-readable prompt describing what to choose. */
  prompt: string;
  /** The only candidates the client may offer for this slot. */
  candidates?: string[];
}

/** One non-target prompt slot (option / select-from-zone / order). */
export interface HookPrompt {
  /** Which prompt shape this is. */
  kind: string;
  /** Opaque slot id the answer keys back to. */
  slot: string;
  /** The named choices of an `option` slot. */
  options?: Array<{ id: string; label: string; requires?: string[] }>;
  /** The candidates of a `select_from_zone` slot. */
  candidates?: string[];
  /** How many must be chosen in a `select_from_zone` slot. */
  count?: number;
  /** The items of an `order` slot. */
  items?: string[];
}

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
  /** Whether the server classified this activation as a mana ability (CR 605). */
  mana_ability?: boolean;
  /** Ordered target slots this action requires before it can be taken. */
  requirements?: HookRequirement[];
  /** Non-target choice slots this action poses. */
  prompts?: HookPrompt[];
}

/** A permanent on the battlefield, reduced to what this suite asserts on. */
export interface HookPermanent {
  /** Entity id of this permanent. */
  id: string;
  /** The seat controlling it. */
  controller: string;
  /** Its computed face; `power` is present only for a creature. */
  card: { name: string; power?: string; toughness?: string };
  /** Whether it is tapped. */
  tapped?: boolean;
  /** Whether the server recorded it as attacking this combat. */
  attacking?: boolean;
  /** The defending **player** it was declared against (CR 508.1a, issue #341). */
  attacking_player?: string;
  /** The attacker it was declared as blocking (CR 509). */
  blocking?: string;
  /** Combat damage marked on it this turn. */
  damage?: number;
}

/** The subset of `GameView` this suite inspects. */
export interface HookView {
  /** The receiving seat's player id. */
  you: string;
  /** Server-owned turn number (the client never counts turns). */
  turn: number;
  /** Current step. */
  phase: string;
  /** The seat whose turn it is. */
  active_player: string;
  /** The seat holding priority, when one does. */
  priority_player?: string;
  /** Every seat in table order, eliminated seats included. */
  seat_order: string[];
  /** The only source of interactivity. */
  valid_actions: HookAction[];
  /** The receiver's hand. */
  my_hand: Array<{ id: string; name: string }>;
  /** The receiver's own public numbers. */
  me: { life: number; library_size: number };
  /** Every other seat, redacted. */
  opponents: Array<{ player_id: string; life: number; eliminated?: boolean }>;
  /** The receiver's unspent mana, as pip strings. */
  mana_pool: string[];
  /**
   * Every permanent in play. Note a permanent's entity id is **not** the id the
   * same card carried in hand — zone changes mint a new object — so the suite
   * identifies a freshly played permanent by controller + face name, never by
   * carrying the hand id across the zone change.
   */
  battlefield: HookPermanent[];
  /** The stack, bottom first. */
  stack: Array<{ id: string; controller: string; description: string }>;
  /** Each seat's graveyard. */
  graveyards: Array<{ player_id: string; cards: Array<{ name: string; power?: string }> }>;
  /** The terminal result, once the server has decided one. */
  result?: { winner?: string; losers?: string[]; reason?: string };
}

/** One individually addressable object staged on the scene plane. */
export interface HookRender {
  /** The render's representative entity id. */
  entityId: string;
  /** Every permanent this render stands for (>1 only when folded ×N). */
  memberIds: string[];
  /** Display name, as the plane labels it. */
  name: string;
  /** Whether the staged object is attacking (combat treatment; never folded). */
  attacking: boolean;
  /** Whether the staged object is blocking. */
  blocking: boolean;
  /** Whether it is one of the active prompt's candidates. */
  candidate: boolean;
}

/** One seat's staged region (receiver band, far side, or wing). */
export interface HookRegion {
  /** The seat this region belongs to. */
  seat: string;
  /** Which fixed slot group it occupies. */
  kind: string;
  /** The individually addressable renders staged in the slot. */
  renders: HookRender[];
  /** Life total, straight from the personalized view. */
  life: number;
  /** Whether any attacker is attacking this seat (crest ring, at every rung). */
  attacked: boolean;
  /** Whether this seat is the active player. */
  active: boolean;
  /** Whether this seat has been eliminated. */
  eliminated: boolean;
}

/** The subset of `StagedPlane` this suite inspects. */
export interface HookPlane {
  /** The receiver's own band — the "local band" a played permanent lands in. */
  receiver?: HookRegion;
  /** The focused opponent's expanded board. */
  farSide?: HookRegion;
  /** Peripheral opponents' wings, in stable seat order. */
  wings: HookRegion[];
  /** Compact summary tiles (phone-portrait rung only). */
  tiles: Array<{ seat: string; attacked: boolean }>;
  /** Every staged seat in staging order. */
  seats: string[];
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
