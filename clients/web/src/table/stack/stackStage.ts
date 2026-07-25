/**
 * The **contextual stack stage**'s pure derivation — `docs/design/stack-and-relationships.md`
 * §1–§3, for issue #534 under
 * [ADR 0032](../../../../../docs/decisions/0032-contextual-shell-anatomy.md).
 *
 * ADR 0032 removed the permanent right rail, so the stack no longer has a carved
 * column to sit in. It becomes a surface that is **absent when the stack is
 * empty** and appears, at the right flank, the moment the server puts something
 * there. This module answers the whole of "what does the stage show for this
 * view" without touching the DOM, so the ladder — pile at depth 1–5, condensed
 * row rail at 6–8, scrolling rail beyond — is unit-testable rather than
 * inspected in a browser (§13 IN5/IN7).
 *
 * ## What it may and may not decide
 *
 * Everything here is a *presentation* transform over fields the server already
 * sent. The one arithmetic it performs on game data is the **display reversal**
 * of §3.3: `GameView.stack` is bottom-first on the wire and the stage reads
 * top-first, so `n = stack.length - i`. That is a reversal of a server-supplied
 * array, not derived game state. It never decides what resolves, what an object
 * does, or whether an entry is a legal target — the last of those arrives as
 * server-listed candidates and is the component's business (`StackStage.tsx`).
 *
 * Text is always the server's (§2.4 rule 3). {@link StackStageEntry.description}
 * is `StackItem.description` verbatim; the subtitle and the accessible name only
 * *concatenate* server fields with the array position.
 *
 * ## The tiers, and the one entry that is Expanded
 *
 * §2.1 and §3.1 read slightly differently on the deep case — §2.1 puts *every*
 * entry at depth ≥ 6 on the Row tier, §3.1 keeps the top entry Expanded above
 * the rail. §3.1 wins here because it is the section that specifies the ladder,
 * and because "what resolves next" is the one thing the stage exists to answer.
 * The two agree on the invariant that matters: **exactly one entry is Expanded
 * at every depth**. Focus therefore *moves* the Expanded tier rather than adding
 * a second one — a focused non-top entry promotes and the top steps down, which
 * is also the only reading under which §2.1's "focus always promotes exactly one
 * entry" is true.
 *
 * ## What is not here, and why
 *
 * The Expanded anatomy of §2.2 is mostly **dormant**: `StackItem` carries no card
 * face (gap G4), no targets (G1), no activated/triggered discriminator (G2), and
 * no copy relation (G3). Those are `#550`'s protocol changes. Rather than invent
 * them, the stage renders the four channels §2.4 rule 2 says may never degrade —
 * controller stripe, order index, kind marker, and (when it exists) the source
 * tether — and leaves the rest absent. Absence is stated positively where the
 * player could otherwise infer something false: an ability whose source has left
 * play says so (§14 C5) instead of printing a raw entity id.
 *
 * Consumed by {@link StackStage} (`StackStage.tsx`), which is the only production
 * caller and turns every field below into DOM.
 */
import type { CSSProperties } from 'react';
import type { EntityId, GameView, PlayerId, StackItem } from '../../protocol';
import { playerName } from '../../playerNames';
import { identityAccent } from '../identityAccents';
import { CONTROL } from '../controls';

/** The three density tiers of §2.1. */
export type StackTier = 'expanded' | 'mini' | 'row';

/**
 * How the stage is laid out at this depth and geometry (§3.1, §10.4).
 *
 * `pile` and `rail` are a **change of kind**, not a shrink (D4): recession stops
 * and rows begin at the collapse point.
 */
export type StackLayout = 'pile' | 'rail' | 'sheet';

/** Whether an entry is pinned out of the scroll (§3.2, D5). */
export type StackSticky = 'head' | 'foot' | null;

/** The subset of a `GameView` the stage reads. */
export type StackStageView = Pick<
  GameView,
  'stack' | 'you' | 'player_names' | 'battlefield' | 'seat_order' | 'opponents'
>;

/** Options the shell supplies alongside the view. */
export interface StackStageOptions {
  /** Compact geometry (phone portrait / landscape below the tablet floor, §10.4). */
  compact?: boolean;
  /** The entry the player has focused, which promotes to Expanded (§9.3). */
  focusId?: EntityId | null;
}

/** One rendered `StackItem`, with everything the slot needs to draw itself. */
export interface StackStageEntry {
  /** `StackItem.id` — the id targeting candidates and inspect are keyed by. */
  id: EntityId;
  /** 1-based display position, top-first: the `n` of `n/N` (§3.3). */
  index: number;
  /** The `N` of `n/N`; the stage header supplies it for tiers that show only `n`. */
  total: number;
  /** The top of the stack — the object that resolves next. */
  isTop: boolean;
  /** The density tier this entry draws at (§2.1). */
  tier: StackTier;
  /**
   * Spell or ability. `StackItem.source`'s presence is the only discriminator the
   * protocol offers today; activated vs triggered is gap G2 and is not guessed.
   */
  kind: 'spell' | 'ability';
  /** `StackItem.description`, verbatim. The client never composes rules prose. */
  description: string;
  /** `StackItem.controller`. */
  controller: PlayerId;
  /** The controller's display name, or `You` for the receiver. */
  controllerName: string;
  /** Whether the receiver controls this object (`controller === view.you`). */
  isMine: boolean;
  /** The controller's seat accent — worn by the **slot**, never the card face (§9.1). */
  accent: string;
  /** An ability's source permanent, when it has one. */
  sourceId?: EntityId;
  /** The source permanent's name, when it is still a visible permanent. */
  sourceName?: string;
  /**
   * Whether {@link sourceId} resolved to a permanent on the battlefield. `false`
   * means the source has left play or is hidden: §14 C5's plate state, drawn as
   * "source has left play" rather than as a raw entity id.
   */
  sourceResolved: boolean;
  /** The kind glyph drawn on the Row tier's tile (§2.1). */
  glyph: string;
  /** The `kind · controller` subtitle, transcribed from the deep-rail mock. */
  subtitle: string;
  /** The accessible name, assembled in §9.2's fixed order. */
  label: string;
  /** Cumulative leftward splay in px; `0` outside the `pile` layout. */
  offsetX: number;
  /** Cumulative recession scale, floored at {@link STACK_STAGE.splayScaleFloor}. */
  scale: number;
  /**
   * Paint order inside the pile — higher is nearer the front, so the top of the
   * stack is `total`. The pile's DOM order is top-first (§9.2 needs it for the
   * reading order and the roving focus) while its *visual* order runs the other
   * way, so paint order cannot be left to the document.
   */
  zOrder: number;
  /** Pinned out of the scroll at the head or foot of a deep rail (§3.2). */
  sticky: StackSticky;
}

/** Everything the stage component needs for one view. */
export interface StackStageModel {
  /**
   * Whether the stage renders at all. **False for an empty stack** — zero nodes,
   * zero reserved space, no placeholder (§1.2, and the issue's "empty stack
   * consumes no permanent battlefield width").
   */
  present: boolean;
  /** `GameView.stack.length`. */
  count: number;
  /** The layout kind at this depth and geometry. */
  layout: StackLayout;
  /** The header line, transcribed verbatim from the deep-rail and phone mocks. */
  header: string;
  /** The stage's accessible name (§9.2). */
  ariaLabel: string;
  /** Every entry, top-first. */
  entries: StackStageEntry[];
  /** Whether the rail scrolls (depth > 8) — the `+K more` divider's precondition. */
  scrolls: boolean;
  /** How many entries are outside the unscrolled window; `0` when nothing hides. */
  hiddenCount: number;
}

/**
 * Stage geometry in CSS px (§2.1, §3.1) plus the placement the stage inherits
 * from the control language.
 *
 * Width and the right margin are **the control cluster's**, not §1.2's viewport
 * fractions. `control-language.md` §4.4/D7 is explicit that the stack column and
 * the cluster are one column with the cluster at its foot, and D1 fixes controls
 * to CSS px rather than viewport fractions; a stage that floated on `0.185 · W`
 * would drift off the column it is supposed to sit on. 268 px lies inside §1.2's
 * own `clamp(232px, …, 300px)` band at every viewport, so nothing is lost.
 */
export const STACK_STAGE = {
  /** The column width — the cluster's, so the RESOLVE/RESPOND pair reads against it. */
  width: CONTROL.wCluster,
  /** Inset from the viewport's right edge — the cluster's. */
  margin: CONTROL.clusterMargin,
  /** Gap between the stage and the cluster below it, and between rail rows' groups. */
  gap: CONTROL.clusterGap,
  /** Expanded tier height. */
  expandedH: 168,
  /** Mini tier height. */
  miniH: 92,
  /** Row tier height. Above the 44 px hit floor by construction. */
  rowH: 48,
  /** Gap between rail rows. */
  rowGap: 4,
  /** Per-step leftward splay offset in the pile layout. */
  splayDx: -10,
  /**
   * The per-step vertical offset of §3.1, expressed as the **peek**: how much of
   * a receding slot stays exposed above the one in front of it. The specification
   * writes it as a `-14 px` offset, but in a physical pile a per-entry offset
   * *is* the exposed strip — and it has to be layout, not a transform, or the
   * slots would not overlap and the pile would grow without bound.
   */
  peek: 14,
  /** Per-step recession scale, and the floor it never goes below. */
  splayScale: 0.94,
  splayScaleFloor: 0.8,
  /** The collapse point: at this depth the pile becomes a row rail (D4). */
  collapseDepth: 6,
  /** Rows visible below the Expanded top entry before the rail scrolls. */
  visibleRows: 7,
  /** Entries pinned at the head of a scrolled rail (D5). */
  stickyHead: 3,
  /** Beyond this depth the bottom-most entry is pinned too, so both ends show. */
  footStickyDepth: 10,
  /** The compact sheet's height ceiling, as a fraction of the viewport (§10.4). */
  compactHeightFraction: 0.42,
  /** The collapsed sheet handle's drawn plate height (§10.4). */
  handleH: 32,
  /** The interactive floor every row and the handle's hit box respect. */
  hit: CONTROL.hit,
  /**
   * The full-history panel's width. Not a transcribed number — the specification
   * does not draw the history surface at all, because under ADR 0023 it was a
   * permanent column. Sized to two cluster columns so a composed log line has
   * room for its clickable references without becoming a full-screen reader.
   */
  historyW: 2 * CONTROL.wCluster,
} as const;

/**
 * The vertical space the stage leaves free at the bottom of the flank for the
 * control cluster (`control-language.md` §3.3), so the stage grows **upward** and
 * never covers the primary.
 *
 * Computed from the cluster's tallest form — the full stadium primary, a utility
 * row, and the phase plaque, plus the cluster's own margin — rather than the
 * compact pair the cluster switches to while the stack is non-empty (D7). Taking
 * the taller of the two means the stage clears the cluster in *both* forms, and
 * that a change to the cluster's composition can only ever leave slack, never
 * cause an overlap.
 */
export const CLUSTER_CLEARANCE =
  CONTROL.clusterMargin +
  CONTROL.plaqueH +
  CONTROL.clusterGap +
  CONTROL.hit +
  CONTROL.clusterGap +
  CONTROL.hPrimary;

/**
 * The `--stack-*` custom properties `stack.module.css` lays out from — the same
 * discipline `shellLayout.ts`'s {@link shellStyleVars} applies to the shell, and
 * for the same reason: the stylesheet then declares no dimensional literal of
 * its own, so the geometry a test reasons about is the geometry that ships
 * (ADR 0019).
 *
 * Applied to both surfaces in this directory, so the activity badge sits on the
 * same column and the same margin as the stage it shares a flank with.
 */
export function stackStyleVars(): CSSProperties {
  return {
    '--stack-width': `${STACK_STAGE.width}px`,
    '--stack-margin': `${STACK_STAGE.margin}px`,
    '--stack-clearance': `${CLUSTER_CLEARANCE}px`,
    '--stack-expanded-h': `${STACK_STAGE.expandedH}px`,
    '--stack-mini-h': `${STACK_STAGE.miniH}px`,
    '--stack-row-h': `${STACK_STAGE.rowH}px`,
    '--stack-row-gap': `${STACK_STAGE.rowGap}px`,
    '--stack-peek': `${STACK_STAGE.peek}px`,
    '--stack-rail-h': `${railViewportHeight()}px`,
    '--stack-sheet-max-h': `${STACK_STAGE.compactHeightFraction * 100}%`,
    '--stack-history-w': `${STACK_STAGE.historyW}px`,
  } as CSSProperties;
}

/** The scroll viewport a deep rail's row list is capped at (§3.1, depth > 8). */
export function railViewportHeight(): number {
  return STACK_STAGE.visibleRows * (STACK_STAGE.rowH + STACK_STAGE.rowGap);
}

/** The height one entry occupies at its tier, excluding the inter-row gap. */
export function tierHeight(tier: StackTier): number {
  if (tier === 'expanded') return STACK_STAGE.expandedH;
  if (tier === 'mini') return STACK_STAGE.miniH;
  return STACK_STAGE.rowH;
}

/**
 * Resolve an ability's `source` permanent to a display name from the current
 * battlefield. Carried from the shipped `StackPanel`, with one change: a source
 * that does not resolve is reported as unresolved (§14 C5) instead of falling
 * back to the raw opaque id, which reads as noise to a player and as a card name
 * to nobody.
 */
function resolveSource(
  view: StackStageView,
  source: EntityId,
): { name: string | undefined; resolved: boolean } {
  const name = view.battlefield.find((perm) => perm.id === source)?.card.name;
  return { name, resolved: name !== undefined };
}

/** The controller's label: `You` for the receiver, else their display name. */
function controllerLabel(view: StackStageView, controller: PlayerId): string {
  return controller === view.you ? 'You' : playerName(view, controller);
}

/**
 * The tier for one entry.
 *
 * Compact geometry is all-Row by §10.4 (the phone mock proves depth 8 at 390 px
 * that way); focus is the one thing that promotes an entry above it.
 */
function tierFor(args: {
  compact: boolean;
  count: number;
  isTop: boolean;
  focused: boolean;
  anyFocus: boolean;
}): StackTier {
  const { compact, count, isTop, focused, anyFocus } = args;
  // Exactly one Expanded entry: the focused one when there is a focus, else the
  // top. A focused non-top entry therefore demotes the top rather than joining it.
  if (focused) return 'expanded';
  if (isTop && !anyFocus) return 'expanded';
  if (compact || count >= STACK_STAGE.collapseDepth) return 'row';
  return 'mini';
}

/** The `n of N. …` accessible name of §9.2, assembled in its fixed order. */
function accessibleName(entry: Omit<StackStageEntry, 'label'>): string {
  const parts = [`${entry.index} of ${entry.total}.`];
  if (entry.isTop) parts.push('Resolves next.');
  const kindWord = entry.kind === 'ability' ? 'Ability' : 'Spell';
  const source =
    entry.kind === 'ability'
      ? entry.sourceResolved
        ? ` from ${entry.sourceName}`
        : ', source no longer on the battlefield'
      : '';
  parts.push(`${kindWord}${source}, controlled by ${entry.isMine ? 'you' : entry.controllerName}.`);
  parts.push(entry.description);
  return parts.join(' ');
}

/**
 * The stage for one view.
 *
 * Total and pure: the same view and options always produce the same model, so a
 * reconnect that replays a `GameView` rebuilds an identical stage and nothing
 * here is load-bearing across messages (invariant I2).
 */
export function deriveStackStage(
  view: StackStageView,
  options: StackStageOptions = {},
): StackStageModel {
  const compact = options.compact ?? false;
  const count = view.stack.length;
  const empty: StackStageModel = {
    present: false,
    count: 0,
    layout: compact ? 'sheet' : 'pile',
    header: '',
    ariaLabel: '',
    entries: [],
    scrolls: false,
    hiddenCount: 0,
  };
  if (count === 0) return empty;

  const layout: StackLayout = compact
    ? 'sheet'
    : count >= STACK_STAGE.collapseDepth
      ? 'rail'
      : 'pile';

  // Wire order is bottom-first (§3.3); the stage reads top-first, so the last
  // wire element is display index 1.
  const topFirst: StackItem[] = [...view.stack].reverse();
  const anyFocus = options.focusId != null && topFirst.some((item) => item.id === options.focusId);

  // Beyond the unscrolled window the rail scrolls and states what it hides.
  const windowSize = 1 + STACK_STAGE.visibleRows;
  const scrolls = layout === 'rail' && count > windowSize;
  const hiddenCount = scrolls ? count - windowSize : 0;

  let splayStep = 0;
  const entries = topFirst.map((item, i): StackStageEntry => {
    const index = i + 1;
    const isTop = i === 0;
    const focused = options.focusId != null && item.id === options.focusId;
    const tier = tierFor({ compact, count, isTop, focused, anyFocus });
    const kind = item.source !== undefined ? 'ability' : 'spell';
    const source = item.source !== undefined ? resolveSource(view, item.source) : undefined;
    const controllerName = controllerLabel(view, item.controller);

    // Recession applies only to the pile's Mini entries; the Expanded entry sits
    // at the pile's front and the rail's rows have no recession at all (D4).
    let offsetX = 0;
    let scale = 1;
    if (layout === 'pile' && tier === 'mini') {
      splayStep += 1;
      offsetX = STACK_STAGE.splayDx * splayStep;
      scale = Math.max(STACK_STAGE.splayScaleFloor, Math.pow(STACK_STAGE.splayScale, splayStep));
    }

    let sticky: StackSticky = null;
    if (scrolls) {
      if (index <= STACK_STAGE.stickyHead) sticky = 'head';
      else if (count > STACK_STAGE.footStickyDepth && index === count) sticky = 'foot';
    }

    const base: Omit<StackStageEntry, 'label'> = {
      id: item.id,
      index,
      total: count,
      isTop,
      tier,
      kind,
      description: item.description,
      controller: item.controller,
      controllerName,
      isMine: item.controller === view.you,
      accent: identityAccent(view, item.controller),
      sourceId: item.source,
      sourceName: source?.name,
      sourceResolved: source?.resolved ?? false,
      // A letter tile for a spell (the deep-rail mock's `C`, `S`, `G`), an open
      // diamond for an ability. Shape, not hue: an ability's tile is also square
      // -cornered, so it can never be read as a card (§2.3).
      glyph: kind === 'ability' ? '◇' : (item.description.trim()[0] ?? '?').toUpperCase(),
      subtitle:
        kind === 'ability'
          ? `ability — ${source?.resolved ? source.name : 'source left play'} · ${controllerName}`
          : `spell · ${controllerName}`,
      offsetX,
      scale,
      zOrder: count - index + 1,
      sticky,
    };
    return { ...base, label: accessibleName(base) };
  });

  return {
    present: true,
    count,
    layout,
    header: `STACK (${count}) — TOP RESOLVES FIRST`,
    ariaLabel: `Stack, ${count} ${count === 1 ? 'object' : 'objects'}, top resolves first`,
    entries,
    scrolls,
    hiddenCount,
  };
}
