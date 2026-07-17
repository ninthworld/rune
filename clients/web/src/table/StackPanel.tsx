/**
 * The stack panel (React DOM, ADR 0003 — the stack is text a user reads, so it is
 * DOM chrome, not the Pixi canvas).
 *
 * Renders {@link GameView.stack} so a game stays followable while objects resolve:
 * every {@link StackItem} in stack order, with the top of the stack (the object
 * that resolves next) clearly distinguished, its controller, whether it is a spell
 * or an ability, and — for an ability — the source permanent it is tied to. The
 * server bakes any chosen targets into each entry's `description`, so the display
 * text already carries them (the client derives none). `GameView.stack` is
 * "bottom first" on the wire; the panel shows it top-first so the resolving object
 * reads at the top.
 *
 * Pure render of the latest view: nothing here is load-bearing across messages, so
 * a reconnect that replays the same GameView reproduces the identical panel. No
 * game logic is computed (AGENTS.md hard rule) — the panel only formats the fields
 * the server sent.
 *
 * In **targeting mode** a stack object can itself be a legal target (e.g. a
 * counterspell), exactly as a permanent or a player can (ADR 0009 §Client). Each
 * entry whose id is a server-listed candidate becomes pickable; every other entry
 * is inert. The client only makes the server's candidates pickable — it derives no
 * legality.
 */
import type { EntityId, GameView, StackItem } from '../protocol';
import {
  inspectRowHandle,
  stackBadges,
  stackItem,
  stackItemButtonReset,
  stackItemMeta,
  stackItemName,
  stackItemTop,
  stackKindBadge,
  stackItemRow,
  stackList,
  stackPanel,
  stackTargetItem,
  stackTitle,
  stackTopBadge,
} from './styles';

/** The active target slot's stack-object candidates plus the pick handler. */
interface TargetingStack {
  /** Entity ids that are legal targets for the active slot (stack objects included). */
  candidates: EntityId[];
  /** Pick a stack object as the current slot's answer. */
  onPick: (id: EntityId) => void;
}

interface Props {
  /** The latest view; the panel renders exactly its `stack`. */
  view: GameView;
  /** Present only in targeting mode; makes candidate stack objects pickable. */
  targeting?: TargetingStack;
  /** Open the inspect popover for a stack object (issue #261). */
  onInspect?: (id: EntityId) => void;
}

/**
 * Resolve an ability's `source` permanent id to a display name from the current
 * battlefield, falling back to the raw id if it is not a visible permanent (the
 * source could be hidden or already gone). Presentation only — no lookup drives
 * legality.
 */
function sourceName(view: GameView, source: EntityId): string {
  return view.battlefield.find((perm) => perm.id === source)?.card.name ?? source;
}

export function StackPanel({ view, targeting, onInspect }: Props) {
  // Empty stack renders no chrome at all (acceptance: unobtrusive when nothing is
  // on the stack). There is nothing to reconstruct, so the panel simply vanishes.
  if (view.stack.length === 0) return null;

  const candidateSet = targeting ? new Set(targeting.candidates) : null;
  // Wire order is bottom-first; show top-first so the object resolving next is at
  // the top of the panel. `topIndex` marks that entry for the "resolves next" badge.
  const topIndex = view.stack.length - 1;
  const ordered = view.stack.map((item, i) => ({ item, isTop: i === topIndex }));
  ordered.reverse();

  const renderEntry = (item: StackItem, isTop: boolean) => {
    const isAbility = item.source !== undefined;
    return (
      <>
        <div style={stackBadges}>
          <span style={stackKindBadge}>{isAbility ? 'Ability' : 'Spell'}</span>
          {isTop && (
            <span style={stackTopBadge} data-testid={`stack-top-${item.id}`}>
              Resolves next
            </span>
          )}
        </div>
        <div style={stackItemName}>{item.description}</div>
        <div style={stackItemMeta}>Controller {item.controller}</div>
        {isAbility && item.source !== undefined && (
          <div style={stackItemMeta} data-testid={`stack-source-${item.id}`}>
            Source: {sourceName(view, item.source)}
          </div>
        )}
      </>
    );
  };

  // The inspect handle sits as a sibling of the entry inside its `<li>` (never
  // nested inside a candidate's target `<button>`, which would be invalid HTML), so
  // a stack object is inspectable whether or not it is a legal target.
  const inspectButton = (item: StackItem) =>
    onInspect && (
      <button
        type="button"
        data-testid={`inspect-${item.id}`}
        aria-label={`Inspect ${item.description}`}
        onClick={() => onInspect(item.id)}
        style={inspectRowHandle}
      >
        i
      </button>
    );

  return (
    <section data-testid="stack-panel" style={stackPanel} aria-label="Stack">
      <h2 style={stackTitle}>Stack ({view.stack.length})</h2>
      <ol style={stackList}>
        {ordered.map(({ item, isTop }) => {
          const style = isTop ? { ...stackItem, ...stackItemTop } : stackItem;
          const isCandidate = candidateSet?.has(item.id) ?? false;
          if (isCandidate && targeting) {
            return (
              <li key={item.id} style={stackItemRow}>
                <button
                  type="button"
                  data-testid={`target-${item.id}`}
                  aria-label={`Target ${item.description}`}
                  onClick={() => targeting.onPick(item.id)}
                  style={{ ...stackItemButtonReset, ...style, ...stackTargetItem }}
                >
                  {renderEntry(item, isTop)}
                </button>
                {inspectButton(item)}
              </li>
            );
          }
          return (
            <li key={item.id} style={stackItemRow}>
              <div data-testid={`stack-item-${item.id}`} style={{ ...style, flex: 1 }}>
                {renderEntry(item, isTop)}
              </div>
              {inspectButton(item)}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
