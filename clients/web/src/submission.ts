/**
 * Composing one `choose_action` submission.
 *
 * This is bookkeeping over what the server advertised, **not** rules reasoning. The client
 * never decides whether a target is legal, whether a cost is payable, or whether a
 * declaration is sensible; it tracks which slots the player has filled and how many ids each
 * holds, against counts the server itself published.
 *
 * The one gate here is sanctioned by the protocol in as many words: "A client enables a choice
 * once every slot it requires holds exactly the advertised number of ids; the server enforces
 * the same coupling on resolution, so `requires` changes no legality, it only keeps a client
 * from offering an answer that must be rejected" (`docs/protocol.md`).
 *
 * Target *requirements* are deliberately never blocking. A requirement carries candidates but
 * no count, and the legal size genuinely varies — a burn spell wants exactly one, while
 * declaring no attackers at all is a legal declaration. Guessing either way would be the
 * client asserting a rule. So the player submits whatever they selected and the server
 * answers; a rejection comes back as `action_rejected` on the next view.
 */
import type { ChooseAction, Prompt, ValidAction } from './protocol'

/** Answers so far, keyed by slot. A `number` slot holds its value as a decimal string. */
export type Draft = Readonly<Record<string, readonly string[]>>

/** How many ids the server advertised for this prompt slot, or `null` when it sets no count. */
export function advertisedCount(prompt: Prompt): number | null {
  switch (prompt.kind) {
    case 'option':
      return 1
    case 'select_from_zone':
      return prompt.count
    case 'order':
      // An `order` answer is a permutation of every item, so its size is the item count.
      return prompt.items?.length ?? 0
    case 'number':
      return 1
  }
}

/**
 * Slots that are only owed an answer when a particular option is chosen.
 *
 * A slot named by some option's `requires` is conditional; one named by none is unconditional
 * and always owed. This is what lets a mulligan action carry a `bottom` slot that only the
 * *keep* choice must answer — taking another hand bottoms nothing.
 */
function conditionalSlots(action: ValidAction): ReadonlySet<string> {
  const slots = new Set<string>()
  for (const prompt of action.prompts ?? []) {
    if (prompt.kind !== 'option') continue
    for (const option of prompt.options ?? []) {
      for (const slot of option.requires ?? []) slots.add(slot)
    }
  }
  return slots
}

/** The prompt slots this draft currently owes an answer to. */
export function requiredSlots(action: ValidAction, draft: Draft): ReadonlySet<string> {
  const conditional = conditionalSlots(action)
  const required = new Set<string>()

  for (const prompt of action.prompts ?? []) {
    if (!conditional.has(prompt.slot)) required.add(prompt.slot)
  }

  // Whatever the chosen options pull in on top of that.
  for (const prompt of action.prompts ?? []) {
    if (prompt.kind !== 'option') continue
    const chosenId = draft[prompt.slot]?.[0]
    if (chosenId === undefined) continue
    const chosen = (prompt.options ?? []).find((o) => o.id === chosenId)
    for (const slot of chosen?.requires ?? []) required.add(slot)
  }

  return required
}

/** Whether every owed prompt slot holds exactly the number of ids the server advertised. */
export function isSubmittable(action: ValidAction, draft: Draft): boolean {
  const required = requiredSlots(action, draft)
  for (const prompt of action.prompts ?? []) {
    if (!required.has(prompt.slot)) continue
    const count = advertisedCount(prompt)
    if (count === null) continue
    if ((draft[prompt.slot]?.length ?? 0) !== count) return false
  }
  return true
}

/**
 * Build the message for this action and draft.
 *
 * Only owed prompt slots are carried: answering a slot the chosen option does not require
 * would be sending an answer the server must reject. Target requirement slots are carried
 * whenever the player selected anything for them.
 */
export function buildChooseAction(
  action: ValidAction,
  draft: Draft,
  submission?: string,
): ChooseAction & { type: 'choose_action' } {
  const required = requiredSlots(action, draft)
  const requirementSlots = new Set((action.requirements ?? []).map((r) => r.slot))

  const targets = Object.entries(draft)
    .filter(([slot, ids]) => {
      if (ids.length === 0) return false
      return required.has(slot) || requirementSlots.has(slot)
    })
    .map(([slot, ids]) => ({ slot, chosen: [...ids] }))

  const message: ChooseAction & { type: 'choose_action' } = {
    type: 'choose_action',
    action_id: action.id,
  }
  // Every optional field is omitted rather than sent empty, matching how the server elides.
  if (action.token) message.token = action.token
  if (targets.length > 0) message.targets = targets
  if (submission) message.submission = submission
  return message
}

/** Toggle one id within a slot, respecting the slot's advertised size. */
export function toggleSelection(
  draft: Draft,
  slot: string,
  id: string,
  limit: number | null,
): Draft {
  const current = draft[slot] ?? []
  if (current.includes(id)) {
    return { ...draft, [slot]: current.filter((existing) => existing !== id) }
  }
  // A single-answer slot replaces rather than accumulates, so clicking another option swaps
  // to it instead of silently composing an over-long, rejectable answer.
  if (limit === 1) return { ...draft, [slot]: [id] }
  if (limit !== null && current.length >= limit) return draft
  return { ...draft, [slot]: [...current, id] }
}
