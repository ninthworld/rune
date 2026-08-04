import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { GameView, type ValidAction } from './protocol'
import { buildChooseAction } from './submission'
import {
  IDLE,
  actionsFor,
  answer,
  arm,
  ask,
  disarm,
  remainingCost,
  spentSources,
  waysToPay,
  clear,
  fill,
  focus,
  gestureFor,
  globalActions,
  highlightFor,
  finishesPayment,
  needsChoices,
  needsConfirmation,
  owedActions,
  payFor,
  release,
  reset,
  select,
  settle,
  slotsOf,
  stopPaying,
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

  it('takes the one action the server offered for an object', () => {
    // One action is one meaning, so the click is that meaning: casting the fireball. Routing it
    // through a selection and a button in the dock would be two clicks to say what the view
    // already said.
    expect(gestureFor(ACTIONS, IDLE, 'c_fireball')).toEqual({ kind: 'take', action: 'a2' })
  })

  it('opens the list when the server offered more than one', () => {
    // A click cannot mean two things, and choosing a "primary" would be this client ranking
    // actions by a type it would have to interpret. The count is the whole rule.
    const both: ValidAction[] = [
      { id: 'x1', type: 'attack', label: 'Attack', subject: ['perm_bear'] },
      { id: 'x2', type: 'activate', label: '{T}: Draw a card', subject: ['perm_bear'] },
    ]
    expect(gestureFor(both, IDLE, 'perm_bear')).toEqual({ kind: 'select' })
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

describe('declaring attackers one at a time', () => {
  // The server states which attacker each defender slot belongs to, so the client can ask the
  // question the way a player thinks about it — this creature, at that seat — instead of
  // showing every candidate's slot at once and letting one click mean any of them.
  const DECLARE: ValidAction = {
    id: 'a_attack',
    type: 'declare_attackers',
    label: 'Declare attackers',
    requirements: [
      {
        slot: 'attackers',
        prompt: 'Choose which creatures attack',
        candidates: ['perm_bear', 'perm_ogre'],
      },
      {
        slot: 'defend_bear',
        prompt: 'Choose what Bear attacks',
        subject: 'perm_bear',
        candidates: ['p2', 'perm_walker'],
      },
      {
        slot: 'defend_ogre',
        prompt: 'Choose what Ogre attacks',
        subject: 'perm_ogre',
        candidates: ['p2', 'perm_walker'],
      },
    ],
  }
  const OFFERED = [DECLARE]

  it('asks nothing about an attacker that has not been declared', () => {
    expect(slotsOf(DECLARE, {}).map((slot) => slot.slot)).toEqual(['attackers'])
  })

  it('asks what an attacker attacks as soon as it is declared, and only for that one', () => {
    const slots = slotsOf(DECLARE, { attackers: ['perm_bear'] })
    expect(slots.map((slot) => slot.slot)).toEqual(['attackers', 'defend_bear'])
    expect(slots[1]?.subject).toBe('perm_bear')
    expect(slots[1]?.conditional).toBe(true)
  })

  it('aims the attacker it was just given, so the next click is that attacker’s answer', () => {
    const armedDeclare = arm(IDLE, DECLARE)
    const slots = slotsOf(DECLARE, {})
    const picked = fill(
      armedDeclare,
      slots[0] as never,
      'perm_bear',
      slotsOf(DECLARE, {
        attackers: ['perm_bear'],
      }),
    )
    expect(picked.aiming).toBe('perm_bear')
    // And a click on a defender answers *that* attacker's slot rather than the first one that
    // happens to list the same id.
    expect(gestureFor(OFFERED, picked, 'p2')).toEqual({ kind: 'fill', slot: 'defend_bear' })
  })

  it('stops aiming once the attacker has been given something to attack', () => {
    const withAim: Interaction = {
      armed: 'a_attack',
      draft: { attackers: ['perm_bear'] },
      aiming: 'perm_bear',
    }
    const slots = focus(OFFERED, withAim).slots
    const aimed = slots.find((slot) => slot.slot === 'defend_bear')
    const answered = fill(withAim, aimed as never, 'p2', slots)
    expect(answered.aiming).toBeUndefined()
    expect(answered.draft.defend_bear).toEqual(['p2'])
  })

  it('stops aiming when the attacker is taken back out of the declaration', () => {
    const withAim: Interaction = {
      armed: 'a_attack',
      draft: { attackers: ['perm_bear'] },
      aiming: 'perm_bear',
    }
    const slots = focus(OFFERED, withAim).slots
    const undone = fill(withAim, slots[0] as never, 'perm_bear', slots)
    expect(undone.draft.attackers).toEqual([])
    expect(undone.aiming).toBeUndefined()
  })

  it('lights only what the aimed attacker may attack', () => {
    const withAim: Interaction = {
      armed: 'a_attack',
      draft: { attackers: ['perm_bear'] },
      aiming: 'perm_bear',
    }
    expect(highlightFor(OFFERED, withAim, 'p2')).toBe('candidate')
    // The other creature is still a legal attacker, but this is not the question being asked.
    expect(highlightFor(OFFERED, withAim, 'perm_ogre')).toBe('idle')
  })

  it('will not send a declaration with an attacker that has nothing to attack', () => {
    const half: Interaction = { armed: 'a_attack', draft: { attackers: ['perm_bear'] } }
    expect(focus(OFFERED, half).ready).toBe(false)
    const whole: Interaction = {
      armed: 'a_attack',
      draft: { attackers: ['perm_bear'], defend_bear: ['p2'] },
    }
    expect(focus(OFFERED, whole).ready).toBe(true)
    // Declaring nothing at all is a legal declaration and stays one.
    expect(focus(OFFERED, { armed: 'a_attack', draft: {} }).ready).toBe(true)
  })

  it('takes back every answer without leaving the question', () => {
    const drafted: Interaction = {
      armed: 'a_attack',
      draft: { attackers: ['perm_bear'], defend_bear: ['p2'] },
      aiming: 'perm_bear',
    }
    const again = reset(drafted)
    expect(again.armed).toBe('a_attack')
    expect(again.draft).toEqual({})
    expect(again.aiming).toBeUndefined()
  })
})

describe('a slot about a subject the action does not ask you to choose', () => {
  // Blocker slots name their attacker too, and the shape is identical — but the attacker is a
  // board fact rather than something this action asks the player to pick, so the slot is asked
  // outright and answering it with nothing is a legal declaration.
  const BLOCK: ValidAction = {
    id: 'a_block',
    type: 'declare_blockers',
    label: 'Declare blockers',
    requirements: [
      {
        slot: 'block_bear',
        prompt: 'Choose blockers for Bear',
        subject: 'perm_bear',
        candidates: ['perm_wall'],
      },
    ],
  }

  it('is asked immediately, and is not owed an answer', () => {
    const slots = slotsOf(BLOCK, {})
    expect(slots.map((slot) => slot.slot)).toEqual(['block_bear'])
    expect(slots[0]?.conditional).toBe(false)
    expect(focus([BLOCK], { armed: 'a_block', draft: {} }).ready).toBe(true)
  })
})

describe('saying what you are playing before you can pay for it', () => {
  const MANA: ValidAction = {
    id: 'a_tap',
    type: 'activate_ability',
    label: '{T}: Add {G}.',
    subject: ['perm_forest'],
    mana_ability: true,
  }
  const PASS: ValidAction = { id: 'a_pass', type: 'pass_priority', label: 'Pass' }
  const OFFERED = [MANA, PASS]

  it('lets a card the server offered nothing for say “this is what I am playing”', () => {
    expect(gestureFor(OFFERED, IDLE, 'c_bear', new Set(['c_bear']))).toEqual({ kind: 'pay' })
  })

  it('never overrides an action the server did offer', () => {
    // Rule 3 still comes first: a card that can be cast is cast, not queued behind an intent.
    expect(gestureFor(OFFERED, IDLE, 'perm_forest', new Set(['perm_forest']))).toEqual({
      kind: 'take',
      action: 'a_tap',
    })
  })

  it('lights every source the server offered, and the card being paid for', () => {
    const paying = payFor(IDLE, 'c_bear')
    expect(paying.paying).toBe('c_bear')
    expect(highlightFor(OFFERED, paying, 'c_bear')).toBe('selected')
    expect(highlightFor(OFFERED, paying, 'perm_forest')).toBe('candidate')
    // Nothing else is lit: the client is not saying which sources would finish the cost.
    expect(highlightFor(OFFERED, paying, 'perm_other')).toBe('idle')
  })

  it('carries the intent across the views the payment itself produces', () => {
    // Making mana is one message per source, so an intent that did not survive a view would be
    // gone before the second land was tapped.
    const paying = payFor(IDLE, 'c_bear')
    expect(settle(paying, undefined).paying).toBe('c_bear')
    const sent = submitted(paying, { submission: 's:1', actionId: 'a_tap', label: 'tap' })
    expect(settle(sent, { submission: 's:1', accepted: true }).paying).toBe('c_bear')
  })

  it('knows which submission is the cast the intent was for', () => {
    const paying = payFor(IDLE, 'c_bear')
    const cast: ValidAction = {
      id: 'a_cast',
      type: 'cast_spell',
      label: 'Cast Grizzly Bears',
      subject: ['c_bear'],
    }
    expect(finishesPayment(paying, cast)).toBe(true)
    expect(finishesPayment(paying, MANA)).toBe(false)
  })

  it('gives up the intent without undoing anything, because nothing was sent for it', () => {
    expect(stopPaying(payFor(IDLE, 'c_bear')).paying).toBeUndefined()
  })
})

describe('assembling a payment', () => {
  /** Four Plains, `{1}{W}` in hand: the cast the server offers before the mana exists. */
  const CAST: ValidAction = {
    id: 'cast',
    type: 'cast_spell',
    label: "Cast Ajani's Pridemate",
    subject: ['card_1'],
    token: 't',
    prompts: [
      {
        kind: 'pay_mana',
        slot: 'pay_0',
        prompt: 'Pay {W}',
        pip: '{W}',
        candidates: [
          { id: 'perm_1#0', source: 'perm_1', label: '{W}' },
          { id: 'perm_2#0', source: 'perm_2', label: '{W}' },
        ],
      },
      {
        kind: 'pay_mana',
        slot: 'pay_1',
        prompt: 'Pay {1}',
        pip: '{1}',
        candidates: [
          { id: 'perm_1#0', source: 'perm_1', label: '{W}' },
          { id: 'perm_2#0', source: 'perm_2', label: '{W}' },
        ],
      },
    ],
  }

  /** A dual land that pays the coloured pip two ways. */
  const DUAL: ValidAction = {
    ...CAST,
    id: 'dual',
    prompts: [
      {
        kind: 'pay_mana',
        slot: 'pay_0',
        prompt: 'Pay {W}',
        pip: '{W}',
        candidates: [
          { id: 'perm_9#1', source: 'perm_9', label: '{W}' },
          { id: 'perm_9#2', source: 'perm_9', label: '{U}' },
        ],
      },
    ],
  }

  it('shows the whole cost before anything is chosen, and one pip less per source', () => {
    let interaction = arm(IDLE, CAST)
    expect(remainingCost(slotsOf(CAST, interaction.draft))).toEqual(['{1}', '{W}'])

    // A click on a Plains pays the coloured pip — the server put it first for exactly this.
    const first = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, first[0]!, 'perm_1', first)
    expect(remainingCost(slotsOf(CAST, interaction.draft))).toEqual(['{1}'])

    const second = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, second[1]!, 'perm_2', second)
    expect(remainingCost(slotsOf(CAST, interaction.draft))).toEqual([])
  })

  it('will not send until every pip is paid, and will the moment they are', () => {
    let interaction = arm(IDLE, CAST)
    expect(focus([CAST], interaction).ready).toBe(false)

    for (const id of ['perm_1', 'perm_2']) {
      const slots = slotsOf(CAST, interaction.draft)
      const gesture = gestureFor([CAST], interaction, id)
      expect(gesture.kind).toBe('fill')
      const slot = slots.find((each) => each.slot === (gesture as { slot: string }).slot)!
      interaction = fill(interaction, slot, id, slots)
    }
    expect(focus([CAST], interaction).ready).toBe(true)
  })

  it('takes a source back out, and the pip comes back', () => {
    let interaction = arm(IDLE, CAST)
    const slots = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_1', slots)
    expect(remainingCost(slotsOf(CAST, interaction.draft))).toEqual(['{1}'])

    // Clicking the same land again is the way back out — and the click lands on the slot that
    // is holding it, not on the next empty one.
    const held = slotsOf(CAST, interaction.draft)
    const gesture = gestureFor([CAST], interaction, 'perm_1')
    expect(gesture).toEqual({ kind: 'fill', slot: 'pay_0' })
    interaction = fill(interaction, held[0]!, 'perm_1', held)
    expect(remainingCost(slotsOf(CAST, interaction.draft))).toEqual(['{1}', '{W}'])
    expect(focus([CAST], interaction).ready).toBe(false)
  })

  it('spends one permanent once — a land already paying a pip is not offered for another', () => {
    let interaction = arm(IDLE, CAST)
    const slots = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_1', slots)

    // `perm_1` is a listed candidate for the generic pip too, but it is spent. The click goes
    // back to the pip holding it rather than double-spending the land.
    expect(gestureFor([CAST], interaction, 'perm_1')).toEqual({ kind: 'fill', slot: 'pay_0' })
    expect([...spentSources(slotsOf(CAST, interaction.draft))]).toEqual(['perm_1'])
  })

  it('asks which way a dual land taps, and does not guess', () => {
    let interaction = arm(IDLE, DUAL)
    const slots = slotsOf(DUAL, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_9', slots)

    // Nothing was chosen — the question is open, and it carries the server's own labels.
    expect(interaction.draft['pay_0'] ?? []).toEqual([])
    expect(interaction.asking).toEqual({ slot: 'pay_0', source: 'perm_9' })
    expect(waysToPay(slots[0]!, 'perm_9').map((way) => way.label)).toEqual(['{W}', '{U}'])

    interaction = answer(interaction, 'pay_0', ['perm_9#2'])
    expect(interaction.draft['pay_0']).toEqual(['perm_9#2'])
    expect(interaction.asking).toBeUndefined()
    expect(focus([DUAL], interaction).ready).toBe(true)
  })

  it('does not ask when the permanent pays the pip only one way', () => {
    let interaction = arm(IDLE, CAST)
    const slots = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_1', slots)
    expect(interaction.asking).toBeUndefined()
    expect(interaction.draft['pay_0']).toEqual(['perm_1#0'])
  })

  it('cancels the whole payment without having sent anything', () => {
    let interaction = arm(IDLE, CAST)
    const slots = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_1', slots)
    expect(interaction.pending).toBeUndefined()

    const cancelled = disarm(interaction)
    expect(cancelled.armed).toBeUndefined()
    expect(cancelled.draft).toEqual({})
    expect(cancelled.pending).toBeUndefined()
  })

  it('sends the activations it chose, keyed by slot', () => {
    let interaction = arm(IDLE, CAST)
    for (const [index, id] of ['perm_1', 'perm_2'].entries()) {
      const slots = slotsOf(CAST, interaction.draft)
      interaction = fill(interaction, slots[index]!, id, slots)
    }
    const message = buildChooseAction(CAST, interaction.draft)
    expect(message.targets).toEqual([
      { slot: 'pay_0', chosen: ['perm_1#0'] },
      { slot: 'pay_1', chosen: ['perm_2#0'] },
    ])
  })
})

describe('what a committed source looks like', () => {
  const CAST: ValidAction = {
    id: 'cast',
    type: 'cast_spell',
    label: 'Cast it',
    subject: ['card_1'],
    token: 't',
    prompts: [
      {
        kind: 'pay_mana',
        slot: 'pay_0',
        prompt: 'Pay {W}',
        pip: '{W}',
        candidates: [
          { id: 'perm_1#0', source: 'perm_1', label: '{W}' },
          { id: 'perm_2#0', source: 'perm_2', label: '{W}' },
        ],
      },
    ],
  }

  it('draws a land paying a pip as chosen, not as still available', () => {
    let interaction = arm(IDLE, CAST)
    expect(highlightFor([CAST], interaction, 'perm_1')).toBe('candidate')

    const slots = slotsOf(CAST, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_1', slots)

    // The draft holds `perm_1#0`; the board draws `perm_1`. Committed, and visibly so.
    expect(highlightFor([CAST], interaction, 'perm_1')).toBe('selected')
    // The other Plains stays live, because clicking it swaps which land pays the pip. A pip
    // that could only be un-picked and re-picked would make changing your mind two clicks.
    expect(highlightFor([CAST], interaction, 'perm_2')).toBe('candidate')

    const slots2 = slotsOf(CAST, interaction.draft)
    const swapped = fill(interaction, slots2[0]!, 'perm_2', slots2)
    expect(swapped.draft['pay_0']).toEqual(['perm_2#0'])
  })
})

describe('a cost that is not only mana', () => {
  /** Tormenting Voice: {1}{R}, and discard a card as an additional cost. */
  const VOICE: ValidAction = {
    id: 'voice',
    type: 'cast_spell',
    label: 'Cast Tormenting Voice',
    subject: ['card_1'],
    token: 't',
    prompts: [
      {
        kind: 'pay_mana',
        slot: 'pay_0',
        prompt: 'Pay {R}',
        pip: '{R}',
        candidates: [{ id: 'perm_1#0', source: 'perm_1', label: '{R}' }],
      },
      {
        kind: 'pay_mana',
        slot: 'pay_1',
        prompt: 'Pay {1}',
        pip: '{1}',
        candidates: [
          { id: 'perm_1#0', source: 'perm_1', label: '{R}' },
          { id: 'perm_2#0', source: 'perm_2', label: '{R}' },
        ],
      },
      {
        kind: 'select_from_zone',
        slot: 'cost_discard',
        prompt: 'Discard a card',
        zone: 'hand',
        owner: 'p0',
        count: 1,
        candidates: ['card_2', 'card_3'],
      },
    ],
  }

  it('will not cast until the discard is chosen as well as the mana', () => {
    let interaction = arm(IDLE, VOICE)
    for (const [index, id] of ['perm_1', 'perm_2'].entries()) {
      const slots = slotsOf(VOICE, interaction.draft)
      interaction = fill(interaction, slots[index]!, id, slots)
    }
    // Every pip is paid and the cost line is empty — and it still cannot be cast, because
    // the discard is part of the cost and has not been chosen.
    expect(remainingCost(slotsOf(VOICE, interaction.draft))).toEqual([])
    expect(focus([VOICE], interaction).ready).toBe(false)

    const slots = slotsOf(VOICE, interaction.draft)
    const discard = slots.find((slot) => slot.slot === 'cost_discard')!
    expect(discard.kind).toBe('zone')
    expect(gestureFor([VOICE], interaction, 'card_3')).toEqual({
      kind: 'fill',
      slot: 'cost_discard',
    })
    interaction = fill(interaction, discard, 'card_3', slots)
    expect(focus([VOICE], interaction).ready).toBe(true)
  })

  it('sends the mana and the discard in one message', () => {
    let interaction = arm(IDLE, VOICE)
    for (const [index, id] of ['perm_1', 'perm_2'].entries()) {
      const slots = slotsOf(VOICE, interaction.draft)
      interaction = fill(interaction, slots[index]!, id, slots)
    }
    const slots = slotsOf(VOICE, interaction.draft)
    interaction = fill(
      interaction,
      slots.find((s) => s.slot === 'cost_discard')!,
      'card_3',
      slots,
    )

    const message = buildChooseAction(VOICE, interaction.draft)
    expect(message.targets).toEqual([
      { slot: 'pay_0', chosen: ['perm_1#0'] },
      { slot: 'pay_1', chosen: ['perm_2#0'] },
      { slot: 'cost_discard', chosen: ['card_3'] },
    ])
  })

  it('takes the whole cost back at once, discard included', () => {
    let interaction = arm(IDLE, VOICE)
    const slots = slotsOf(VOICE, interaction.draft)
    interaction = fill(interaction, slots[0]!, 'perm_1', slots)
    interaction = fill(
      interaction,
      slots.find((s) => s.slot === 'cost_discard')!,
      'card_2',
      slots,
    )
    expect(interaction.draft['cost_discard']).toEqual(['card_2'])

    // Nothing was sent, so abandoning it costs a click and no message — and the card that
    // was going to be discarded is still just a card in hand.
    const cancelled = disarm(interaction)
    expect(cancelled.draft).toEqual({})
    expect(cancelled.pending).toBeUndefined()
  })
})
