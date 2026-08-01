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
 *   3. **Select**, if the server named this object as the subject of an action.
 *   4. **Inspect**, because there is nothing else the client can offer for it.
 *
 * That order is why a card the server did not name still opens the inspector on one click, and
 * why reading a card is never lost behind an action: an object with actions gives up its
 * inspector to the second click and to the dock, never to nothing.
 *
 * Nothing here survives a message. A new view rebuilds every derivation from that view, and the
 * only thing carried across the boundary is the submission the client is still waiting on —
 * which is not game state but a fact about a message this client sent, and which the server
 * itself settles by echoing it back in `action_ack`.
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

/** Whether taking this action asks the player anything first. */
export function needsChoices(action: ValidAction): boolean {
  return (action.requirements?.length ?? 0) > 0 || (action.prompts?.length ?? 0) > 0
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
}

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
  const slots: Slot[] = []

  for (const requirement of action.requirements ?? []) {
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
  return {
    action,
    slots: slotsOf(action, interaction.draft),
    ready: isSubmittable(action, interaction.draft),
  }
}

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
function slotFor(slots: readonly Slot[], id: string): Slot | undefined {
  const takes = slots.filter((slot) => slot.byEntity && slot.candidates.includes(id))
  return (
    takes.find((slot) => slot.chosen.includes(id)) ??
    takes.find((slot) => slot.max === null || slot.chosen.length < slot.max) ??
    takes[0]
  )
}

export type Gesture = { kind: 'fill'; slot: string } | { kind: 'select' } | { kind: 'inspect' }

/** What one click on the object `id` means right now. The order is this module's whole rule. */
export function gestureFor(
  actions: readonly ValidAction[],
  interaction: Interaction,
  id: string,
): Gesture {
  const open = slotFor(focus(actions, interaction).slots, id)
  if (open) return { kind: 'fill', slot: open.slot }
  if (interaction.selected === id) return { kind: 'inspect' }
  if (subjects(actions).has(id)) return { kind: 'select' }
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
    return slotFor(current.slots, id) ? 'candidate' : 'idle'
  }

  if (interaction.selected === id) return 'selected'
  return subjects(actions).has(id) ? 'candidate' : 'idle'
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

/** Put an id into a slot, or take it back out, respecting the count the server advertised. */
export function fill(interaction: Interaction, slot: Slot, id: string): Interaction {
  return { ...interaction, draft: toggleSelection(interaction.draft, slot.slot, id, slot.max) }
}

/** Answer a slot outright — an option chosen, a number typed. */
export function answer(
  interaction: Interaction,
  slot: string,
  ids: readonly string[],
): Interaction {
  return { ...interaction, draft: { ...interaction.draft, [slot]: [...ids] } }
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
  if (pending && ack && ack.submission === pending.submission) {
    return { draft: {}, rejected: ack.accepted ? undefined : pending.label }
  }
  return { draft: {}, pending }
}
