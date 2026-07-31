import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { GameView, type ValidAction } from './protocol'
import {
  advertisedCount,
  buildChooseAction,
  isSubmittable,
  requiredSlots,
  toggleSelection,
} from './submission'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

/** The real mulligan action from the contract fixture, not a hand-made stand-in. */
function mulliganAction(): ValidAction {
  const view = GameView.parse(
    JSON.parse(readFileSync(join(FIXTURES, 'gameview-prompts.json'), 'utf8')),
  )
  const action = view.valid_actions?.find((a) => a.type === 'mulligan_decision')
  if (!action) throw new Error('fixture no longer carries a mulligan action')
  return action
}

describe('an action with no choices', () => {
  it('is submittable immediately and carries its token', () => {
    const concede: ValidAction = { id: 'a1', type: 'concede', label: 'Concede', token: 'tok' }
    expect(isSubmittable(concede, {})).toBe(true)
    expect(buildChooseAction(concede, {})).toEqual({
      type: 'choose_action',
      action_id: 'a1',
      token: 'tok',
    })
  })

  it('omits empty optional fields rather than sending them', () => {
    const action: ValidAction = { id: 'a0', type: 'pass_priority', label: 'Pass' }
    // The server elides empties; a client that sends `targets: []` and `token: ''` is noisier
    // than the contract and invites a needless rejection.
    expect(buildChooseAction(action, {})).toEqual({ type: 'choose_action', action_id: 'a0' })
  })
})

describe('conditional slots (an option that pulls another slot in)', () => {
  it('owes only the option slot before a choice is made', () => {
    const action = mulliganAction()
    // `bottom` is required by the *keep* option alone, so it is not owed yet.
    expect([...requiredSlots(action, {})]).toEqual(['decision'])
    expect(isSubmittable(action, {})).toBe(false)
  })

  it('owes the required slot once the choice that requires it is made', () => {
    const action = mulliganAction()
    const keeping = { decision: ['keep'] }
    expect([...requiredSlots(action, keeping)].sort()).toEqual(['bottom', 'decision'])
    // The zone prompt advertises a count of 1, so one card must be picked.
    expect(isSubmittable(action, keeping)).toBe(false)
    expect(isSubmittable(action, { ...keeping, bottom: ['card_10'] })).toBe(true)
  })

  it('owes nothing extra for a choice that requires nothing', () => {
    const action = mulliganAction()
    // Taking another hand bottoms nothing, so mulliganing is submittable on its own.
    expect(isSubmittable(action, { decision: ['mulligan'] })).toBe(true)
  })

  it('drops an answer the chosen option does not require', () => {
    const action = mulliganAction()
    // A player who picks *keep*, selects a card, then switches to *mulligan* must not ship the
    // stale `bottom` answer: the server would reject an answer to a slot it is not owed.
    const message = buildChooseAction(action, { decision: ['mulligan'], bottom: ['card_10'] })
    expect(message.targets).toEqual([{ slot: 'decision', chosen: ['mulligan'] }])
  })

  it('sends both answers when the choice does require them', () => {
    const action = mulliganAction()
    const message = buildChooseAction(action, { decision: ['keep'], bottom: ['card_11'] })
    expect(message.targets).toEqual([
      { slot: 'decision', chosen: ['keep'] },
      { slot: 'bottom', chosen: ['card_11'] },
    ])
  })
})

describe('advertised counts', () => {
  it('reads a zone prompt count from the server', () => {
    const zone = mulliganAction().prompts?.find((p) => p.kind === 'select_from_zone')
    expect(zone && advertisedCount(zone)).toBe(1)
  })

  it('treats an order prompt as a full permutation', () => {
    expect(
      advertisedCount({
        kind: 'order',
        slot: 'o0',
        prompt: 'Order blockers',
        items: ['perm_1', 'perm_2', 'perm_3'],
      }),
    ).toBe(3)
  })

  it('counts a number prompt as one value', () => {
    expect(advertisedCount({ kind: 'number', slot: 'x', prompt: 'X', min: 0, max: 5 })).toBe(1)
  })
})

describe('target requirements', () => {
  const attack: ValidAction = {
    id: 'a2',
    type: 'declare_attackers',
    label: 'Declare attackers',
    token: 'tok',
    requirements: [{ slot: 'attackers', prompt: 'Attack with', candidates: ['perm_1', 'perm_2'] }],
  }

  it('never blocks submission on a target slot', () => {
    // A requirement carries candidates but no count, and the legal size genuinely varies —
    // declaring no attackers is a legal declaration. Guessing either way would be a rule the
    // client invented, so the server decides and answers.
    expect(isSubmittable(attack, {})).toBe(true)
    expect(isSubmittable(attack, { attackers: ['perm_1'] })).toBe(true)
  })

  it('carries whatever was selected', () => {
    const message = buildChooseAction(attack, { attackers: ['perm_1', 'perm_2'] })
    expect(message.targets).toEqual([{ slot: 'attackers', chosen: ['perm_1', 'perm_2'] }])
  })

  it('omits a target slot the player left empty', () => {
    expect(buildChooseAction(attack, { attackers: [] }).targets).toBeUndefined()
  })
})

describe('toggling a selection', () => {
  it('replaces rather than accumulates in a single-answer slot', () => {
    // Clicking a second option swaps to it; composing a two-id answer to a one-id slot would
    // only produce a rejection.
    const after = toggleSelection({ decision: ['keep'] }, 'decision', 'mulligan', 1)
    expect(after.decision).toEqual(['mulligan'])
  })

  it('accumulates up to the advertised limit, then stops', () => {
    let draft = toggleSelection({}, 'bottom', 'card_10', 2)
    draft = toggleSelection(draft, 'bottom', 'card_11', 2)
    draft = toggleSelection(draft, 'bottom', 'card_12', 2)
    expect(draft.bottom).toEqual(['card_10', 'card_11'])
  })

  it('deselects an id that is already chosen', () => {
    const draft = toggleSelection({ attackers: ['perm_1', 'perm_2'] }, 'attackers', 'perm_1', null)
    expect(draft.attackers).toEqual(['perm_2'])
  })

  it('is unbounded when no count is advertised', () => {
    let draft = toggleSelection({}, 'attackers', 'perm_1', null)
    draft = toggleSelection(draft, 'attackers', 'perm_2', null)
    draft = toggleSelection(draft, 'attackers', 'perm_3', null)
    expect(draft.attackers).toHaveLength(3)
  })

  it('preserves click order, which an order prompt depends on', () => {
    let draft = toggleSelection({}, 'o0', 'perm_3', 3)
    draft = toggleSelection(draft, 'o0', 'perm_1', 3)
    expect(draft.o0).toEqual(['perm_3', 'perm_1'])
  })
})
