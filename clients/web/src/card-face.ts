/**
 * One presentation model for every card-shaped object the server projects.
 *
 * The view carries four different shapes that a player reads as "a card": a `CardView` in a
 * hand or a pile, a `Permanent` on the battlefield, a `StackItem` waiting to resolve, and an
 * `Emblem`, which is a card-shaped thing with no card at all. Rendering each of them where it
 * happens is how a hand, a battlefield, and a stack end up disagreeing about what a counter
 * looks like. They are reduced here instead, to one `CardFace`, and the components downstream
 * render that and nothing else.
 *
 * This module invents no information. Every field is either something the server stated or an
 * absence, and an absence stays absent — a face with no rules text renders without rules text
 * rather than with a placeholder, because the client has no way to tell "this card has no
 * abilities" from "the server did not send them" and must not imply either.
 *
 * Two derivations here are load-bearing rather than cosmetic:
 *
 * - **Printed loyalty is not current loyalty.** `CardView.loyalty` is the number in a
 *   planeswalker's corner (CR 306.5b) — what it enters the battlefield with. What it *has* is
 *   the `loyalty` entry in `Permanent.counters`. On the battlefield only the counter is the
 *   answer, so a planeswalker that has spent down to 1 never reads as its printed 5.
 * - **A token has no card identity.** `functional_id` is always empty for one (CR 111), so the
 *   art key of ADR 0012 is withheld rather than passed along as an empty string that a cache
 *   would treat as a card it failed to resolve.
 */
import type { CardView, Counter, Emblem, Permanent, StackItem } from './protocol'
import { list, powerToughness } from './normalize'

/**
 * The surface a face is being drawn on.
 *
 * A variant is a presentation budget, not a different card: the same model backs all five, and
 * each one decides how much of it there is room to show. `inspect` is the only variant that
 * shows everything, which is why every other one must be able to open it.
 */
export type CardFaceVariant = 'inspect' | 'hand' | 'battlefield' | 'stack' | 'compact'

/**
 * How a face is participating in whatever interaction is in progress.
 *
 * These are presentation states the caller assigns, never conclusions this module draws. The
 * client does not know what is legal — `candidate` means the server *named* this object, in an
 * action's `subject` or a requirement's `candidates`, and `disabled` means the caller chose not
 * to offer it here, not that a rule forbids it.
 */
export type CardFaceState = 'idle' | 'selected' | 'candidate' | 'pending' | 'disabled'

/** The number a face leads with, and what that number means. */
export interface CardFaceStat {
  kind: 'power_toughness' | 'loyalty'
  value: string
  /** Read aloud to a screen reader, where `4` on its own says nothing. */
  label: string
}

/** Everything a component may draw for one card-shaped object. */
export interface CardFace {
  /** The entity id the server addresses this object by, and the inspector keys on. */
  id: string
  name: string
  manaCost?: string
  typeLine?: string
  rulesText?: string
  keywords: readonly string[]
  stat?: CardFaceStat
  /** Counters other than the one already shown as `stat`. */
  counters: readonly Counter[]
  /** Damage marked on a permanent this turn; absent when none is marked. */
  damage?: number
  /** Server-projected state a player must be able to see: token, commander, stack kind. */
  markers: readonly string[]
  tapped: boolean
  /** `functional_id` — the ADR 0012 art key. Absent for anything with no card identity. */
  artKey?: string
}

/** Display wording for the stack kinds the server states (`docs/protocol.md`). */
const STACK_KIND_LABELS: Record<string, string> = {
  spell: 'Spell',
  ability: 'Ability',
  activated: 'Activated ability',
  triggered: 'Triggered ability',
}

/** A face for a card in a hand, a pile, or a reveal — anywhere that is not the battlefield. */
export function cardFace(card: CardView): CardFace {
  const pt = powerToughness(card)
  return {
    id: card.id,
    name: card.name,
    manaCost: card.mana_cost,
    typeLine: card.type_line,
    rulesText: card.rules_text,
    keywords: list(card.keywords),
    // Off the battlefield there is no loyalty counter to prefer, so the printed number is the
    // only one there is — and it is the right one, because that is what the card would enter
    // with. Labelled as starting loyalty so it is not misread as a current total.
    stat: pt
      ? { kind: 'power_toughness', value: pt, label: 'Power/toughness' }
      : card.loyalty !== undefined
        ? { kind: 'loyalty', value: card.loyalty, label: 'Starting loyalty' }
        : undefined,
    counters: [],
    markers: card.token ? ['Token'] : [],
    tapped: false,
    artKey: card.token ? undefined : card.functional_id || undefined,
  }
}

/** A face for a permanent on the battlefield, with everything the board state adds to it. */
export function permanentFace(permanent: Permanent): CardFace {
  const base = cardFace(permanent.card)
  const counters = list(permanent.counters)
  const loyalty = counters.find((counter) => counter.kind === 'loyalty')
  const pt = powerToughness(permanent.card)

  // Power and toughness are server-computed and already effective, so they stand as sent.
  // Loyalty is not: the printed number from `base` is deliberately dropped here, and a
  // planeswalker with no loyalty counter shows no loyalty rather than its printed one.
  const stat: CardFaceStat | undefined = pt
    ? { kind: 'power_toughness', value: pt, label: 'Power/toughness' }
    : loyalty
      ? { kind: 'loyalty', value: String(loyalty.count), label: 'Loyalty' }
      : undefined

  return {
    ...base,
    stat,
    // Only strip the loyalty counter when it *is* the stat above, where repeating it would
    // show one number twice as though it were two facts. A permanent that is both a creature
    // and a planeswalker leads with its P/T, so its loyalty stays in the counter row rather
    // than vanishing from the board entirely.
    counters:
      stat?.kind === 'loyalty'
        ? counters.filter((counter) => counter.kind !== 'loyalty')
        : counters,
    damage: permanent.damage !== undefined && permanent.damage > 0 ? permanent.damage : undefined,
    markers: [...base.markers, ...(permanent.is_commander ? ['Commander'] : [])],
    tapped: permanent.tapped === true,
  }
}

/**
 * A face for one object on the stack.
 *
 * The face rides along when there is a card behind it; an ability has none, and its
 * server-composed `description` is the whole of what there is to show. The id is the stack
 * object's, not the card's, because that is what targets and the inspector address.
 */
export function stackFace(item: StackItem): CardFace {
  const kind = item.kind === undefined ? [] : [STACK_KIND_LABELS[item.kind] ?? item.kind]
  if (!item.card) {
    return {
      id: item.id,
      name: item.description,
      keywords: [],
      counters: [],
      markers: kind,
      tapped: false,
    }
  }
  const base = cardFace(item.card)
  return { ...base, id: item.id, markers: [...base.markers, ...kind] }
}

/**
 * A face for an emblem (CR 114).
 *
 * An emblem is public, in no zone, and never removed. It has no card, no cost, and no type
 * line — the shape carries abilities and nothing else, which is why the name is the only word
 * the client supplies and the abilities are rendered exactly as the server composed them.
 */
export function emblemFace(emblem: Emblem): CardFace {
  const abilities = list(emblem.abilities)
  return {
    id: emblem.id,
    name: 'Emblem',
    rulesText: abilities.length > 0 ? abilities.join(' ') : undefined,
    keywords: [],
    counters: [],
    markers: ['Emblem'],
    tapped: false,
  }
}

/**
 * A one-line summary of a face, for an accessible name and for surfaces with no room to draw.
 *
 * Built from the same model the frame draws, so what a screen reader hears and what the board
 * shows can never disagree.
 */
export function faceSummary(face: CardFace): string {
  const parts = [face.name]
  if (face.manaCost) parts.push(face.manaCost)
  if (face.typeLine) parts.push(face.typeLine)
  if (face.stat) parts.push(`${face.stat.label} ${face.stat.value}`)
  if (face.tapped) parts.push('tapped')
  if (face.damage !== undefined) parts.push(`${face.damage} damage`)
  for (const counter of face.counters) parts.push(`${counter.count}× ${counter.kind}`)
  parts.push(...face.markers)
  return parts.join(' · ')
}
