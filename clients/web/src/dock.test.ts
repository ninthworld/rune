import { describe, expect, it } from 'vitest'

import { dockTone, dockWording, type DockTone } from './dock'
import { IDLE, arm, ask, submitted, type Interaction } from './interaction'
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
