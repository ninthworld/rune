/**
 * The universal card inspect popover (React DOM, ADR 0003 — text a user reads is
 * DOM, not the Pixi canvas).
 *
 * One component, one interaction pattern for reading what any card does, wherever
 * it lives: a hand card, a permanent (own or opponent's), a stack object, and —
 * once they exist — a card inside a zone browser. It renders **only** what the
 * server already put in the view: name, mana cost, type line, rules text,
 * keywords, current (effective) P/T, and a permanent's dynamic state (tapped,
 * counters). The client derives nothing — there is no card database and no rules
 * inference here (issue #261; ADR 0018 §9 defers any client-local enrichment).
 *
 * Pure render of its {@link InspectTarget}: nothing here is load-bearing across
 * messages, so the same target always produces the same panel. Opening/closing is
 * ephemeral selection state owned by {@link Table}, discarded on the next view.
 */
import type { CardView, Counter, StackItem } from '../protocol';
import s from './chrome.module.css';

/**
 * What the popover is inspecting. A `card` target is any {@link CardView} (hand,
 * graveyard/exile pile, or a permanent's current face, in which case the
 * permanent's dynamic state rides alongside); a `stack` target is a
 * {@link StackItem}, which carries only the server-composed `description` (no
 * CardView exists for a stack object in the current protocol).
 */
export type InspectTarget =
  | { kind: 'card'; card: CardView; tapped?: boolean; counters?: Counter[] }
  | { kind: 'stack'; item: StackItem };

interface Props {
  /** The resolved target to inspect; the popover is rendered only when non-null. */
  target: InspectTarget;
  /** Close the popover (backdrop click or the explicit close control). */
  onClose: () => void;
}

/**
 * Display-format a lowercase wire keyword, e.g. `first_strike` → `First Strike`.
 * Presentation only — the server owns which keywords a card has.
 */
function formatKeyword(keyword: string): string {
  return keyword
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

/** The name shown at the top of the panel for either target kind. */
function targetName(target: InspectTarget): string {
  return target.kind === 'card' ? target.card.name : target.item.description;
}

export function CardInspect({ target, onClose }: Props) {
  const name = targetName(target);
  return (
    <div
      data-testid="card-inspect-backdrop"
      className={s.inspectBackdrop}
      onClick={onClose}
      role="presentation"
    >
      {/* Stop propagation so a click inside the panel does not dismiss it. */}
      <div
        data-testid="card-inspect"
        className={s.inspectPanel}
        role="dialog"
        aria-modal="true"
        aria-label={`Inspect ${name}`}
        onClick={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          data-testid="card-inspect-close"
          aria-label="Close inspect"
          onClick={onClose}
          className={s.inspectClose}
        >
          ×
        </button>
        <h2 className={s.inspectName} data-testid="card-inspect-name">
          {name}
        </h2>
        {target.kind === 'card' ? (
          <CardBody card={target.card} tapped={target.tapped} counters={target.counters} />
        ) : (
          <StackBody item={target.item} />
        )}
      </div>
    </div>
  );
}

/** The body for a {@link CardView} target: cost, type, P/T, keywords, rules, state. */
function CardBody({
  card,
  tapped,
  counters,
}: {
  card: CardView;
  tapped?: boolean;
  counters?: Counter[];
}) {
  const keywords = card.keywords ?? [];
  const rules = card.rules_text ?? '';
  const hasPt = card.power !== undefined && card.toughness !== undefined;
  return (
    <>
      {card.mana_cost !== undefined && (
        <div className={s.inspectCost} data-testid="card-inspect-cost">
          {card.mana_cost}
        </div>
      )}
      <div className={s.inspectTypeLine} data-testid="card-inspect-type">
        {card.type_line}
      </div>
      {hasPt && (
        <div className={s.inspectPt} data-testid="card-inspect-pt">
          {card.power}/{card.toughness}
        </div>
      )}
      {keywords.length > 0 && (
        <div className={s.inspectKeywords} data-testid="card-inspect-keywords">
          {keywords.map((keyword) => (
            <span key={keyword} className={s.inspectKeyword}>
              {formatKeyword(keyword)}
            </span>
          ))}
        </div>
      )}
      {rules ? (
        <p className={s.inspectRules} data-testid="card-inspect-rules">
          {rules}
        </p>
      ) : (
        <p className={s.inspectNoText} data-testid="card-inspect-rules">
          No rules text.
        </p>
      )}
      {(tapped || (counters && counters.length > 0)) && (
        <div className={s.inspectStateRow} data-testid="card-inspect-state">
          {tapped && <span className={s.inspectState}>Tapped</span>}
          {(counters ?? []).map((counter) => (
            <span key={counter.kind} className={s.inspectState}>
              {counter.count}× {counter.kind}
            </span>
          ))}
        </div>
      )}
    </>
  );
}

/** The body for a stack object: the server-composed description and controller. */
function StackBody({ item }: { item: StackItem }) {
  return (
    <>
      <div className={s.inspectTypeLine} data-testid="card-inspect-type">
        {item.source !== undefined ? 'Ability on the stack' : 'Spell on the stack'}
      </div>
      <p className={s.inspectRules} data-testid="card-inspect-rules">
        {item.description}
      </p>
      <div className={s.inspectStateRow} data-testid="card-inspect-state">
        <span className={s.inspectState}>Controller {item.controller}</span>
      </div>
    </>
  );
}
