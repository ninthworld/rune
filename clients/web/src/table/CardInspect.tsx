/**
 * The universal card inspect surface (React DOM, ADR 0003 — text a player reads
 * is DOM, not the Pixi canvas).
 *
 * One component, one interaction, wherever a card lives: a hand card, a
 * permanent (own or opponent's), a stack object, a card inside a zone browser,
 * and the deck builder's catalog.
 *
 * **Issue #569 changed what it draws, not what it knows.** It used to be a
 * rectangular text panel that re-listed the card's fields in its own typography;
 * it now **brings the resolved card face forward** — the same {@link CardFace}
 * the hand fan, the stack rail, the zone browser, and the battlefield draw, at
 * the fixed screen-space `inspect` tier — and keeps a small **annex** for the
 * facts a card face has no printed home for: the spelled-out keyword names, the
 * permanent's dynamic state (tapped, counters), its attachments, and a stack
 * object's server-composed description.
 *
 * The face is the one already resolved by the player's art mode (ADR 0024): the
 * consumer's own `artUrlFor` lookup, exactly like every other surface. There is
 * no second art pipeline here, and nothing is fetched.
 *
 * Three distinct states, one surface (`control-language.md` §7 parity):
 *
 * | State | Backdrop | Input | Home |
 * | --- | --- | --- | --- |
 * | **pinned** (`I`, click, context menu) | dismiss veil | modal | brought forward, centre stage |
 * | **peek** (`transient`) | none | `pointer-events: none` | parked preview |
 * | **deferred** (`deferring`) | none | the panel only | parked clear of the decision |
 *
 * `deferring` is what keeps the promise of `control-language.md` §10: a decision
 * surface "must not occlude the subject of the decision or any candidate", and
 * neither may anything the player opens over it. While a decision is open the
 * stage drops its veil and parks itself away from the action column, so every
 * candidate underneath stays visible and tappable.
 *
 * Pure render of its {@link InspectTarget}: nothing here is load-bearing across
 * messages, so the same target always produces the same surface. Opening and
 * closing is ephemeral selection state owned by the shell, dropped on the next
 * view.
 */
import { useSyncExternalStore } from 'react';
import type { CardView, Counter, EntityId, StackItem } from '../protocol';
import { isAbilityStackItemKind } from '../protocol';
import { getArtVersion, subscribeArt } from '../card/art/artStore';
import { CardFace } from '../card/dom';
import type { CardDisplayData } from '../card/cardFactory';
import { SymbolText, symbolNotationText } from '../chrome/symbols';
import { cx } from '../chrome/cx';
import { domCardArt } from './planeDisplayData';
import { toDisplayData } from './scene/card-helpers';
import s from './inspect.module.css';

/** A named reference to another permanent, for the inspector's attachment lines. */
export interface AttachmentRef {
  /** Entity id of the referenced permanent. */
  id: EntityId;
  /** Its display name, taken straight from the view. */
  name: string;
}

/**
 * What the surface is inspecting. A `card` target is any {@link CardView} (hand,
 * graveyard/exile pile, or a permanent's current face, in which case the
 * permanent's dynamic state rides alongside); a `stack` target is a
 * {@link StackItem}, which always carries a server-composed `description` and,
 * since issue #550, usually the {@link CardView} face to draw for it.
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
  /** The resolved target to inspect; the surface is rendered only when non-null. */
  target: InspectTarget;
  /** Close the surface (veil click or the explicit close control). */
  onClose: () => void;
  /**
   * Render as a **transient peek** (issue #321): a non-blocking preview in one
   * consistent home rather than the pinned, brought-forward card. A peek has no
   * veil, no close control, and never captures pointer input, so a hover-dwell /
   * long-press / just-selected preview coexists with the interaction the player
   * is already making. Omitted/`false` renders the pinned surface.
   */
  transient?: boolean;
  /**
   * Whether a decision is currently open (issue #569). The surface then drops
   * its dismiss veil and parks clear of the action column, so the decision and
   * its candidates are never covered by something the player opened over them.
   */
  deferring?: boolean;
  /**
   * Open the card-art settings (ADR 0024). Rendered as an entry point on the
   * pinned surface only — inspect is where a player is looking at art closely
   * enough to want to change it (`card-representation.md` §8.2's "art-source
   * controls" row). Absent on surfaces that have no art settings to open.
   */
  onOpenArtSettings?: () => void;
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

/** The `CardView` a target draws a face from, or `undefined` when it has none
 * (an ability whose source has already left the battlefield, CR 608.2). */
function faceCard(target: InspectTarget): CardView | undefined {
  return target.kind === 'card' ? target.card : target.item.card;
}

/** The surface's accessible name: the card's name, or a stack object's text. */
function targetName(target: InspectTarget): string {
  if (target.kind === 'card') return target.card.name;
  return target.item.card?.name ?? target.item.description;
}

/**
 * The face's display data. This is the SAME mapping every other card surface
 * uses ({@link toDisplayData}); the permanent's live state rides along so an
 * inspected permanent draws tapped and countered exactly as it does on the
 * board. Nothing is derived and nothing is enriched — the client has no card
 * database (ADR 0018 §9).
 */
function inspectDisplayData(target: InspectTarget, card: CardView): CardDisplayData {
  return toDisplayData(card, {
    tapped: target.kind === 'card' ? target.tapped : undefined,
    counters: target.kind === 'card' ? target.counters : undefined,
    selected: false,
    actionable: false,
  });
}

export function CardInspect({
  target,
  onClose,
  transient = false,
  deferring = false,
  onOpenArtSettings,
}: Props) {
  // Re-subscribed so a background download landing mid-inspect paints into the
  // face that is already on screen (ADR 0024). Pure presentation cache.
  useSyncExternalStore(subscribeArt, getArtVersion);
  const name = targetName(target);
  // A stack object's text can carry symbol notation; the drawn text renders it
  // as symbols, and the accessible name — a pure text context — takes the
  // spoken substitution instead (issue #462).
  const spoken = symbolNotationText(name);
  const card = faceCard(target);

  const face = card ? (
    <CardFace
      data={inspectDisplayData(target, card)}
      tier="inspect"
      elevation="held"
      art={domCardArt(card)}
      rulesText={card.rules_text}
      className={s.face}
    />
  ) : (
    // No face to draw. The stack object still has a server-composed description,
    // which the annex always shows — so the surface degrades to the text it has
    // rather than to nothing.
    <div
      className={s.facelessPlate}
      data-testid="card-inspect-faceless"
      role="img"
      aria-label={spoken}
    >
      <SymbolText text={name} />
    </div>
  );

  const body = (
    <>
      {face}
      <div className={s.annex} data-testid="card-inspect-annex">
        {target.kind === 'card' ? (
          <CardAnnex
            card={target.card}
            tapped={target.tapped}
            counters={target.counters}
            attachedTo={target.attachedTo}
            attachments={target.attachments}
          />
        ) : (
          <StackAnnex item={target.item} />
        )}
        {/* The art entry point: pinned only, because a peek never takes input. */}
        {!transient && onOpenArtSettings && (
          <button
            type="button"
            className={s.annexAction}
            data-testid="card-inspect-art-settings"
            onClick={onOpenArtSettings}
          >
            Card art…
          </button>
        )}
      </div>
    </>
  );

  // Transient peek (issue #321): the preview's one consistent home. No veil, no
  // close control, `pointer-events: none` — it can never block the click, drag,
  // or pick the player is mid-way through.
  if (transient) {
    return (
      <div
        data-testid="card-inspect"
        data-transient="true"
        data-inspect="peek"
        className={cx(s.stage, s.peek)}
        role="img"
        aria-label={`Preview ${spoken}`}
      >
        {body}
      </div>
    );
  }

  // Pinned. The veil is what a click outside dismisses on — and it is dropped
  // entirely while a decision is open, so the candidates under it stay live.
  return (
    <div
      data-testid="card-inspect-backdrop"
      data-deferring={deferring || undefined}
      className={cx(s.veil, deferring && s.veilClear)}
      onClick={deferring ? undefined : onClose}
      role="presentation"
    >
      {/* Stop propagation so a click on the card itself does not dismiss it. */}
      <div
        data-testid="card-inspect"
        data-inspect={deferring ? 'deferred' : 'pinned'}
        className={cx(s.stage, s.pinned, deferring && s.deferred)}
        role="dialog"
        aria-modal={deferring ? undefined : 'true'}
        aria-label={`Inspect ${spoken}`}
        onClick={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          data-testid="card-inspect-close"
          aria-label="Close inspect"
          onClick={onClose}
          className={s.close}
        >
          ×
        </button>
        {body}
      </div>
    </div>
  );
}

/**
 * The annex for a {@link CardView} target: everything the drawn face cannot
 * carry as printed matter — keyword names spelled out (the face draws capped
 * glyph plates, and a keyword with no glyph has none), the permanent's dynamic
 * state, and its attachment relationships. Every value is the server's.
 */
function CardAnnex({
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
  const attachmentList = attachments ?? [];
  return (
    <>
      {keywords.length > 0 && (
        <div className={s.keywords} data-testid="card-inspect-keywords">
          {keywords.map((keyword) => (
            <span key={keyword} className={s.keyword}>
              {formatKeyword(keyword)}
            </span>
          ))}
        </div>
      )}
      {(tapped || (counters && counters.length > 0)) && (
        <div className={s.stateRow} data-testid="card-inspect-state">
          {tapped && <span className={s.state}>Tapped</span>}
          {(counters ?? []).map((counter) => (
            <span key={counter.kind} className={s.state}>
              {counter.count}× {counter.kind}
            </span>
          ))}
        </div>
      )}
      {/* Attachment relationship (issue #333), shown from either side: an attached
          permanent names its host; a host lists what is attached to it. Straight
          from the view — the client derives no rules from the reference. */}
      {(attachedTo || attachmentList.length > 0) && (
        <div className={s.stateRow} data-testid="card-inspect-attachments">
          {attachedTo && <span className={s.state}>Attached to {attachedTo.name}</span>}
          {attachmentList.map((ref) => (
            <span key={ref.id} className={s.state}>
              Enchanted by {ref.name}
            </span>
          ))}
        </div>
      )}
    </>
  );
}

/**
 * The annex for a stack object: what it is (server-stated `kind`, never inferred
 * from `source`) and the authoritative composed description, which is the one
 * text the protocol guarantees for every stack entry.
 */
function StackAnnex({ item }: { item: StackItem }) {
  return (
    <>
      <div className={s.stackKind} data-testid="card-inspect-stack-kind">
        {item.kind === 'triggered'
          ? 'Triggered ability on the stack'
          : item.kind === 'activated'
            ? 'Activated ability on the stack'
            : isAbilityStackItemKind(item.kind)
              ? 'Ability on the stack'
              : item.kind === 'spell'
                ? 'Spell on the stack'
                : 'On the stack'}
      </div>
      <p className={s.description} data-testid="card-inspect-description">
        <SymbolText text={item.description} />
      </p>
      <div className={s.stateRow} data-testid="card-inspect-state">
        <span className={s.state}>Controller {item.controller}</span>
      </div>
    </>
  );
}
