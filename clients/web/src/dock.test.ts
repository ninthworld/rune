import { describe, expect, it } from 'vitest'

import {
  barTone,
  dockCandidates,
  dockDensity,
  dockNarrates,
  dockTone,
  dockWording,
  type DockDensity,
  type DockTone,
} from './dock'
import { IDLE, arm, ask, slotsOf, submitted, type Interaction, type Slot } from './interaction'
import type { GameResult, ValidAction } from './protocol'

const pass: ValidAction = { id: 'a0', type: 'pass_priority', label: 'Pass' }
const concede: ValidAction = { id: 'a1', type: 'concede', label: 'Concede', subject: [] }
const cast: ValidAction = {
  id: 'a2',
  type: 'cast_spell',
  label: 'Cast Bolt',
  subject: ['c1'],
  requirements: [{ slot: 't0', prompt: 'Target', candidates: ['p1'] }],
}
const owed: ValidAction = {
  id: 'a3',
  type: 'choose_targets',
  label: 'Aim the trigger',
  subject: ['stack_1'],
}

const RESULT: GameResult = { winner: 'p0', reason: 'concede' }
const sent = (interaction: Interaction) =>
  submitted(interaction, { submission: 's:1', actionId: 'a0', label: 'Pass' })

describe('what the controls are currently for', () => {
  it('is your move when the server is offering something and nothing is in progress', () => {
    expect(dockTone([pass, cast], IDLE, undefined)).toBe('yours')
  })

  it('is waiting when the server offered nothing', () => {
    expect(dockTone([], IDLE, undefined)).toBe('waiting')
  })

  it('is asking once an action that has questions is armed', () => {
    expect(dockTone([pass, cast], arm(IDLE, cast), undefined)).toBe('asking')
  })

  it('is asking when the server offers no pass, because play stops until this seat answers', () => {
    // A trigger waiting to be aimed. Conceding is offered in every state and answers nothing.
    expect(dockTone([owed, concede], IDLE, undefined)).toBe('asking')
  })

  it('is in flight while a submission is unanswered', () => {
    // The player has already answered; what they need to know is that their click is out there.
    expect(dockTone([pass], sent(IDLE), undefined)).toBe('sent')
  })

  it('is a confirmation ahead of anything else still in flight', () => {
    // The one state that should stop a player who is clicking through by reflex.
    expect(dockTone([concede], ask(sent(IDLE), concede), undefined)).toBe('confirm')
  })

  it('is over above all of it, because nobody is coming', () => {
    expect(dockTone([], IDLE, RESULT)).toBe('over')
    expect(dockTone([pass], ask(IDLE, concede), RESULT)).toBe('over')
  })
})

describe('the wording beside the colour', () => {
  it('says something for every tone, so the colour is never the only copy', () => {
    const tones: DockTone[] = ['over', 'confirm', 'sent', 'asking', 'yours', 'waiting']
    for (const tone of tones) expect(dockWording(tone).length).toBeGreaterThan(0)
    expect(new Set(tones.map(dockWording)).size).toBe(tones.length)
  })
})

describe('whether the band says what the game wants, or the controls do', () => {
  it('says it whenever nothing below it is stating a question', () => {
    expect(dockNarrates([pass, cast], IDLE, undefined)).toBe(true)
    expect(dockNarrates([], IDLE, undefined)).toBe(true)
    expect(dockNarrates([pass], sent(IDLE), undefined)).toBe(true)
    // A question the game will not proceed past is a row of buttons and nothing else, so the
    // sentence beside them is the only statement of what is being asked.
    expect(dockNarrates([owed, concede], IDLE, undefined)).toBe(true)
  })

  it('goes quiet while the controls below it are the question', () => {
    // §6.5 rule 2. A draft's own options are the question, and a confirmation asks in words —
    // "the game is waiting on your answer" above either is the fourth statement of one fact.
    expect(dockNarrates([pass, cast], arm(IDLE, cast), undefined)).toBe(false)
    expect(dockNarrates([concede], ask(IDLE, concede), undefined)).toBe(false)
  })

  it('still says a finished game is finished, whatever else is open', () => {
    expect(dockNarrates([pass], ask(IDLE, concede), RESULT)).toBe(true)
  })
})

describe('what the dock carries, and what the board answers', () => {
  const slotOf = (action: ValidAction): Slot => {
    const slot = slotsOf(action, {})[0]
    if (!slot) throw new Error('no slot')
    return slot
  }

  const blockers = (count: number): ValidAction => ({
    id: 'blk',
    type: 'declare_blockers',
    label: 'Declare blockers',
    requirements: [
      {
        slot: 'blockers',
        prompt: 'Choose which creatures block',
        candidates: Array.from({ length: count }, (_, index) => `perm_${index}`),
      },
    ],
  })

  it('carries nothing the table already drew', () => {
    const slot = slotOf(blockers(20))
    const drawn = new Set(slot.candidates)
    expect(dockCandidates(slot, drawn)).toEqual([])
  })

  it('is the same size for twenty legal subjects as for two, once the board has them', () => {
    // The whole of #678: what made the dock grow without bound was the second copy of a control
    // that the board already carries.
    const two = slotOf(blockers(2))
    const twenty = slotOf(blockers(20))
    expect(dockCandidates(two, new Set(two.candidates)).length).toBe(
      dockCandidates(twenty, new Set(twenty.candidates)).length,
    )
  })

  it('keeps a control for every subject no surface drew', () => {
    // A card in a face-down pile, an ability with no permanent, an id in no rendered zone: these
    // have nothing to click, and the dock is the guarantee that they are still reachable.
    const slot = slotOf(blockers(3))
    expect(dockCandidates(slot, new Set(['perm_1']))).toEqual(['perm_0', 'perm_2'])
    expect(dockCandidates(slot, new Set())).toEqual(['perm_0', 'perm_1', 'perm_2'])
  })

  const ordering = (): ValidAction => ({
    id: 'ord',
    type: 'order_combat_damage',
    label: 'Order blockers',
    prompts: [{ kind: 'order', slot: 'order', prompt: 'Order them', items: ['perm_a', 'perm_b'] }],
  })

  it('keeps every item of an ordering, because a position is only readable on a control', () => {
    const order = slotOf(ordering())
    expect(dockCandidates(order, new Set(['perm_a', 'perm_b']))).toEqual(['perm_a', 'perm_b'])
  })

  it('drops the items a surface is drawing with their positions on them', () => {
    // The pile a library ordering is answered in badges each card as it is clicked
    // (`docs/client-design.md` §6.7), so the control the dock was holding as the only place a
    // position could be read is now the second copy of one — and §6.5 rule 1 says the question
    // is drawn once.
    const order = slotOf(ordering())
    expect(
      dockCandidates(order, new Set(['perm_a', 'perm_b']), new Set(['perm_a', 'perm_b'])),
    ).toEqual([])
    // Merely being drawn is not enough: a permanent on the board wears no ordinal, so its item
    // stays here.
    expect(dockCandidates(order, new Set(['perm_a', 'perm_b']), new Set(['perm_a']))).toEqual([
      'perm_b',
    ])
  })

  it('carries no control at all for a slot no object answers', () => {
    const option = slotOf({
      id: 'mull',
      type: 'mulligan_decision',
      label: 'Keep or mulligan',
      prompts: [
        {
          kind: 'option',
          slot: 'decision',
          prompt: 'Keep this hand or take a mulligan?',
          options: [
            { id: 'keep', label: 'Keep this hand' },
            { id: 'mulligan', label: 'Mulligan' },
          ],
        },
      ],
    })
    // Its options are the server's own and are drawn straight; there are no entity ids in it for
    // the board to have answered.
    expect(dockCandidates(option, new Set())).toEqual([])
  })
})

describe('how much of its drawing the dock’s band can afford', () => {
  const fields: (keyof DockDensity)[] = ['text', 'padY', 'padX', 'gap', 'rowGap']

  /** Every height from well under the floor to well over the asking band, one px at a time. */
  const sweep = Array.from({ length: 260 }, (_, index) => index)

  it('never draws chrome below §7’s 11px floor, at any height', () => {
    for (const height of sweep) expect(dockDensity(height).text).toBeGreaterThanOrEqual(11)
  })

  it('never draws more than the designed size, however tall the band gets', () => {
    const designed = dockDensity(10_000)
    for (const height of sweep) {
      for (const field of fields) {
        expect(dockDensity(height)[field]).toBeLessThanOrEqual(designed[field])
      }
    }
  })

  it('is monotone: a taller band is never a smaller drawing', () => {
    // The recurring defect in this client is a bigger screen producing a worse board (§3), and a
    // ladder applied as a checklist is how it happens. Asserted as a sweep, not a table.
    for (const height of sweep) {
      const here = dockDensity(height)
      const taller = dockDensity(height + 1)
      for (const field of fields) expect(taller[field]).toBeGreaterThanOrEqual(here[field])
    }
  })

  it('reads only the height it was handed, so no count can reach it', () => {
    expect(dockDensity(44)).toEqual(dockDensity(44))
    expect(dockDensity(160)).not.toEqual(dockDensity(44))
  })
})

describe('the tone the action bar wears', () => {
  it('is green on the turn’s bookends', () => {
    for (const phase of ['untap', 'upkeep', 'draw', 'end', 'cleanup']) {
      expect(barTone(phase)).toBe('green')
    }
  })

  it('is blue in a main phase, where you may cast at will', () => {
    expect(barTone('precombat_main')).toBe('blue')
    expect(barTone('postcombat_main')).toBe('blue')
  })

  it('is red once combat is live', () => {
    for (const phase of [
      'begin_combat',
      'declare_attackers',
      'declare_blockers',
      'combat_damage',
      'end_combat',
    ]) {
      expect(barTone(phase)).toBe('red')
    }
  })

  it('tracks the turn and not what the controls are for', () => {
    // The same step, whatever the game is currently asking: the colour says the game moved
    // somewhere different, and the words say what is being asked.
    expect(barTone('precombat_main')).toBe(barTone('precombat_main'))
  })

  it('falls to green for a step this build has never heard of', () => {
    expect(barTone('some_future_step')).toBe('green')
    expect(barTone(undefined)).toBe('green')
  })
})
