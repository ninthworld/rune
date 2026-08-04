import { describe, expect, it } from 'vitest'

import { IDLE, type Interaction } from './interaction'
import { objectMenu } from './menu'
import type { ValidAction } from './protocol'

const action = (id: string, subject?: string[]): ValidAction => ({
  id,
  label: id,
  type: 'activate',
  ...(subject ? { subject } : {}),
})

const ACTIONS = [
  action('tap', ['bear']),
  action('pump', ['bear']),
  action('sac', ['altar']),
  action('pass'),
]

const selected = (id: string, extra: Partial<Interaction> = {}): Interaction => ({
  draft: {},
  selected: id,
  ...extra,
})

describe('an object’s actions, at the object', () => {
  it('offers exactly what the server attached to that id, in the view’s order', () => {
    // The same list the dock draws, from the same call. Not a subset ranked by what an action
    // looks like it does — that reading is what the client does not do.
    expect(objectMenu(ACTIONS, selected('bear'))).toEqual({
      id: 'bear',
      actions: [ACTIONS[0], ACTIONS[1]],
    })
  })

  it('opens for an object with nothing offered, rather than refusing to', () => {
    // A player who clicked a creature and got no menu cannot tell "nothing right now" from
    // "the click missed". The empty answer is the useful one, and it is still an answer.
    expect(objectMenu(ACTIONS, selected('wall'))).toEqual({ id: 'wall', actions: [] })
  })

  it('stays closed while nothing is selected', () => {
    // Actions with no object live in the dock; that is the whole of #626's reason.
    expect(objectMenu(ACTIONS, IDLE)).toBeUndefined()
  })

  it('stays closed while an action is asking the board a question', () => {
    // An armed action turns the table into the answer sheet. A menu over a candidate would sit
    // between the question and the card that answers it.
    expect(objectMenu(ACTIONS, selected('bear', { armed: 'pump' }))).toBeUndefined()
  })

  it('stays closed while a card is being paid for', () => {
    // Clicking a card in hand the server offered nothing for says "I am playing this", which
    // selects it — and that selection used to open an empty panel over the hand, in the one
    // moment the player is being asked to click a land on the board instead.
    expect(objectMenu(ACTIONS, selected('bolt', { paying: 'bolt' }))).toBeUndefined()
  })

  it('stays closed while a match-ending action is being confirmed', () => {
    // Conceding is one button asked twice, and every other click is a "no".
    expect(objectMenu(ACTIONS, selected('bear', { confirming: 'concede' }))).toBeUndefined()
  })

  it('stays closed while a submission is in flight', () => {
    // Nothing can be taken until the server answers, so the menu would be a list of disabled
    // buttons floating over the board.
    const pending = { submission: 's:1', actionId: 'tap', label: 'Tap' }
    expect(objectMenu(ACTIONS, selected('bear', { pending }))).toBeUndefined()
  })
})
