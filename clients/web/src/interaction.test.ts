import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { GameView, type ValidAction } from './protocol'
import {
  IDLE,
  actionsFor,
  arm,
  ask,
  clear,
  fill,
  focus,
  gestureFor,
  globalActions,
  highlightFor,
  needsChoices,
  needsConfirmation,
  owedActions,
  release,
  select,
  settle,
  slotsOf,
  submitted,
  subjects,
  unask,
  type Interaction,
} from './interaction'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const viewOf = (name: string): GameView =>
  GameView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

/** The contract fixture's own action list: a pass, a land, an X spell, and a mana ability. */
const ACTIONS: readonly ValidAction[] = viewOf('gameview-actions.json').valid_actions ?? []

const byId = (id: string): ValidAction => {
  const action = ACTIONS.find((each) => each.id === id)
  if (!action) throw new Error(`fixture no longer carries action ${id}`)
  return action
}

const armed = (id: string): Interaction => arm(IDLE, byId(id))

describe('reading the action list', () => {
  it('names every object the server attached an action to, and no others', () => {
    expect(subjects(ACTIONS)).toEqual(new Set(['c_forest', 'c_fireball', 'perm_elves']))
    expect(actionsFor(ACTIONS, 'perm_elves').map((action) => action.label)).toEqual([
      '{T}: Add {G}.',
    ])
    // The elves are a target candidate of the fireball as well as the subject of their own
    // ability; being pointed at by a slot does not make an object an owner of that action.
    expect(actionsFor(ACTIONS, 'perm_elves')).toHaveLength(1)
  })

  it('keeps an action with no subject as a global one', () => {
    expect(globalActions(ACTIONS).map((action) => action.id)).toEqual(['a0'])
  })

  it('owes nothing while the seat may simply pass', () => {
    expect(owedActions(ACTIONS)).toEqual([])
  })

  it('holds out the action the game is blocked on, subject or no subject', () => {
    // A trigger waiting to be aimed: no pass is offered, because play does not continue around
    // it. The choice is bound to its source — which is exactly why it needs saying here too.
    const blocked: ValidAction[] = [
      {
        id: 'b0',
        type: 'choose_targets',
        label: 'Skymarch Bloodletter: target opponent loses 1 life',
        subject: ['stack_7', 'perm_3'],
        token: 't1',
      },
      { id: 'b1', type: 'concede', label: 'Concede', subject: [], token: 't2' },
    ]
    expect(owedActions(blocked).map((action) => action.id)).toEqual(['b0'])
    // Conceding is offered in every one of these states and answers none of them; it is already
    // in the global list and stays there.
    expect(globalActions(blocked).map((action) => action.id)).toEqual(['b1'])
  })

  it('leaves an unrecognized category alone rather than hiding it', () => {
    const blocked: ValidAction[] = [
      {
        id: 'b0',
        type: 'some_future_choice',
        label: 'Answer it',
        subject: ['perm_3'],
        token: 't1',
      },
      { id: 'b1', type: 'concede', label: 'Concede', subject: [], token: 't2' },
    ]
    expect(owedActions(blocked).map((action) => action.id)).toEqual(['b0'])
  })

  it('tells a one-click action from one that asks something first', () => {
    expect(needsChoices(byId('a0'))).toBe(false)
    expect(needsChoices(byId('a3'))).toBe(false)
    expect(needsChoices(byId('a2'))).toBe(true)
  })
})

describe('the questions an action is asking', () => {
  it('puts a spell’s targets before its prompts, and carries the server’s bounds', () => {
    const slots = slotsOf(byId('a2'), {})
    expect(slots.map((slot) => [slot.slot, slot.kind])).toEqual([
      ['t0', 'target'],
      ['x', 'number'],
    ])
    expect(slots[0]?.candidates).toEqual(['perm_elves', 'p1'])
    // A requirement publishes no arity, so none is invented.
    expect([slots[0]?.min, slots[0]?.max]).toEqual([null, null])
    // X takes one id, whose *value* may be anywhere in the range the server computed. Reading
    // the count as the range would offer a spinner that only ever allows 1.
    expect([slots[1]?.min, slots[1]?.max]).toEqual([1, 1])
    expect(slots[1]?.range).toEqual({ min: 0, max: 3 })
  })

  it('withholds a slot until the option that requires it is chosen', () => {
    const mulligan = viewOf('gameview-prompts.json').valid_actions?.[0]
    if (!mulligan) throw new Error('fixture no longer carries a mulligan action')

    expect(slotsOf(mulligan, {}).map((slot) => slot.slot)).toEqual(['decision'])
    expect(slotsOf(mulligan, { decision: ['mulligan'] }).map((slot) => slot.slot)).toEqual([
      'decision',
    ])
    expect(slotsOf(mulligan, { decision: ['keep'] }).map((slot) => slot.slot)).toEqual([
      'decision',
      'bottom',
    ])
  })

  it('says when the server allowed an answer smaller than the maximum', () => {
    const choice = viewOf('gameview-choice.json').valid_actions?.[0]
    if (!choice) throw new Error('fixture no longer carries a choice action')

    const slot = slotsOf(choice, {})[0]
    expect(slot?.kind).toBe('zone')
    expect([slot?.min, slot?.max]).toEqual([0, 2])
    // "Choose up to 2" — bottoming nothing is an answer, not an unfinished draft.
    expect(slot?.optional).toBe(true)
  })
})

describe('where the next click lands', () => {
  /**
   * The shape of a combat declaration: one multi-select of who attacks, then one slot per
   * attacker naming what it attacks. There is no fixture for it, and its whole point here is
   * that the two slots have *disjoint* candidates.
   */
  const COMBAT: readonly ValidAction[] = [
    {
      id: 'atk',
      type: 'declare_attackers',
      label: 'Declare attackers',
      requirements: [
        {
          slot: 'attackers',
          prompt: 'Choose which creatures attack',
          candidates: ['bear', 'ogre'],
        },
        { slot: 'defend_bear', prompt: 'Choose what Bear attacks', candidates: ['p1', 'walker'] },
        { slot: 'defend_ogre', prompt: 'Choose what Ogre attacks', candidates: ['p1', 'walker'] },
      ],
    },
  ]
  const declaring = arm(IDLE, COMBAT[0]!)

  it('sends a click to the slot the server listed it in, not to a cursor', () => {
    expect(gestureFor(COMBAT, declaring, 'bear')).toEqual({ kind: 'fill', slot: 'attackers' })
    expect(gestureFor(COMBAT, declaring, 'walker')).toEqual({ kind: 'fill', slot: 'defend_bear' })
  })

  it('keeps taking attackers after the first one', () => {
    // The failure this exists for: a model that advanced to the next slot once `attackers` had
    // an answer could declare exactly one attacker, ever.
    const first = fill(declaring, slotsOf(COMBAT[0]!, {})[0]!, 'bear')
    expect(gestureFor(COMBAT, first, 'ogre')).toEqual({ kind: 'fill', slot: 'attackers' })
  })

  it('gives a chosen id back to the slot that is holding it', () => {
    const held = { ...declaring, draft: { defend_ogre: ['walker'] } }
    // Not `defend_bear`, which lists it too and is empty: un-choosing has to undo the choice
    // that was made, or a second click on a card silently moves it somewhere else.
    expect(gestureFor(COMBAT, held, 'walker')).toEqual({ kind: 'fill', slot: 'defend_ogre' })
  })

  it('highlights every slot’s candidates, since any of them may be clicked', () => {
    expect(highlightFor(COMBAT, declaring, 'bear')).toBe('candidate')
    expect(highlightFor(COMBAT, declaring, 'walker')).toBe('candidate')
    expect(highlightFor(COMBAT, declaring, 'somethingelse')).toBe('idle')
  })

  it('is ready only once every counted slot holds a count the server published', () => {
    const withTarget = fill(armed('a2'), slotsOf(byId('a2'), {})[0]!, 'p1')
    expect(focus(ACTIONS, withTarget).ready).toBe(false)

    const withX = { ...withTarget, draft: { ...withTarget.draft, x: ['2'] } }
    expect(focus(ACTIONS, withX).ready).toBe(true)
  })

  it('forgets an armed action the server has stopped offering', () => {
    expect(focus(ACTIONS, { draft: {}, armed: 'a9' })).toEqual({ slots: [], ready: false })
  })
})

describe('what one click means', () => {
  it('inspects an object the server never named', () => {
    expect(gestureFor(ACTIONS, IDLE, 'perm_bear')).toEqual({ kind: 'inspect' })
  })

  it('selects an object that owns an action', () => {
    expect(gestureFor(ACTIONS, IDLE, 'c_fireball')).toEqual({ kind: 'select' })
  })

  it('inspects the selected object on a second click', () => {
    const selected = select(IDLE, 'c_fireball')
    expect(gestureFor(ACTIONS, selected, 'c_fireball')).toEqual({ kind: 'inspect' })
  })

  it('fills the open slot from a candidate the server listed', () => {
    expect(gestureFor(ACTIONS, armed('a2'), 'p1')).toEqual({ kind: 'fill', slot: 't0' })
  })

  it('leaves an object outside the candidate list alone while drafting', () => {
    // The bear is a creature on the board and would be an obvious target — but the server did
    // not list it, and obvious is a rules judgment this client does not make.
    expect(gestureFor(ACTIONS, armed('a2'), 'perm_bear')).toEqual({ kind: 'inspect' })
  })
})

describe('what the table highlights', () => {
  it('marks every object with an action, and only those, when nothing is selected', () => {
    expect(highlightFor(ACTIONS, IDLE, 'c_forest')).toBe('candidate')
    expect(highlightFor(ACTIONS, IDLE, 'perm_elves')).toBe('candidate')
    expect(highlightFor(ACTIONS, IDLE, 'perm_bear')).toBe('idle')
  })

  it('narrows to the open slot’s candidates once an action is armed', () => {
    const drafting = armed('a2')
    expect(highlightFor(ACTIONS, drafting, 'c_fireball')).toBe('selected')
    expect(highlightFor(ACTIONS, drafting, 'perm_elves')).toBe('candidate')
    expect(highlightFor(ACTIONS, drafting, 'p1')).toBe('candidate')
    // The forest has an action of its own, and is not part of this one.
    expect(highlightFor(ACTIONS, drafting, 'c_forest')).toBe('idle')
  })

  it('marks a candidate as chosen once it is in the draft', () => {
    const chosen = fill(armed('a2'), slotsOf(byId('a2'), {})[0]!, 'perm_elves')
    expect(highlightFor(ACTIONS, chosen, 'perm_elves')).toBe('selected')
  })

  it('marks only what is in flight while a submission is unanswered', () => {
    const waiting = submitted(IDLE, { submission: 's:1', actionId: 'a2', label: 'Cast' })
    expect(highlightFor(ACTIONS, waiting, 'c_fireball')).toBe('pending')
    expect(highlightFor(ACTIONS, waiting, 'c_forest')).toBe('idle')
  })
})

describe('a submission in flight', () => {
  const waiting = submitted(select(IDLE, 'c_fireball'), {
    submission: 's:1',
    actionId: 'a2',
    label: 'Cast Emberfall Surge',
  })

  it('is cleared by the server’s own echo', () => {
    expect(settle(waiting, { submission: 's:1', accepted: true })).toEqual({ draft: {} })
  })

  it('keeps the label when the server refused it', () => {
    expect(settle(waiting, { submission: 's:1', accepted: false })).toEqual({
      draft: {},
      rejected: 'Cast Emberfall Surge',
    })
  })

  it('survives a view that answers a different submission', () => {
    // An ordinary broadcast — another seat acting — carries no ack of ours, and reading one as
    // an answer is exactly the race the correlation id exists to remove.
    expect(settle(waiting, undefined).pending).toEqual(waiting.pending)
    expect(settle(waiting, { submission: 's:9', accepted: true }).pending).toEqual(waiting.pending)
  })

  it('drops the draft either way, because the view it was built against is gone', () => {
    const drafting = submitted(fill(armed('a2'), slotsOf(byId('a2'), {})[0]!, 'p1'), {
      submission: 's:2',
      actionId: 'a2',
      label: 'Cast',
    })
    expect(settle(drafting, undefined).draft).toEqual({})
    expect(settle(drafting, undefined).armed).toBeUndefined()
  })

  it('can be given up on, for a server that will never send an ack', () => {
    expect(release(waiting).pending).toBeUndefined()
  })
})

describe('backing out', () => {
  it('abandons the draft when a different object is selected', () => {
    const drafting = fill(armed('a2'), slotsOf(byId('a2'), {})[0]!, 'p1')
    const moved = select(drafting, 'c_forest')
    expect(moved.draft).toEqual({})
    expect(moved.armed).toBeUndefined()
    expect(moved.selected).toBe('c_forest')
  })

  it('clears everything but what is already in flight', () => {
    const waiting = submitted(armed('a2'), { submission: 's:1', actionId: 'a2', label: 'Cast' })
    expect(clear(waiting)).toEqual({ draft: {}, pending: waiting.pending })
  })
})

describe('asking twice', () => {
  const concede: ValidAction = { id: 'a9', type: 'concede', label: 'Concede' }

  it('asks about conceding and about nothing else', () => {
    expect(needsConfirmation(concede)).toBe(true)
    for (const action of ACTIONS) expect(needsConfirmation(action)).toBe(false)
  })

  it('does not qualify a category this build has never heard of', () => {
    // Failing towards the ordinary path: a newer server's action is taken on one click, as
    // every action was before this existed, rather than silently becoming unusable.
    expect(needsConfirmation({ id: 'a9', type: 'ritual_sacrifice', label: 'Something new' })).toBe(
      false,
    )
  })

  it('sends nothing while the question is open', () => {
    const asked = ask(IDLE, concede)
    expect(asked.confirming).toBe('a9')
    expect(asked.draft).toEqual({})
    expect(asked.pending).toBeUndefined()
  })

  it('treats every other move as a no', () => {
    const asked = ask(IDLE, concede)
    expect(unask(asked).confirming).toBeUndefined()
    expect(select(asked, 'c_forest').confirming).toBeUndefined()
    expect(arm(asked, byId('a2')).confirming).toBeUndefined()
    expect(clear(asked).confirming).toBeUndefined()
    // And a new view, which withdraws the question along with everything else built against
    // the view it was asked on.
    expect(settle(asked, undefined).confirming).toBeUndefined()
  })

  it('keeps the selection, so backing out lands where the player was', () => {
    const asked = ask(select(IDLE, 'perm_17'), concede)
    expect(unask(asked).selected).toBe('perm_17')
  })
})
