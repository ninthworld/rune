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
import { useSyncExternalStore } from 'react';
import type { CardView, Counter, EntityId, StackItem } from '../protocol';
import { artUrlFor, getArtVersion, subscribeArt } from '../card/art/artStore';
import s from './chrome.module.css';

/** A named reference to another permanent, for the inspector's attachment lines. */
export interface AttachmentRef {
  /** Entity id of the referenced permanent. */
  id: EntityId;
  /** Its display name, taken straight from the view. */
  name: string;
}

/**
 * What the popover is inspecting. A `card` target is any {@link CardView} (hand,
 * graveyard/exile pile, or a permanent's current face, in which case the
 * permanent's dynamic state rides alongside); a `stack` target is a
 * {@link StackItem}, which carries only the server-composed `description` (no
 * CardView exists for a stack object in the current protocol).
 */
export type InspectTarget =
  | {
      kind: 'card';
      card: CardView;
      tapped?: boolean;
      counters?: Counter[];
      /**
       * The host this permanent is attached to (issue #333), if any — an Aura names
       * the object it enchants. Resolved from the view's `attached_to`; absent when
       * unattached or the host is not in the visible battlefield.
       */
      attachedTo?: AttachmentRef;
      /**
       * The permanents attached to this one (issue #333) — the host side of the
       * relationship, so inspecting an enchanted creature lists its Auras. Absent
       * when nothing is attached.
       */
      attachments?: AttachmentRef[];
    }
  | { kind: 'stack'; item: StackItem };

interface Props {
  /** The resolved target to inspect; the popover is rendered only when non-null. */
  target: InspectTarget;
  /** Close the popover (backdrop click or the explicit close control). */
  onClose: () => void;
  /**
   * Render as a **transient peek** (issue #321): a non-blocking preview in one
   * consistent home rather than the pinned modal. A peek has no backdrop, no close
   * control, and never captures pointer input, so a hover-dwell / long-press / just-
   * selected preview coexists with the interaction the player is already making.
   * Omitted/`false` renders the pinned, dismissible panel.
   */
  transient?: boolean;
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

export function CardInspect({ target, onClose, transient = false }: Props) {
  const name = targetName(target);
  const body =
    target.kind === 'card' ? (
      <CardBody
        card={target.card}
        tapped={target.tapped}
        counters={target.counters}
        attachedTo={target.attachedTo}
        attachments={target.attachments}
      />
    ) : (
      <StackBody item={target.item} />
    );

  // Transient peek (issue #321): a non-blocking preview parked in a consistent home.
  // No backdrop, no close control, `pointer-events: none` — it never steals input
  // from the interaction the player is already making, and its entrance honors
  // `prefers-reduced-motion` (handled in the stylesheet).
  if (transient) {
    return (
      <div
        data-testid="card-inspect"
        data-transient="true"
        className={s.inspectPreview}
        role="img"
        aria-label={`Preview ${name}`}
      >
        <h2 className={s.inspectName} data-testid="card-inspect-name">
          {name}
        </h2>
        {body}
      </div>
    );
  }

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
        {body}
      </div>
    </div>
  );
}

/** The body for a {@link CardView} target: cost, type, P/T, keywords, rules, state. */
function CardBody({
  card,
  tapped,
  counters,
  attachedTo,
  attachments,
}: {
  card: CardView;
  tapped?: boolean;
  counters?: Counter[];
  attachedTo?: AttachmentRef;
  attachments?: AttachmentRef[];
}) {
  const keywords = card.keywords ?? [];
  const rules = card.rules_text ?? '';
  const hasPt = card.power !== undefined && card.toughness !== undefined;
  const attachmentList = attachments ?? [];
  // The card's illustration under the player's chosen art source (ADR 0024), if
  // one is loaded — resubscribed so a background download appearing mid-inspect
  // shows up. Pure presentation cache; absent renders the text-only panel.
  useSyncExternalStore(subscribeArt, getArtVersion);
  const artUrl = artUrlFor(card.functional_id);
  return (
    <>
      {artUrl !== undefined && (
        <img
          className={s.inspectArt}
          data-testid="card-inspect-art"
          src={artUrl}
          alt=""
          aria-hidden="true"
        />
      )}
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
      {/* Attachment relationship (issue #333), shown from either side: an attached
          permanent names its host; a host lists what is attached to it. Straight
          from the view — the client derives no rules from the reference. */}
      {(attachedTo || attachmentList.length > 0) && (
        <div className={s.inspectStateRow} data-testid="card-inspect-attachments">
          {attachedTo && <span className={s.inspectState}>Attached to {attachedTo.name}</span>}
          {attachmentList.map((ref) => (
            <span key={ref.id} className={s.inspectState}>
              Enchanted by {ref.name}
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
