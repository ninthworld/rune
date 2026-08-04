/**
 * Turning clicks on the table into one submission.
 *
 * `submission.ts` composes the message; this decides what a click *means* on the way there. It
 * is the same discipline: every question is answered out of `valid_actions` — which objects own
 * an action, which ids a slot will accept, how many it wants — and none of it is worked out from
 * the game. Ask this module "is that a legal target" and there is nothing here to answer with;
 * the only thing it knows is which ids the server itself listed.
 *
 * One gesture reaches every object, and what it does is decided in a fixed order:
 *
 *   1. **Fill** the slot being drafted, if the server listed this id among its candidates.
 *   2. **Inspect**, if this object is already selected — clicking again looks closer.
 *   3. **Take**, if the server offered exactly one action for this object. One click casts the
 *      spell, plays the land, taps for the mana.
 *   4. **Select**, if it offered more than one — the dock asks which.
 *   5. **Inspect**, because there is nothing else the client can offer for it.
 *
 * Rule 3 is the one worth defending. A card the server offered a single action for has an
 * unambiguous meaning for a click, and routing it through a selection and then a button in the
 * dock is two clicks and a change of focus to say something the view already said. Where the
 * server offered a *choice* — a creature that can attack and also activate — the click cannot
 * mean one thing, so it opens the list and the player picks. The client is still not deciding
 * anything: the count of actions the server attached to an object is the whole of the rule.
 *
 * Reading is not a click at all, which is what makes rule 3 safe. Looking at an object previews
 * its full face, and a right-click opens the inspector over any object at any time — so an
 * object whose single action now fires on one click did not become harder to read, it became
 * readable without spending a click on it.
 *
 * That order is also why a card the server did not name still opens the inspector on one click:
 * an object gives its first click to an action only where the server offered one.
 *
 * Two of those steps are *directed* rather than free, and both are the same idea: the game is
 * asking about one object at a time, so the click means what that object is being asked.
 *
 * - **Aiming.** A combat declaration is one action with several slots, and the server states
 *   which attacker each defender slot belongs to (`TargetRequirement.subject`). Choosing an
 *   attacker therefore *aims* it: the next click on a defender answers that attacker's slot and
 *   nothing else, and the board draws the arrow while it is being drafted. Without that, every
 *   defender slot lists the same candidates and one click could mean any of them.
 * - **Paying.** A card whose cost is not yet floating owns no action, so under rule 5 it was
 *   unclickable in the one moment a player most wants to click it. Clicking it now says *this is
 *   what I am doing*: the bar names the cost, the mana sources stay live, and the card is cast
 *   the moment the **server** offers a cast for it. Nothing here decides that moment — a client
 *   that added up pips would be computing cost, which is the whole of what this module is not.
 *
 * Almost nothing here survives a message. A new view rebuilds every derivation from that view,
 * and exactly two things cross the boundary — neither of them game state, and both of them facts
 * about what *this client's player* is in the middle of:
 *
 * - the submission the client is still waiting on, which is a fact about a message it sent, and
 *   which the server itself settles by echoing it back in `action_ack`;
 * - the card being paid for, which is a fact about what the player said they are doing. It has
 *   to survive, because making the mana for a spell is one message per source and an intent that
 *   died on the first view would be gone before the second land was tapped. It names a card and
 *   claims nothing, and a view that no longer draws that card simply stops showing it.
 */
import type { CardFaceState } from './card-face'
import type { ActionAck, ValidAction } from './protocol'
import {
  advertisedCount,
  advertisedMinimum,
  isSubmittable,
  requiredSlots,
  toggleSelection,
  type Draft,
} from './submission'

/** A submission sent and not yet answered, kept so the dock can say what is in flight. */
export interface Pending {
  submission: string
  actionId: string
  label: string
}

export interface Interaction {
  /** The object the player clicked, if any. Presentation: it selects nothing in the game. */
  readonly selected?: string
  /** The action being drafted, by id. */
  readonly armed?: string
  /** Answers so far, in the shape `submission.ts` will send. */
  readonly draft: Draft
  readonly pending?: Pending
  /** The label of the last submission the server refused, so the notice can name it. */
  readonly rejected?: string
  /** An action asked for a second, explicit click before it is sent. */
  readonly confirming?: string
  /**
   * The subject whose own slot the next click answers — an attacker that has been declared and
   * has not yet been given something to attack. Presentation only: the server stated the
   * pairing (`TargetRequirement.subject`), and this is which half of it the player is on.
   */
  readonly aiming?: string
  /**
   * A card the player has said they are playing, before the server offers it.
   *
   * Held across messages, unlike everything else here, because that is the whole point: the
   * player names the card and *then* makes the mana, which takes one message per source. It is
   * an intent and not a claim — it asserts nothing about legality, cost, or affordability, and
   * the action it is waiting for appears when the **server** says so.
   */
  readonly paying?: string
}

/** Nothing selected, nothing drafted, nothing in flight. */
export const IDLE: Interaction = { draft: {} }

/** Every object the server named as owning at least one action. */
export function subjects(actions: readonly ValidAction[]): ReadonlySet<string> {
  const named = new Set<string>()
  for (const action of actions) {
    for (const id of action.subject ?? []) named.add(id)
  }
  return named
}

/** The actions this object owns, in the order the server listed them. */
export function actionsFor(actions: readonly ValidAction[], id: string): readonly ValidAction[] {
  return actions.filter((action) => (action.subject ?? []).includes(id))
}

/**
 * The actions no visible object owns.
 *
 * "An empty subject identifies a global action such as passing priority" (`docs/protocol.md`) —
 * and a mulligan decision, a concede, and the combat declarations are the same shape. These
 * have nowhere on the table to be clicked, so the dock is where they live.
 */
export function globalActions(actions: readonly ValidAction[]): readonly ValidAction[] {
  return actions.filter((action) => (action.subject ?? []).length === 0)
}

/**
 * The actions the game is *waiting on this seat for*, when it is waiting on one.
 *
 * A player who holds priority may always pass it, so a view offering no `pass_priority` is not
 * offering a choice of whether to act: the seat owes one specific answer and play does not
 * continue until it gives it — a trigger to aim, the cleanup discard, a commander's return. Those
 * actions are bound to the object that is asking, and that binding is worth keeping: it is what
 * highlights the source on the board. But the dock lists them too, because a question the game
 * will not proceed past must never be reachable only by guessing which card to click.
 *
 * The judgment is made on the server's own `type`, "a free-form category used for presentation and
 * input routing" (`docs/protocol.md`) — the same stated classifier `needsConfirmation` reads, and
 * the same failure direction: a build that does not recognize `pass_priority` shows a few extra
 * buttons, which is the harmless way to be wrong.
 *
 * Global actions are left out, since the dock already lists those; conceding is offered in every
 * one of these states and answers none of them, so it stays where it lives.
 */
export function owedActions(actions: readonly ValidAction[]): readonly ValidAction[] {
  if (actions.some((action) => action.type === 'pass_priority')) return []
  return actions.filter((action) => action.type !== 'concede' && (action.subject ?? []).length > 0)
}

/** Whether taking this action asks the player anything first. */
export function needsChoices(action: ValidAction): boolean {
  return (action.requirements?.length ?? 0) > 0 || (action.prompts?.length ?? 0) > 0
}

/**
 * Whether one click is too few for this action.
 *
 * Conceding ends the game for the player who does it (CR 104.3a) and nothing in the game undoes
 * it, so it is the one action where a misclick is unrecoverable — a button that ends a match must
 * be asked twice. The judgment is made on the server's own `type`, which is "a free-form category
 * used for presentation and input routing" (`docs/protocol.md`), so this stays a presentation
 * decision about a stated classifier rather than the client deciding what an action does. A
 * `type` this build does not know simply does not qualify, which fails towards the ordinary path.
 */
export function needsConfirmation(action: ValidAction): boolean {
  return action.type === 'concede'
}

export type SlotKind = 'target' | 'zone' | 'order' | 'option' | 'number'

/** One question an armed action is asking, with everything needed to draw and answer it. */
export interface Slot {
  slot: string
  kind: SlotKind
  prompt: string
  /** Entity ids the server listed, in its order. Empty for a `number`. */
  candidates: readonly string[]
  /** The server's own option ids and labels, for an `option`. */
  options: readonly { id: string; label: string }[]
  chosen: readonly string[]
  /** How many ids the slot takes, or `null` where the server published no count. */
  min: number | null
  max: number | null
  /**
   * The bounds of a `number` slot's **value**, which is a different question from how many ids
   * it holds — a number slot always holds exactly one, and that one may be anywhere in here.
   * Absent for every other kind.
   */
  range?: { min: number; max: number }
  /** The slot may be left unanswered — the "up to" of an "up to two target creatures". */
  optional: boolean
  /** Whether clicking an object on the table answers this slot. */
  byEntity: boolean
  /**
   * The entity this slot is *about*, as the server stated it — the attacker whose defender this
   * chooses, the attacker these blockers are assigned to. Absent for a slot about the action as
   * a whole.
   */
  subject?: string
  /**
   * Whether this slot exists **because** its subject was chosen in another slot of the same
   * action, rather than existing outright.
   *
   * The distinction is the difference between "what does this attacker attack" — which is only a
   * question once you have declared that attacker, and must then be answered — and "what blocks
   * this attacker", which is a question about a board fact and whose answer may legally be
   * nothing. Both are subject slots; only one is owed.
   */
  conditional: boolean
}

/**
 * The entities this action asks the player to *choose*, across its subject-less slots.
 *
 * A slot whose subject is one of these is a follow-up question about a choice already made, so
 * it appears once that choice is made and is owed an answer from then on. A slot whose subject
 * is not — an attacker on the other side of the table — is a question about the board, asked
 * outright. Nothing here reads what the action *is*: the shape of its own slots is the whole
 * of the rule.
 */
function choosableSubjects(action: ValidAction): ReadonlySet<string> {
  const ids = new Set<string>()
  for (const requirement of action.requirements ?? []) {
    if (requirement.subject !== undefined) continue
    for (const id of requirement.candidates ?? []) ids.add(id)
  }
  return ids
}

/** Every id this draft holds, in any slot. */
const drafted = (draft: Draft): ReadonlySet<string> =>
  new Set(Object.values(draft).flatMap((ids) => [...ids]))

/**
 * The questions this action is currently asking, given what has been answered so far.
 *
 * Target requirements come first — they are the spell's own targets and read as part of casting
 * it — then the prompts the draft still owes. A prompt slot that only some option requires
 * appears once that option is chosen and not before (`requiredSlots`), which is what lets a
 * mulligan show its bottoming slot only after *keep*.
 */
export function slotsOf(action: ValidAction, draft: Draft): readonly Slot[] {
  const owed = requiredSlots(action, draft)
  const choosable = choosableSubjects(action)
  const chosen = drafted(draft)
  const slots: Slot[] = []

  for (const requirement of action.requirements ?? []) {
    const subject = requirement.subject
    const conditional = subject !== undefined && choosable.has(subject)
    // A question about a choice that has not been made is not a question yet. Declaring three
    // attackers used to show a defender slot for every creature that *could* have attacked.
    if (conditional && subject !== undefined && !chosen.has(subject)) continue
    slots.push({
      slot: requirement.slot,
      kind: 'target',
      prompt: requirement.prompt,
      candidates: requirement.candidates ?? [],
      options: [],
      chosen: draft[requirement.slot] ?? [],
      // A requirement publishes no count, and the legal size genuinely varies: one for a burn
      // spell, any number — including none — for a combat declaration. The server knows which;
      // this carries no bound rather than inventing one (`submission.ts`).
      min: null,
      max: null,
      optional: requirement.optional ?? false,
      byEntity: true,
      ...(subject === undefined ? {} : { subject }),
      conditional,
    })
  }

  for (const prompt of action.prompts ?? []) {
    if (!owed.has(prompt.slot)) continue
    const chosen = draft[prompt.slot] ?? []
    const max = advertisedCount(prompt)
    const min = advertisedMinimum(prompt)
    const shared = { slot: prompt.slot, prompt: prompt.prompt, chosen, min, max }

    switch (prompt.kind) {
      case 'option':
        slots.push({
          ...shared,
          kind: 'option',
          candidates: [],
          options: (prompt.options ?? []).map((option) => ({
            id: option.id,
            label: option.label,
          })),
          optional: false,
          byEntity: false,
          conditional: false,
        })
        break
      case 'select_from_zone':
        slots.push({
          ...shared,
          kind: 'zone',
          candidates: prompt.candidates ?? [],
          options: [],
          // The server said fewer than the maximum is an answer: scrying any number of the
          // cards looked at, or failing to find.
          optional: (min ?? 0) === 0,
          byEntity: true,
          conditional: false,
        })
        break
      case 'order':
        slots.push({
          ...shared,
          kind: 'order',
          candidates: prompt.items ?? [],
          options: [],
          optional: false,
          byEntity: true,
          conditional: false,
        })
        break
      case 'number':
        slots.push({
          ...shared,
          kind: 'number',
          candidates: [],
          options: [],
          // The bounds are the server's, computed from mana, the source's text, and the state.
          range: { min: prompt.min, max: prompt.max },
          optional: false,
          byEntity: false,
          conditional: false,
        })
        break
    }
  }

  return slots
}

/** What the dock is currently asking, derived fresh from the view and the draft. */
export interface Focus {
  /** The armed action, if it is still one the server offers. */
  action?: ValidAction
  slots: readonly Slot[]
  /** Whether every owed slot holds a count the server said it would take. */
  ready: boolean
}

export function focus(actions: readonly ValidAction[], interaction: Interaction): Focus {
  const action = actions.find((candidate) => candidate.id === interaction.armed)
  if (!action) return { slots: [], ready: false }
  const slots = slotsOf(action, interaction.draft)
  return {
    action,
    slots,
    // A conditional slot is only here because its subject was chosen, and it is owed from then
    // on: an attacker in the declaration with nothing named to attack is an answer the server
    // must reject, so the client does not offer to send it. This is the same bookkeeping
    // `submission.isSubmittable` does — over slots the server itself published — and it is not a
    // rule: which slots exist, and for which subjects, is entirely the server's statement.
    ready: isSubmittable(action, interaction.draft) && slots.every(answered),
  }
}

const answered = (slot: Slot): boolean => !slot.conditional || slot.chosen.length > 0

/**
 * Which question a click on `id` answers.
 *
 * Not "the slot the player is currently on" — that reading cannot declare an attack, because
 * choosing three attackers and then what each one attacks is one action with several slots and
 * no sequence between them. It is the slot that *lists this id*, which the server decided when
 * it enumerated candidates per slot; a creature that may attack is in `attackers`, and the
 * planeswalker it may attack is in a different slot entirely.
 *
 * Only where one id genuinely appears twice — the two halves of a spell with identical target
 * specs — is there anything to break, and it breaks in reading order: give it back where it is
 * already held, else put it in the first slot with room. Either way each slot's own list in the
 * dock is the exact path, so an ambiguity is never a dead end.
 */
function slotFor(slots: readonly Slot[], id: string, aiming?: string): Slot | undefined {
  const takes = slots.filter((slot) => slot.byEntity && slot.candidates.includes(id))
  return (
    // What is being aimed wins over everything: while an attacker is waiting to be told what it
    // attacks, a click on a defender is that attacker's answer and cannot be another's.
    (aiming === undefined ? undefined : takes.find((slot) => slot.subject === aiming)) ??
    takes.find((slot) => slot.chosen.includes(id)) ??
    takes.find((slot) => slot.max === null || slot.chosen.length < slot.max) ??
    takes[0]
  )
}

/** The slot currently being aimed, if the player is in the middle of aiming one. */
export function aimedSlot(slots: readonly Slot[], interaction: Interaction): Slot | undefined {
  if (interaction.aiming === undefined) return undefined
  return slots.find((slot) => slot.subject === interaction.aiming)
}

export type Gesture =
  | { kind: 'fill'; slot: string }
  | { kind: 'take'; action: string }
  | { kind: 'select' }
  | { kind: 'pay' }
  | { kind: 'inspect' }

/**
 * What one click on the object `id` means right now. The order is this module's whole rule.
 *
 * `payable` is the set of ids the caller is willing to let a player *declare an intent to play*
 * — a card in their own hand, while the server is offering at least one mana source. It is a
 * fact about what this client drew and what the server listed, and never a judgment that the
 * card could be cast: rule 3 still comes first, so a card the server *did* offer an action for
 * is simply taken.
 */
export function gestureFor(
  actions: readonly ValidAction[],
  interaction: Interaction,
  id: string,
  payable: ReadonlySet<string> = new Set(),
): Gesture {
  const open = slotFor(focus(actions, interaction).slots, id, interaction.aiming)
  if (open) return { kind: 'fill', slot: open.slot }
  if (interaction.selected === id) return { kind: 'inspect' }

  // One action means the click has one meaning, so it is that meaning. More than one means the
  // click has no single meaning, and inventing one — a "primary" action ranked by a type this
  // client would have to interpret — is exactly the rules reasoning that does not live here.
  const [only, ...rest] = actionsFor(actions, id)
  if (only) return rest.length === 0 ? { kind: 'take', action: only.id } : { kind: 'select' }

  // Nothing offered for it, and it is a card the player holds: the click is "I am playing this",
  // which is a statement of intent the bar then carries while the mana is made.
  if (payable.has(id) && interaction.paying !== id) return { kind: 'pay' }
  return { kind: 'inspect' }
}

/**
 * How an object is taking part in the interaction, for the frame it is drawn in.
 *
 * Only ids the server itself listed are ever marked as choosable. Everything else is `idle`,
 * including an object that would obviously be a legal target — obvious is a rules judgment, and
 * the absence of an id from `candidates` is the server's answer to it.
 */
export function highlightFor(
  actions: readonly ValidAction[],
  interaction: Interaction,
  id: string,
): CardFaceState {
  if (interaction.pending) {
    const inFlight = actions.find((action) => action.id === interaction.pending?.actionId)
    return (inFlight?.subject ?? []).includes(id) ? 'pending' : 'idle'
  }

  const current = focus(actions, interaction)
  if (current.action) {
    if (Object.values(interaction.draft).some((ids) => ids.includes(id))) return 'selected'
    if ((current.action.subject ?? []).includes(id)) return 'selected'
    // While one subject is being aimed, only its own slot's candidates are choosable — the
    // board says "this attacker, now pick what it attacks" rather than lighting up every
    // creature that could also have attacked.
    const aimed = aimedSlot(current.slots, interaction)
    if (aimed) return aimed.candidates.includes(id) ? 'candidate' : 'idle'
    return slotFor(current.slots, id, interaction.aiming) ? 'candidate' : 'idle'
  }

  // A card the player said they are playing stays lit while its cost is made, and every source
  // the server offered is a candidate — which is the whole of what "pay this" looks like on the
  // board. The client is not saying which of them would finish the cost; it does not know.
  if (interaction.paying !== undefined) {
    if (interaction.paying === id) return 'selected'
    return manaSubjects(actions).has(id) ? 'candidate' : 'idle'
  }

  if (interaction.selected === id) return 'selected'
  return subjects(actions).has(id) ? 'candidate' : 'idle'
}

/**
 * Every object the server offered a **mana ability** on (CR 605).
 *
 * Read straight off `mana_ability`, "server-computed so a client may offer a lighter gesture …
 * without ever classifying abilities itself" (`docs/protocol.md`). A build that does not see the
 * flag simply lights nothing extra, which is the harmless direction.
 */
export function manaSubjects(actions: readonly ValidAction[]): ReadonlySet<string> {
  const named = new Set<string>()
  for (const action of actions) {
    if (action.mana_ability !== true) continue
    for (const id of action.subject ?? []) named.add(id)
  }
  return named
}

// ---------------------------------------------------------------------------
// Transitions. Each returns a whole new interaction; none mutates.
// ---------------------------------------------------------------------------

/** Select an object. Any draft in progress is abandoned — a new subject is a new intent. */
export function select(interaction: Interaction, id: string): Interaction {
  return { draft: {}, selected: id, pending: interaction.pending }
}

/** Arm an action, selecting whatever the server said it belongs to. */
export function arm(interaction: Interaction, action: ValidAction): Interaction {
  return {
    draft: {},
    armed: action.id,
    selected: (action.subject ?? [])[0] ?? interaction.selected,
    pending: interaction.pending,
  }
}

/**
 * Put an id into a slot, or take it back out, respecting the count the server advertised.
 *
 * `slots` is the rest of the question, so that answering one part of it can *aim* the next:
 * choosing an attacker leaves the declaration pointing at that attacker, and the click after it
 * answers what that attacker attacks. Taking the attacker back out un-aims it, and answering the
 * aimed slot finishes it — so the sequence is attacker, target, attacker, target, with no mode
 * to enter and none to leave.
 */
export function fill(
  interaction: Interaction,
  slot: Slot,
  id: string,
  slots: readonly Slot[] = [],
): Interaction {
  const draft = toggleSelection(interaction.draft, slot.slot, id, slot.max)
  const next = { ...interaction, draft }
  if (slot.subject !== undefined) return { ...next, aiming: undefined }

  const added = (draft[slot.slot] ?? []).includes(id)
  if (!added) {
    return interaction.aiming === id ? { ...next, aiming: undefined } : next
  }
  const owed = slots.some((candidate) => candidate.subject === id)
  return owed ? { ...next, aiming: id } : next
}

/**
 * Say that this card is the one being played, before the server has offered it.
 *
 * Nothing is sent and nothing is claimed. The bar names the printed cost, the mana sources stay
 * live on the board, and the cast happens when the server lists it — so a player who taps the
 * wrong land has made the mana the rules say they made, exactly as at a table.
 */
export function payFor(interaction: Interaction, id: string): Interaction {
  return { draft: {}, selected: id, paying: id, pending: interaction.pending }
}

/** Give up on playing that card. The mana already made stays made; nothing was sent to undo. */
export function stopPaying(interaction: Interaction): Interaction {
  return { draft: {}, selected: interaction.selected, pending: interaction.pending }
}

/** Answer a slot outright — an option chosen, a number typed. */
export function answer(
  interaction: Interaction,
  slot: string,
  ids: readonly string[],
): Interaction {
  return { ...interaction, draft: { ...interaction.draft, [slot]: [...ids] } }
}

/**
 * Take back every answer without leaving the question.
 *
 * The way out of a half-built combat declaration that is *not* the way out of declaring: three
 * attackers aimed at the wrong things are undone in one click and the declaration is still the
 * thing being answered. Backing out of the action entirely is `disarm`, which is the second
 * press of the same control and the Escape key.
 */
export function reset(interaction: Interaction): Interaction {
  return { ...interaction, draft: {}, aiming: undefined }
}

/**
 * Ask before sending. Nothing is in flight and nothing is drafted — this is a question.
 *
 * Every other transition returns an interaction without `confirming`, so any click elsewhere —
 * another object, another action, a cancel — is a "no". Only clicking the same action again is a
 * "yes", which is what makes the second click deliberate rather than merely another click.
 */
export function ask(interaction: Interaction, action: ValidAction): Interaction {
  return {
    draft: {},
    selected: interaction.selected,
    pending: interaction.pending,
    confirming: action.id,
  }
}

/** Take back the question. */
export function unask(interaction: Interaction): Interaction {
  return { draft: {}, selected: interaction.selected, pending: interaction.pending }
}

/** Back out of a draft, keeping the subject selected. Nothing was sent, so nothing is undone. */
export function disarm(interaction: Interaction): Interaction {
  return { draft: {}, selected: interaction.selected, pending: interaction.pending }
}

/** Drop the selection entirely. */
export function clear(interaction: Interaction): Interaction {
  return { draft: {}, pending: interaction.pending }
}

/** Record that a submission went out. Until it is answered, nothing else may be sent. */
export function submitted(interaction: Interaction, pending: Pending): Interaction {
  return { ...interaction, pending }
}

/**
 * Whether this submission is the cast the player has been paying for.
 *
 * The intent ends when the card it named is played, and not before: every other submission a
 * paying player makes is a mana source being tapped, which is the intent being *carried out*.
 */
export const finishesPayment = (interaction: Interaction, action: ValidAction): boolean =>
  interaction.paying !== undefined && (action.subject ?? []).includes(interaction.paying)

/**
 * Stop waiting on an unanswered submission.
 *
 * The ack is advisory: an older server never sends one, and a view that carries none "says
 * nothing about a submission in flight" (`docs/protocol.md`), so a client that treated the next
 * view as an answer would be inventing the very race the correlation exists to remove. Waiting
 * is therefore correct and this is the way out of it — the player says the reply is not coming.
 */
export function release(interaction: Interaction): Interaction {
  return { ...interaction, pending: undefined }
}

/**
 * Settle against a newly arrived view.
 *
 * Everything the player was building is dropped: the view is the whole truth, and a draft
 * assembled against the previous one names ids and slots this one may no longer offer. The
 * pending submission is the single exception, and it is settled by the server's own echo — a
 * matching `action_ack` answers it, anything else leaves it in flight.
 */
export function settle(interaction: Interaction, ack: ActionAck | undefined): Interaction {
  const pending = interaction.pending
  // The payment intent is the one other thing that crosses the boundary, and for the same
  // reason: making the mana for a spell is one message per source, so an intent that did not
  // survive a view would be gone before the second land was tapped. It is not game state — it
  // names a card and asserts nothing — and a view that no longer draws that card simply stops
  // showing it (`Board`), which is how it ends when the card is played or leaves the hand.
  const paying = interaction.paying
  if (pending && ack && ack.submission === pending.submission) {
    return { draft: {}, paying, rejected: ack.accepted ? undefined : pending.label }
  }
  return { draft: {}, paying, pending }
}
