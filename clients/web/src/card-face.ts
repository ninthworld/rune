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
import type {
  CardType,
  CardView,
  CatalogCard,
  Color,
  Counter,
  Emblem,
  Permanent,
  StackItem,
} from './protocol'
import { list, powerToughness } from './normalize'

/**
 * How a face is participating in whatever interaction is in progress.
 *
 * These are presentation states the caller assigns, never conclusions this module draws. The
 * client does not know what is legal — `candidate` means the server *named* this object, in an
 * action's `subject` or a requirement's `candidates`, and `disabled` means the caller chose not
 * to offer it here, not that a rule forbids it.
 */
export type CardFaceState = 'idle' | 'selected' | 'candidate' | 'pending' | 'disabled'

/**
 * How a face stands in the relationships of whatever is currently focused (`relations.ts`).
 *
 * `focus` is the object being traced from; `linked` is an object on the other end of a
 * relationship the server projected about it. Both are presentation, and both are drawn from
 * stated identifiers — a client that concluded two objects were related would be deciding a
 * fact about the game.
 */
export type CardFaceLink = 'focus' | 'linked' | undefined

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
  /**
   * Keywords this object has that its printed card does not — the trample an until-end-of-turn
   * pump gave it, the flying an Aura grants (`docs/protocol.md`, `granted_keywords`).
   *
   * The server's words, kept apart from `rulesText` because `rulesText` is what the *card*
   * prints and this is what is true of the object right now. A frame draws both; nothing here
   * works out which is which, and nothing anywhere reads a keyword back out of them.
   */
  grantedKeywords: readonly string[]
  /** Damage marked on a permanent this turn; absent when none is marked. */
  damage?: number
  /** Server-projected state a player must be able to see: token, commander, stack kind. */
  markers: readonly string[]
  tapped: boolean
  /** `functional_id` — the ADR 0012 art key. Absent for anything with no card identity. */
  artKey?: string
  /**
   * The card's types, as the server stated them (`board.ts`).
   *
   * Carried on the face so a surface that arranges cards never has to reach past it to the
   * view. Empty means the server stated none — which is a fact about the projection, not a
   * claim that the object has no types, and every surface treats it that way.
   */
  cardTypes: readonly CardType[]
  /**
   * The card's colour identity, as the server stated it (CR 903.4).
   *
   * Carried so a frame can be tinted by what a card *belongs to* rather than only by the pips
   * that happen to be printed on it — the difference between a green Forest and a grey one
   * (`mana.frameTint`). Empty means the server stated none, which is a colourless card and a
   * card the server could not resolve alike; both draw neutral, which is the honest answer.
   */
  colorIdentity: readonly Color[]
  /**
   * Whether the summoning-sickness restriction currently applies (CR 302.6), as the server
   * stated it. Never a conclusion: the client cannot see continuous control or a granted haste,
   * and `false` on a face the server said nothing about is "not stated", not "may attack".
   */
  summoningSick: boolean
  /**
   * Whether the server said this permanent sits out its controller's next untap step
   * (CR 502.4). Never a conclusion: the spell that imposed it has left the board, so
   * nothing the client can see would let it work this out.
   */
  skipsNextUntap: boolean
}

/**
 * The word for a colour letter the server states — the wire carries `"R"`, a player reads
 * "Red". Naming a stated value is not deriving one: nothing here works out what colour
 * anything is, and a letter this build has never seen is set as itself rather than dropped.
 */
const COLOR_WORDS: Record<string, string> = {
  W: 'White',
  U: 'Blue',
  B: 'Black',
  R: 'Red',
  G: 'Green',
}

/** Display wording for the stack kinds the server states (`docs/protocol.md`). */
const STACK_KIND_LABELS: Record<string, string> = {
  spell: 'Spell',
  ability: 'Ability',
  activated: 'Activated ability',
  triggered: 'Triggered ability',
}

/**
 * The characteristics printed on a card, whichever projection stated them.
 *
 * A `CardView` in a game and a `CatalogCard` in the lobby describe the same printed card — the
 * server builds both from one projection, so their rules text is byte-for-byte identical (ADR
 * 0008 §7). Reading them separately is how a deck builder and a hand end up disagreeing about a
 * card neither of them is allowed to have an opinion on, so both come through here.
 */
interface Printed {
  name: string
  mana_cost?: string
  type_line?: string
  rules_text?: string
  power?: string
  toughness?: string
  loyalty?: string
  keywords?: readonly string[]
  color_identity?: readonly Color[]
}

function printedFace(id: string, card: Printed): CardFace {
  const pt = powerToughness(card)
  return {
    id,
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
    grantedKeywords: [],
    markers: [],
    tapped: false,
    cardTypes: [],
    colorIdentity: list(card.color_identity),
    summoningSick: false,
    skipsNextUntap: false,
  }
}

/** A face for a card in a hand, a pile, or a reveal — anywhere that is not the battlefield. */
export function cardFace(card: CardView): CardFace {
  return {
    ...printedFace(card.id, card),
    markers: card.token ? ['Token'] : [],
    artKey: card.token ? undefined : card.functional_id || undefined,
    cardTypes: list(card.card_types),
  }
}

/**
 * A face for one entry in the public card catalog — a card as it is *browsed*, before any game
 * exists.
 *
 * A catalog entry names a card by identity rather than by instance (there is no per-game entity
 * id to have), so the `functional_id` is both the id every surface addresses it by and the art
 * key of ADR 0012 — the same handle a decklist submits. Nothing about a game rides here: no
 * counters, no tap state, no markers, because none of that is true of a card nobody has drawn.
 */
export function catalogFace(card: CatalogCard): CardFace {
  return {
    ...printedFace(card.functional_id, card),
    artKey: card.functional_id || undefined,
    cardTypes: list(card.card_types),
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
    // What this permanent has that its card never said. Stated by the server (CR 613.1f), so a
    // creature given trample for the turn says trample, on the card, while it has it.
    grantedKeywords: list(permanent.granted_keywords),
    damage: permanent.damage !== undefined && permanent.damage > 0 ? permanent.damage : undefined,
    // The colour named as this permanent entered (CR 614.12), stated by the server. It wears
    // the same pill a counter and a marker do because it is the same kind of fact — something
    // true of *this object* that its printed card never said — and the card's own text says
    // "the chosen color", so the board has to answer which one. Nothing is inferred: the
    // permanent may be colourless and the choice is one player's, made once, on entry.
    markers: [
      ...base.markers,
      ...(permanent.is_commander ? ['Commander'] : []),
      ...(permanent.chosen_color
        ? [`Chose ${COLOR_WORDS[permanent.chosen_color] ?? permanent.chosen_color}`]
        : []),
    ],
    tapped: permanent.tapped === true,
    // CR 302.6, stated. Nothing here concludes it from `entered_turn` (which is not on the
    // wire) or from the absence of an attack action (which means nothing outside one step).
    summoningSick: permanent.summoning_sick === true,
    // CR 502.4, stated. The spell that imposed it is gone, so there is nothing left on the
    // board to conclude it from.
    skipsNextUntap: permanent.skips_next_untap === true,
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
      grantedKeywords: [],
      markers: kind,
      tapped: false,
      cardTypes: [],
      colorIdentity: [],
      summoningSick: false,
      skipsNextUntap: false,
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
    grantedKeywords: [],
    markers: ['Emblem'],
    tapped: false,
    cardTypes: [],
    colorIdentity: [],
    summoningSick: false,
    skipsNextUntap: false,
  }
}

/**
 * A one-line summary of a face, for an accessible name and for surfaces with no room to draw.
 *
 * Built from the same model the frame draws, so what a screen reader hears and what the board
 * shows can never disagree.
 */
/**
 * Whether a paragraph of rules text is a line of keywords, and so set bold on the card.
 *
 * **The answer comes from the keywords the server stated**, not from reading the words: a line is
 * a keyword line when everything on it, comma by comma, is one of them. That is what makes
 * "Flying, haste" the same kind of line as "Flying", and keeps "First strike" one too — both of
 * which a rule about spaces gets wrong.
 *
 * Reminder text in brackets and any mana symbols are dropped before matching, because they are
 * printed alongside a keyword rather than being part of its name. A face the server stated no
 * keywords for falls back to that older reading, so nothing that never carried them changes.
 */
export function keywordLine(para: string, keywords: readonly string[]): boolean {
  const bare = para
    .replace(/\([^)]*\)/g, '')
    .replace(/\{[^}]+\}/g, '')
    .trim()
  if (bare === '') return false

  const parts = bare
    .split(',')
    .map((part) => part.trim())
    .filter((part) => part !== '')
  if (parts.length === 0) return false
  if (keywords.length === 0) return parts.length === 1 && !parts[0]!.includes(' ')

  const stated = new Set(keywords.map((keyword) => keyword.toLowerCase()))
  return parts.every((part) => stated.has(part.toLowerCase()))
}

/**
 * A planeswalker's loyalty ability, split into the cost it is activated for and the rest of the
 * line: `+1: Draw a card.` → `+1` and `Draw a card.`
 *
 * The server writes that cost as the signed number the card prints (CR 606.1) with the
 * typographic minus, and a card prints it inside a symbol rather than as text. A hyphen is
 * accepted too, because a minus sign is easy to lose in transit and the line still means the
 * same thing.
 */
export function loyaltyCost(para: string): { cost: string; rest: string } | undefined {
  const match = /^([+−-]?(?:\d+|X)):\s+(.*)$/s.exec(para.trim())
  if (!match) return undefined
  const [, cost, rest] = match
  return { cost: cost!.replace('-', '−'), rest: rest! }
}

export function faceSummary(face: CardFace): string {
  const parts = [face.name]
  if (face.manaCost) parts.push(face.manaCost)
  if (face.typeLine) parts.push(face.typeLine)
  if (face.stat) parts.push(`${face.stat.label} ${face.stat.value}`)
  if (face.tapped) parts.push('tapped')
  // The one board fact with no mark of its own that a player acts on every turn, so it is said
  // as well as drawn — a dimmed card says nothing when read aloud.
  if (face.summoningSick) parts.push('summoning sick')
  // Said for the same reason: a creature that stays tapped through an untap step is a rule
  // the board applied and never explained, and a reader that skipped it would describe a
  // creature that is simply, inexplicably, still tapped.
  if (face.skipsNextUntap) parts.push("doesn't untap next untap step")
  // Read aloud with the rest of what is true of the object, because it is not in the card's
  // printed text and a reader that skipped it would describe a creature without its trample.
  parts.push(...face.grantedKeywords)
  if (face.damage !== undefined) parts.push(`${face.damage} damage`)
  for (const counter of face.counters) parts.push(`${counter.count}× ${counter.kind}`)
  parts.push(...face.markers)
  return parts.join(' · ')
}
