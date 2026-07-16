/**
 * Inline style objects for the React DOM layer of the table (ADR 0003: prompts,
 * action bar, player tiles, and the interactive overlay are DOM, not canvas).
 *
 * Card colors and sizes always come from `src/tokens.ts`; the values here are UI
 * chrome (bars, tiles, spacing) — never card characteristics. Where the DOM must
 * agree with a card token (the selection ring, the board backdrop) it reads the
 * shared token so both renderers stay in lockstep.
 */
import type { CSSProperties } from 'react';
import { SURFACES } from '../tokens';
import type { Rect } from './scene';

/** Minimum touch target per AGENTS.md (44px), applied to every affordance. */
const TOUCH = 44;

export const main: CSSProperties = {
  minHeight: '100vh',
  background: SURFACES.board,
  color: SURFACES.nameText,
  fontFamily: 'system-ui, sans-serif',
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 8,
  boxSizing: 'border-box',
};

export const banner: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 16,
  alignItems: 'center',
  padding: '10px 14px',
  borderRadius: 8,
  background: '#1E2126',
  fontSize: 14,
};

export const bannerAccent: CSSProperties = { color: SURFACES.selection, fontWeight: 600 };

/** The lead span of the targeting-mode banner, in the shared targeting color. */
export const bannerTargeting: CSSProperties = { color: SURFACES.targeting, fontWeight: 600 };

/**
 * The modal option picker inside the prompt banner (issue #157): the option prompt
 * plus its named-choice buttons, grouped so a keep/mulligan-style decision reads as
 * one contained choice rather than scattered chrome.
 */
export const bannerOptions: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 8,
  marginLeft: 'auto',
  paddingLeft: 12,
  borderLeft: '1px solid #3A4049',
};

/** One named-choice button in the banner's modal option picker. */
export const optionButton: CSSProperties = {
  minHeight: TOUCH,
  padding: '0 16px',
  borderRadius: 8,
  border: `1px solid ${SURFACES.selection}`,
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 14,
  fontWeight: 600,
  cursor: 'pointer',
};

/**
 * The prompt surface panel (issue #157): a DOM list overlay for a `select_from_zone`
 * whose zone is not on the board (graveyard/library) and for an `order` arrange
 * list. Text a user reads/clicks is DOM (ADR 0003); it reads the shared `SURFACES`
 * palette and never touches card color/size tokens.
 */
export const promptSurface: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: '12px 14px',
  borderRadius: 8,
  background: '#1E2126',
  border: `1px solid ${SURFACES.targeting}`,
  alignSelf: 'center',
  width: '100%',
  maxWidth: 420,
  boxSizing: 'border-box',
};

/** The prompt surface heading (the server's slot prompt). */
export const promptSurfaceTitle: CSSProperties = {
  margin: 0,
  fontSize: 14,
  fontWeight: 700,
  color: SURFACES.nameText,
};

/** The zone context sub-line (e.g. "Graveyard"). */
export const promptSurfaceZone: CSSProperties = {
  fontSize: 12,
  color: SURFACES.typeText,
  textTransform: 'capitalize',
};

/** The vertical list of candidate / order rows. */
export const promptSurfaceList: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  margin: 0,
  padding: 0,
  listStyle: 'none',
};

/** One row in the prompt surface: a card name with its controls. */
export const promptSurfaceRow: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 8,
  padding: '6px 8px',
  borderRadius: 8,
  background: '#15171A',
  border: '1px solid #2C313A',
  minHeight: TOUCH,
  boxSizing: 'border-box',
};

/** A prompt-surface row that is currently chosen (select mode), ringed in accent. */
export const promptSurfaceRowChosen: CSSProperties = {
  borderColor: SURFACES.selection,
  boxShadow: `0 0 0 1px ${SURFACES.selection}`,
};

/** The card-name label within a prompt-surface row (grows to push controls right). */
export const promptSurfaceName: CSSProperties = {
  flex: 1,
  fontSize: 13,
  fontWeight: 600,
  color: SURFACES.nameText,
};

/** The 1-based position badge shown on an order row. */
export const promptSurfaceIndex: CSSProperties = {
  fontSize: 12,
  fontWeight: 700,
  color: SURFACES.typeText,
  minWidth: 18,
  textAlign: 'right',
};

/** A compact square control button (toggle / move up / move down) in a row. */
export const promptSurfaceControl: CSSProperties = {
  minHeight: TOUCH,
  minWidth: TOUCH,
  padding: '0 10px',
  borderRadius: 8,
  border: '1px solid #3A4049',
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 14,
  fontWeight: 600,
  cursor: 'pointer',
};

export const tiles: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 8,
};

export const tile: CSSProperties = {
  minWidth: 140,
  padding: '8px 12px',
  borderRadius: 8,
  background: '#1E2126',
  border: '1px solid #2C313A',
  fontSize: 13,
  lineHeight: 1.5,
};

export const localTile: CSSProperties = {
  borderColor: SURFACES.selection,
};

/** A player tile that is a legal target during targeting mode: ringed + pickable. */
export const targetTile: CSSProperties = {
  borderColor: SURFACES.targeting,
  boxShadow: `0 0 0 1px ${SURFACES.targeting}`,
  cursor: 'pointer',
};

/** A player tile dimmed as an ineligible target during targeting mode. */
export const dimmedTile: CSSProperties = {
  opacity: 0.32,
};

/**
 * Reset applied to a `<button>` wrapping a player tile so it keeps the tile's own
 * box/typography rather than the browser's default button chrome. The tile styles
 * (including {@link targetTile}) are spread on top.
 */
export const tileButtonReset: CSSProperties = {
  font: 'inherit',
  color: 'inherit',
  textAlign: 'left',
  padding: 0,
  margin: 0,
  background: 'none',
  border: 'none',
};

export const tileName: CSSProperties = {
  fontWeight: 600,
  fontSize: 14,
  marginBottom: 2,
};

export function boardWrap(width: number, height: number): CSSProperties {
  return {
    position: 'relative',
    width,
    height,
    maxWidth: '100%',
    overflowX: 'auto',
    alignSelf: 'center',
  };
}

export function overlay(width: number, height: number): CSSProperties {
  return {
    position: 'absolute',
    inset: 0,
    width,
    height,
    // The overlay itself passes pointer events through; only its buttons catch
    // them, so empty board space never intercepts clicks.
    pointerEvents: 'none',
  };
}

export function hotspot(rect: Rect, selected: boolean): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y,
    width: Math.max(rect.w, TOUCH),
    height: Math.max(rect.h, TOUCH),
    minWidth: TOUCH,
    minHeight: TOUCH,
    padding: 0,
    background: 'transparent',
    border: selected ? `2px solid ${SURFACES.selection}` : '2px solid transparent',
    borderRadius: 10,
    cursor: 'pointer',
    pointerEvents: 'auto',
  };
}

/**
 * A target-pick hotspot for targeting mode: the same touch-sized hitbox as a
 * selection {@link hotspot}, ringed and faintly filled in the shared targeting
 * color so a legal target reads as pickable. Ineligible cards get no hotspot at
 * all (they are dimmed in the canvas), so only candidates catch a click.
 *
 * In a multi-select an already-chosen candidate is `chosen`: it fills more solidly
 * (in the shared selection accent) so a toggled pick reads as committed, while
 * unchosen candidates keep the lighter targeting fill.
 */
export function targetHotspot(rect: Rect, chosen = false): CSSProperties {
  return {
    ...hotspot(rect, false),
    border: `2px solid ${chosen ? SURFACES.selection : SURFACES.targeting}`,
    background: chosen ? 'rgba(127, 178, 229, 0.24)' : 'rgba(224, 120, 74, 0.14)',
  };
}

export function entityActions(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x,
    top: rect.y + rect.h + 4,
    display: 'flex',
    flexWrap: 'wrap',
    gap: 4,
    pointerEvents: 'auto',
    zIndex: 2,
  };
}

/**
 * Card inspect popover (issue #261, React DOM per ADR 0003 — oracle text a user
 * reads is DOM, never the Pixi canvas). A universal, reusable surface that reads
 * only `CardView`/`StackItem` fields the server already sent; it never touches the
 * card color/size tokens (that is the renderers' job) and reads the shared
 * `SURFACES` palette like the rest of the table chrome.
 */
export const inspectBackdrop: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  padding: 16,
  background: 'rgba(9, 10, 12, 0.66)',
  zIndex: 20,
};

export const inspectPanel: CSSProperties = {
  position: 'relative',
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  width: '100%',
  maxWidth: 340,
  maxHeight: '80vh',
  overflowY: 'auto',
  padding: 20,
  borderRadius: 12,
  background: '#1E2126',
  border: '1px solid #2C313A',
  boxShadow: '0 12px 40px rgba(0,0,0,0.5)',
  boxSizing: 'border-box',
};

/** The close (×) control, anchored top-right of the panel. */
export const inspectClose: CSSProperties = {
  position: 'absolute',
  top: 8,
  right: 8,
  minWidth: TOUCH,
  minHeight: TOUCH,
  padding: 0,
  borderRadius: 8,
  border: 'none',
  background: 'transparent',
  color: SURFACES.typeText,
  fontSize: 22,
  lineHeight: 1,
  cursor: 'pointer',
};

export const inspectName: CSSProperties = {
  margin: 0,
  paddingRight: TOUCH,
  fontSize: 18,
  fontWeight: 700,
  color: SURFACES.nameText,
};

/** The mana cost line, in the muted secondary color (rendered verbatim). */
export const inspectCost: CSSProperties = {
  fontSize: 14,
  fontWeight: 600,
  color: SURFACES.typeText,
};

export const inspectTypeLine: CSSProperties = {
  fontSize: 13,
  color: SURFACES.typeText,
};

/** The current (effective) power/toughness line. */
export const inspectPt: CSSProperties = {
  fontSize: 15,
  fontWeight: 700,
  color: SURFACES.nameText,
};

/** The keyword badge row. */
export const inspectKeywords: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 6,
};

/** One keyword badge. */
export const inspectKeyword: CSSProperties = {
  fontSize: 12,
  fontWeight: 600,
  padding: '2px 8px',
  borderRadius: 999,
  background: '#2A2F37',
  color: SURFACES.nameText,
  border: '1px solid #3A4049',
};

/** The oracle/rules text block, wrapping across lines. */
export const inspectRules: CSSProperties = {
  margin: '4px 0 0',
  fontSize: 14,
  lineHeight: 1.5,
  color: SURFACES.nameText,
  whiteSpace: 'pre-wrap',
};

/** The placeholder shown when a card has no rules text. */
export const inspectNoText: CSSProperties = {
  ...inspectRules,
  fontStyle: 'italic',
  color: SURFACES.typeText,
};

/** The row of dynamic-state badges (tapped, counters, controller). */
export const inspectStateRow: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 6,
  marginTop: 4,
};

/** One dynamic-state badge. */
export const inspectState: CSSProperties = {
  fontSize: 12,
  fontWeight: 600,
  padding: '2px 8px',
  borderRadius: 6,
  background: '#15171A',
  color: SURFACES.typeText,
  border: '1px solid #2C313A',
};

/**
 * The inspect handle drawn ON a card (issue #261): a small, touch-sized affordance
 * anchored at the card's top-right corner that opens the inspect popover. It is a
 * DISTINCT control from the select/target hotspot beneath it (its own testid and a
 * higher stacking context), so a card can be inspected without disturbing its
 * select/target interaction — the two coexist (a card stays both inspectable and
 * toggleable in targeting/multi-select).
 */
export function inspectHandle(rect: Rect): CSSProperties {
  return {
    position: 'absolute',
    left: rect.x + rect.w - TOUCH + 6,
    top: rect.y - 6,
    width: TOUCH,
    height: TOUCH,
    padding: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 999,
    border: `1px solid ${SURFACES.selection}`,
    background: 'rgba(21, 23, 26, 0.9)',
    color: SURFACES.nameText,
    fontSize: 15,
    fontWeight: 700,
    lineHeight: 1,
    cursor: 'pointer',
    pointerEvents: 'auto',
    zIndex: 3,
  };
}

/** A compact inspect handle for a DOM row (stack entry): inline, not absolute. */
export const inspectRowHandle: CSSProperties = {
  minWidth: TOUCH,
  minHeight: TOUCH,
  padding: 0,
  borderRadius: 999,
  border: `1px solid ${SURFACES.selection}`,
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 15,
  fontWeight: 700,
  cursor: 'pointer',
  flexShrink: 0,
};

export const bar: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 8,
  padding: '8px 12px',
  borderRadius: 8,
  background: '#1E2126',
  minHeight: TOUCH + 12,
};

export const button: CSSProperties = {
  minHeight: TOUCH,
  minWidth: TOUCH,
  padding: '0 16px',
  borderRadius: 8,
  border: '1px solid #3A4049',
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 14,
  fontWeight: 600,
  cursor: 'pointer',
};

export const chip: CSSProperties = {
  minHeight: TOUCH,
  padding: '0 12px',
  borderRadius: 8,
  border: `1px solid ${SURFACES.selection}`,
  background: '#2A2F37',
  color: SURFACES.nameText,
  fontSize: 13,
  fontWeight: 600,
  cursor: 'pointer',
  boxShadow: '0 2px 6px rgba(0,0,0,0.4)',
};

export const echo: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  flexWrap: 'wrap',
  gap: 8,
  marginLeft: 'auto',
  paddingLeft: 12,
  borderLeft: '1px solid #3A4049',
};

export const echoLabel: CSSProperties = {
  fontSize: 13,
  color: SURFACES.typeText,
};

export const muted: CSSProperties = {
  fontSize: 13,
  color: SURFACES.typeText,
};

/**
 * Game-over overlay (issue #141). A DOM modal (ADR 0003: text a user reads is
 * DOM, not canvas) laid over the final board, announcing the terminal result. It
 * is pure render output of the latest `GameView.result` — no card tokens, only UI
 * chrome reading the shared `SURFACES` palette.
 */
export const gameOverBackdrop: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  padding: 16,
  background: 'rgba(9, 10, 12, 0.72)',
  zIndex: 10,
};

export const gameOverPanel: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  gap: 10,
  width: '100%',
  maxWidth: 420,
  padding: 28,
  borderRadius: 14,
  background: '#1E2126',
  border: '1px solid #2C313A',
  boxShadow: '0 12px 40px rgba(0,0,0,0.5)',
  textAlign: 'center',
  boxSizing: 'border-box',
};

/** The headline verdict (Victory / Defeat / Draw). */
export const gameOverHeadline: CSSProperties = {
  margin: 0,
  fontSize: 30,
  fontWeight: 800,
  letterSpacing: 0.5,
};

/** Victory tint (shared selection accent). */
export const gameOverWin: CSSProperties = { color: SURFACES.selection };

/** Defeat tint (shared alert/targeting accent). */
export const gameOverLoss: CSSProperties = { color: SURFACES.targeting };

/** Draw / neutral tint. */
export const gameOverNeutral: CSSProperties = { color: SURFACES.nameText };

/** The winner/draw sub-line naming who won. */
export const gameOverWinner: CSSProperties = {
  margin: 0,
  fontSize: 16,
  fontWeight: 600,
  color: SURFACES.nameText,
};

/** The reason line, in the muted secondary color. */
export const gameOverReason: CSSProperties = {
  margin: 0,
  fontSize: 14,
  color: SURFACES.typeText,
};

/**
 * Stack panel (React DOM, ADR 0003 — the stack is text a user reads, so it is DOM
 * chrome, not canvas). Pure render of `GameView.stack`, bottom-first on the wire
 * and shown top-first so the object that resolves next reads at the top. Like all
 * chrome here it reads the shared `SURFACES` palette and never touches card tokens.
 */
export const stackPanel: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding: '10px 12px',
  borderRadius: 8,
  background: '#1E2126',
  border: '1px solid #2C313A',
  maxWidth: 360,
};

/** The panel heading, with the object count. */
export const stackTitle: CSSProperties = {
  margin: 0,
  fontSize: 13,
  fontWeight: 700,
  color: SURFACES.typeText,
  letterSpacing: 0.4,
  textTransform: 'uppercase',
};

/** The ordered list of stack entries. */
export const stackList: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  margin: 0,
  padding: 0,
  listStyle: 'none',
};

/**
 * A stack list row: lays the entry (or its target button) beside its inspect
 * handle. The entry grows to fill; the handle stays a fixed touch-sized control.
 */
export const stackItemRow: CSSProperties = {
  display: 'flex',
  alignItems: 'stretch',
  gap: 6,
};

/** One stack entry (spell or ability). */
export const stackItem: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 3,
  padding: '8px 10px',
  borderRadius: 8,
  background: '#15171A',
  border: '1px solid #2C313A',
  fontSize: 13,
  lineHeight: 1.4,
};

/** The top of the stack — the object that resolves next, ringed in the accent. */
export const stackItemTop: CSSProperties = {
  borderColor: SURFACES.selection,
  boxShadow: `0 0 0 1px ${SURFACES.selection}`,
};

/** A stack entry that is a legal target during targeting mode: ringed + pickable. */
export const stackTargetItem: CSSProperties = {
  borderColor: SURFACES.targeting,
  boxShadow: `0 0 0 1px ${SURFACES.targeting}`,
  background: 'rgba(224, 120, 74, 0.14)',
  cursor: 'pointer',
};

/**
 * Reset for a `<button>` wrapping a stack entry so it keeps the entry's own box and
 * typography rather than the browser's default button chrome (mirrors
 * {@link tileButtonReset} for player tiles). The item styles are spread on top.
 */
export const stackItemButtonReset: CSSProperties = {
  font: 'inherit',
  color: 'inherit',
  textAlign: 'left',
  width: '100%',
  margin: 0,
};

/** The spell name or ability text line (the primary label of an entry). */
export const stackItemName: CSSProperties = {
  fontWeight: 600,
  color: SURFACES.nameText,
};

/** A secondary meta line (controller, source), in the muted color. */
export const stackItemMeta: CSSProperties = {
  fontSize: 12,
  color: SURFACES.typeText,
};

/** The row of small badges on an entry (kind, top-of-stack marker). */
export const stackBadges: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 6,
  alignItems: 'center',
};

/** A small pill labelling an entry's kind (Spell / Ability). */
export const stackKindBadge: CSSProperties = {
  fontSize: 11,
  fontWeight: 700,
  padding: '1px 7px',
  borderRadius: 999,
  background: '#2A2F37',
  color: SURFACES.typeText,
  border: '1px solid #3A4049',
};

/** The "resolves next" marker on the top entry, in the accent color. */
export const stackTopBadge: CSSProperties = {
  ...stackKindBadge,
  color: SURFACES.selection,
  borderColor: SURFACES.selection,
};

/** The pre-first-frame waiting row: status text alongside a Disconnect action. */
export const waitingBar: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 12,
  padding: '10px 14px',
  borderRadius: 8,
  background: '#1E2126',
};

/**
 * Connection-screen chrome. Not a card — these read the shared `SURFACES` tokens
 * the same way the table chrome above does, so the pre-game screen matches the
 * board's look without ever touching card color/size tokens.
 */
export const connectMain: CSSProperties = {
  ...main,
  justifyContent: 'center',
  alignItems: 'center',
};

export const connectPanel: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 16,
  width: '100%',
  maxWidth: 420,
  padding: 24,
  borderRadius: 12,
  background: '#1E2126',
  border: '1px solid #2C313A',
  boxSizing: 'border-box',
};

export const connectHeading: CSSProperties = {
  margin: 0,
  fontSize: 20,
  fontWeight: 700,
};

export const field: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
};

export const fieldLabel: CSSProperties = {
  fontSize: 13,
  color: SURFACES.typeText,
  fontWeight: 600,
};

export const input: CSSProperties = {
  minHeight: TOUCH,
  padding: '0 12px',
  borderRadius: 8,
  border: '1px solid #3A4049',
  background: '#15171A',
  color: SURFACES.nameText,
  fontSize: 15,
  fontFamily: 'inherit',
  boxSizing: 'border-box',
  width: '100%',
};

export const buttonRow: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 8,
};

/** Emphasized closed/error status, in the shared alert (targeting) color. */
export const errorText: CSSProperties = {
  fontSize: 13,
  color: SURFACES.targeting,
  fontWeight: 600,
};

/**
 * Lobby chrome (issue #114). The pre-game lobby screen reuses the connection
 * screen's panel/field/button vocabulary above; these add only the few shapes the
 * lobby needs (a wider panel, the room-id row, and the per-seat roster). Like all
 * the chrome here they read `SURFACES` tokens and never touch card color/size.
 */
export const lobbyPanel: CSSProperties = {
  ...connectPanel,
  maxWidth: 560,
};

/** A native `<select>` styled to match {@link input}. */
export const select: CSSProperties = {
  ...input,
  cursor: 'pointer',
};

/** A subtle grouping card inside the lobby panel (create/join/room sections). */
export const lobbySection: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 12,
  padding: 16,
  borderRadius: 10,
  background: '#15171A',
  border: '1px solid #2C313A',
};

/** Section heading inside the lobby panel. */
export const lobbySectionTitle: CSSProperties = {
  margin: 0,
  fontSize: 15,
  fontWeight: 700,
};

/** The copyable room-id row: monospace id + a Copy affordance. */
export const roomIdRow: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 8,
};

/** The room id rendered as a selectable, monospace code chip. */
export const roomIdCode: CSSProperties = {
  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
  fontSize: 15,
  fontWeight: 700,
  padding: '6px 10px',
  borderRadius: 8,
  background: '#0F1114',
  border: '1px solid #3A4049',
  userSelect: 'all',
  wordBreak: 'break-all',
};

/** The seat roster list. */
export const seatList: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  margin: 0,
  padding: 0,
  listStyle: 'none',
};

/** One seat row in the roster. */
export const seatRow: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  alignItems: 'center',
  gap: 8,
  padding: '8px 12px',
  borderRadius: 8,
  background: '#1E2126',
  border: '1px solid #2C313A',
  fontSize: 13,
};

/** The local player's seat, ringed in the shared selection color. */
export const seatRowLocal: CSSProperties = {
  borderColor: SURFACES.selection,
};

/** A small status badge on a seat row (filled / decked / ready). */
export const seatBadge: CSSProperties = {
  fontSize: 12,
  fontWeight: 600,
  padding: '2px 8px',
  borderRadius: 999,
  background: '#2A2F37',
  color: SURFACES.typeText,
  border: '1px solid #3A4049',
};

/** A status badge for an affirmative state (decked / ready), in the accent color. */
export const seatBadgeOn: CSSProperties = {
  ...seatBadge,
  color: SURFACES.selection,
  borderColor: SURFACES.selection,
};

/** Pushes trailing seat badges to the right of the seat row. */
export const seatBadges: CSSProperties = {
  display: 'flex',
  flexWrap: 'wrap',
  gap: 6,
  marginLeft: 'auto',
};
