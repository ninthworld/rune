/**
 * The **contextual stack stage** — the surface that replaces ADR 0023's permanent
 * right rail (`docs/design/stack-and-relationships.md` §1–§3, §9, §10.4; issue
 * #534 under [ADR 0032](../../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * ## The contract with integration
 *
 * Mount it as a **direct child of the match shell root** (`live-match.module.css`'s
 * `.shell`, which is already `position: relative`) — a *sibling* of `.top`,
 * `.scene`, and `.bottom`, never inside one. A shell region carrying a z-index
 * creates a stacking context nothing inside it can climb out of, which is the
 * defect ADR 0032 §4 was written against; and a region would also give the stage
 * a track to reserve, which is exactly what "the empty stack consumes no
 * permanent battlefield width" forbids.
 *
 * Everything else is props:
 *
 * ```tsx
 * <StackStage
 *   view={view}                                  // the latest GameView
 *   compact={compact}                            // isCompactShell(viewport)
 *   targeting={targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined}
 *   onInspect={setInspectedId}
 * />
 * ```
 *
 * `targeting` and `onInspect` are the two the shipped `StackPanel` already took,
 * with the same shapes and the same `target-<id>` / `inspect-<id>` test ids, so
 * wiring them is a rename rather than a new integration.
 *
 * ## What it renders, and what it refuses to
 *
 * **Nothing at all when `view.stack` is empty.** Not a quiet state, not a
 * placeholder, not a zero-height box — `null`. The shipped `StackPanel` already
 * did this and `Rail.tsx` overrode it with a designed empty panel; retiring the
 * rail restores it.
 *
 * It renders **no button that answers the game**. RESOLVE and RESPOND sit in the
 * control cluster at the stage's foot, not inside it (§1.4, D17), and the stage
 * has no idea what a pass does — deciding that a pass resolves the top of the
 * stack is a rules judgment the client may not make (`control-language.md`
 * GAP-2). The only controls here are the per-entry inspect handle, the compact
 * sheet's handle, and — in targeting mode — the entries the **server** listed as
 * candidates.
 *
 * ## Layering
 *
 * The stage lives at `--rune-z-shell`, below `--rune-z-decision`. ADR 0032's
 * binding rule is that a layer may only be covered by a layer the player invoked
 * and can dismiss without answering it; a stack stage is not a decision, so it
 * must never be in a position to cover one.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent } from 'react';
import type { EntityId, GameView } from '../../protocol';
import { cx } from '../../chrome/cx';
import {
  deriveStackStage,
  stackStyleVars,
  type StackStageEntry,
  type StackStageModel,
} from './stackStage';
import s from './stack.module.css';

/** The active target slot's stack-object candidates plus the pick handler. */
export interface StackTargeting {
  /** Entity ids the **server** listed as legal for the active slot. */
  candidates: EntityId[];
  /** Pick a stack object as the current slot's answer. */
  onPick: (id: EntityId) => void;
}

export interface StackStageProps {
  /** The latest view; the stage renders exactly its `stack`. */
  view: GameView;
  /**
   * Compact geometry — pass `isCompactShell(viewport)`. The stage changes kind
   * to the bottom sheet of §10.4 rather than shrinking the flank column.
   */
  compact?: boolean;
  /** Present only in targeting mode; makes candidate stack objects pickable. */
  targeting?: StackTargeting;
  /** Open the inspect surface for a stack object (issue #261, carried). */
  onInspect?: (id: EntityId) => void;
}

/** The per-slot custom properties the stylesheet positions and paints from. */
function slotVars(entry: StackStageEntry): CSSProperties {
  return {
    '--slot-accent': entry.accent,
    '--slot-dx': `${entry.offsetX}px`,
    '--slot-scale': `${entry.scale}`,
    '--slot-z': `${entry.zOrder}`,
  } as CSSProperties;
}

/** One entry's drawn contents, identical at every tier so nothing degrades away. */
function EntryBody({ entry }: { entry: StackStageEntry }) {
  return (
    <>
      <span className={s.tile} data-kind={entry.kind} aria-hidden="true">
        {entry.glyph}
      </span>
      <span className={s.text}>
        <span className={s.badges}>
          {/* The order index, and the `N` for the tiers that print only `n`. */}
          <span className={s.index} data-testid={`stack-index-${entry.id}`}>
            {entry.tier === 'expanded' ? `${entry.index}/${entry.total}` : entry.index}
          </span>
          {entry.isTop && (
            <span className={s.topBadge} data-testid={`stack-top-${entry.id}`}>
              Resolves next
            </span>
          )}
        </span>
        <span className={s.title}>{entry.description}</span>
        <span className={s.subtitle} data-testid={`stack-subtitle-${entry.id}`}>
          {entry.subtitle}
        </span>
      </span>
    </>
  );
}

export function StackStage({ view, compact = false, targeting, onInspect }: StackStageProps) {
  const [focusId, setFocusId] = useState<EntityId | null>(null);
  // `null` means "follow the automatic rule"; a boolean is the player's own
  // choice for this stack. Ephemeral presentation — a fresh mount re-derives it.
  const [sheetOverride, setSheetOverride] = useState<boolean | null>(null);
  const bodies = useRef(new Map<EntityId, HTMLElement>());

  const depth = view.stack.length;
  const model: StackStageModel = deriveStackStage(view, { compact, focusId });

  // A focused entry that resolved away must not keep the Expanded tier assigned
  // to a ghost, and a stack that emptied returns the sheet to the automatic rule.
  useEffect(() => {
    if (focusId !== null && !view.stack.some((item) => item.id === focusId)) setFocusId(null);
    if (depth === 0) setSheetOverride(null);
  }, [depth, focusId, view.stack]);

  /** Move the roving focus to an entry and put the DOM focus with it (§9.3). */
  const focusAt = useCallback(
    (index: number, entries: StackStageEntry[]) => {
      const next = entries[Math.max(0, Math.min(entries.length - 1, index))];
      if (next === undefined) return;
      setFocusId(next.id);
      bodies.current.get(next.id)?.focus();
    },
    [setFocusId],
  );

  if (!model.present) return null;

  const candidates = targeting ? new Set(targeting.candidates) : null;
  const focusIndex = Math.max(
    0,
    model.entries.findIndex((entry) => entry.id === focusId),
  );

  const onKeyDown = (event: ReactKeyboardEvent<HTMLOListElement>): void => {
    switch (event.key) {
      case 'ArrowDown':
        focusAt(focusIndex + 1, model.entries);
        break;
      case 'ArrowUp':
        focusAt(focusIndex - 1, model.entries);
        break;
      case 'Home':
        focusAt(0, model.entries);
        break;
      case 'End':
        focusAt(model.entries.length - 1, model.entries);
        break;
      default:
        return;
    }
    // Only reached for a handled key, so the page never loses its own scrolling.
    event.preventDefault();
  };

  const registerBody = (id: EntityId) => (node: HTMLElement | null) => {
    if (node === null) bodies.current.delete(id);
    else bodies.current.set(id, node);
  };

  const renderEntry = (entry: StackStageEntry, rovingIndex: number) => {
    const isCandidate = candidates?.has(entry.id) ?? false;
    // Roving tabindex: the stage is ONE tab stop (§9.3), and the arrows walk it.
    const tabIndex = rovingIndex === focusIndex ? 0 : -1;
    const body = isCandidate ? (
      <button
        type="button"
        ref={registerBody(entry.id)}
        className={s.body}
        data-testid={`target-${entry.id}`}
        aria-label={`Target ${entry.description}`}
        tabIndex={tabIndex}
        onFocus={() => setFocusId(entry.id)}
        onClick={() => targeting?.onPick(entry.id)}
      >
        <EntryBody entry={entry} />
      </button>
    ) : (
      <div
        ref={registerBody(entry.id)}
        className={s.body}
        data-testid={`stack-entry-${entry.id}`}
        aria-label={entry.label}
        tabIndex={tabIndex}
        onFocus={() => setFocusId(entry.id)}
      >
        <EntryBody entry={entry} />
      </div>
    );

    return (
      <li
        key={entry.id}
        className={cx(
          s.slot,
          entry.sticky === 'head' && s.sticky,
          entry.sticky === 'foot' && s.stickyFoot,
        )}
        style={slotVars(entry)}
        data-tier={entry.tier}
        data-kind={entry.kind}
        data-top={entry.isTop || undefined}
        data-candidate={isCandidate || undefined}
      >
        {body}
        {/* The inspect handle is a SIBLING of the body, never nested inside a
            candidate's <button> (invalid HTML), so a stack object stays
            inspectable whether or not it is a legal target. Carried verbatim. */}
        {onInspect && (
          <button
            type="button"
            className={s.inspect}
            data-testid={`inspect-${entry.id}`}
            aria-label={`Inspect ${entry.description}`}
            tabIndex={-1}
            onClick={() => onInspect(entry.id)}
          >
            i
          </button>
        )}
      </li>
    );
  };

  const list = (
    <ol
      className={s.list}
      data-testid="stack-stage-list"
      data-layout={model.layout}
      data-scrolls={model.scrolls || undefined}
      onKeyDown={onKeyDown}
    >
      {model.entries.map(renderEntry)}
    </ol>
  );

  if (model.layout === 'sheet') {
    // §10.4: the sheet opens on its own when the stack becomes non-empty AND the
    // receiver holds priority — a membership test on two view fields, never a
    // judgement about whether the player *should* respond. Otherwise it is a
    // handle stating the depth and the top entry, one tap from the full sheet.
    const auto = view.priority_player === view.you;
    const open = sheetOverride ?? auto;
    const top = model.entries[0];
    if (!open) {
      return (
        <button
          type="button"
          className={s.handle}
          style={stackStyleVars()}
          data-testid="stack-sheet-handle"
          aria-expanded={false}
          aria-label={`${model.ariaLabel}. Open the stack.`}
          onClick={() => setSheetOverride(true)}
        >
          <span>{`STACK (${model.count})`}</span>
          <span className={s.handleTitle}>{top.description}</span>
        </button>
      );
    }
    return (
      <section
        className={s.sheet}
        style={stackStyleVars()}
        data-testid="stack-stage"
        data-layout="sheet"
        aria-label={model.ariaLabel}
      >
        <div className={s.historyHead}>
          <h2 className={s.header}>{model.header}</h2>
          <button
            type="button"
            className={s.inspect}
            data-testid="stack-sheet-dismiss"
            aria-expanded
            aria-label="Collapse the stack"
            onClick={() => setSheetOverride(false)}
          >
            ×
          </button>
        </div>
        <div className={s.sheetBody}>{list}</div>
      </section>
    );
  }

  return (
    <section
      className={s.stage}
      style={stackStyleVars()}
      data-testid="stack-stage"
      data-layout={model.layout}
      aria-label={model.ariaLabel}
    >
      <h2 className={s.header}>{model.header}</h2>
      {list}
      {model.hiddenCount > 0 && (
        <p className={s.more} data-testid="stack-more">
          {`+${model.hiddenCount} more`}
        </p>
      )}
    </section>
  );
}
