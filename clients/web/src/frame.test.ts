import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { classifyFrame, decodeFrame } from './frame'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)
const fixture = (name: string): unknown => JSON.parse(readFileSync(join(FIXTURES, name), 'utf8'))

describe('frame classification', () => {
  it('reads a game view by its `phase` and `you`', () => {
    const frame = classifyFrame(fixture('gameview.json'))
    expect(frame.kind).toBe('game')
  })

  it('reads a spectator view as one, not a game view', () => {
    // Both carry `phase`; only `you` separates them, and a seated view always serializes it.
    // Getting this backwards would hand a spectator a screen that expects a hand and actions.
    const frame = classifyFrame(fixture('spectatorview.json'))
    expect(frame.kind).toBe('spectator')
  })

  it('reads a lobby view, which carries no `phase`', () => {
    expect(classifyFrame(fixture('lobbyview.json')).kind).toBe('lobby')
  })

  it('reads a catalog by `catalog_version`, not as a lobby view', () => {
    // A catalog also lacks `phase`, so without its own discriminator it would be mistaken for
    // a lobby view and wipe the real one off the screen.
    expect(classifyFrame(fixture('catalogview.json')).kind).toBe('catalog')
  })

  it('reads a lobby error by its `lobby_error` key', () => {
    const frame = classifyFrame({
      lobby_error: { code: 'copy_limit', reason: 'too many copies', card: 'onakke_ogre' },
    })
    expect(frame.kind).toBe('lobby_error')
    if (frame.kind === 'lobby_error') {
      expect(frame.frame.lobby_error.code).toBe('copy_limit')
    }
  })

  it('prefers `lobby_error` over every other discriminator', () => {
    // The error rides alongside an otherwise unchanged lobby view; the key is carried by no
    // other frame, so it is checked first.
    const frame = classifyFrame({
      lobby_error: { code: 'deck_size', reason: 'too few cards' },
      phase: 'untap',
      you: 'p0',
    })
    expect(frame.kind).toBe('lobby_error')
  })
})

describe('surviving what this client cannot read', () => {
  it('reports an unparseable payload rather than throwing', () => {
    const frame = decodeFrame('{not json')
    expect(frame.kind).toBe('unknown')
  })

  it('reports a non-object payload rather than throwing', () => {
    expect(decodeFrame('[1,2,3]').kind).toBe('unknown')
    expect(decodeFrame('"a string"').kind).toBe('unknown')
  })

  it('reports a frame that classifies but fails validation', () => {
    // `phase` and `you` say game view, but the phase is not one this client knows. Better to
    // say so than to render a half-parsed board.
    const frame = classifyFrame({ phase: 'interstitial', you: 'p0' })
    expect(frame.kind).toBe('unknown')
  })

  it('accepts a frame carrying fields this client does not know', () => {
    const frame = classifyFrame({ ...(fixture('gameview.json') as object), future_field: 1 })
    expect(frame.kind).toBe('game')
  })
})
