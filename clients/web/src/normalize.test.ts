import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { isConnected, list, playerLabel, powerToughness, seatSummary } from './normalize'
import { GameView } from './protocol'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)
const view = (name: string) =>
  GameView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

describe('absence', () => {
  it('reads an absent collection and an empty one the same way', () => {
    // The server elides an empty list, but one can also arrive explicitly empty — a finished
    // game carries `valid_actions: []`. Both mean "nothing", and a screen that told them apart
    // would show a concluded game as still offering moves.
    expect(list(undefined)).toEqual([])
    expect(list([])).toEqual([])
    expect(list(view('gameview-over.json').valid_actions)).toEqual([])
  })

  it('reads an absent `connected` as connected', () => {
    // The flag rides the wire only when false, so absence is the common case. Reading it as
    // false would paint every healthy seat as dropped.
    expect(isConnected({})).toBe(true)
    expect(isConnected({ connected: false })).toBe(false)
    expect(isConnected({ connected: true })).toBe(true)
  })
})

describe('naming', () => {
  it('prefers the server-sent display name, keeping the id visible', () => {
    const v = view('gameview.json')
    const id = v.you ?? ''
    const label = playerLabel(v, id)
    expect(label).toContain(id)
  })

  it('falls back to the opaque id when the server named nobody', () => {
    expect(playerLabel({ phase: 'untap' }, 'p7')).toBe('p7')
  })
})

describe('displayed characteristics', () => {
  it('shows power and toughness together, or not at all', () => {
    expect(powerToughness({ power: '2', toughness: '2' })).toBe('2/2')
    expect(powerToughness({})).toBeUndefined()
    // A land has neither; a half-present pair is not a creature to display.
    expect(powerToughness({ power: '2' })).toBeUndefined()
    // Which stat a face leads with — P/T, loyalty, or nothing — is `card-face.ts`, because
    // the answer differs between the battlefield and everywhere else and a single string
    // helper could not tell them apart.
  })

  it('summarizes a seat without inventing anything', () => {
    const summary = seatSummary({ life: 18, library_size: 33, hand_size: 5, graveyard_size: 2 })
    expect(summary).toContain('18 life')
    expect(summary).toContain('5 hand')
    expect(summary).not.toContain('disconnected')
  })

  it('names the states that matter', () => {
    const summary = seatSummary({
      life: 0,
      library_size: 0,
      eliminated: true,
      connected: false,
      ai: true,
    })
    expect(summary).toContain('eliminated')
    expect(summary).toContain('disconnected')
    expect(summary).toContain('AI')
  })
})
