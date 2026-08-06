import { describe, expect, it } from 'vitest'

import { claims, intentFor, type KeyPress } from './keys'

const press = (key: string, over: Partial<KeyPress> = {}): KeyPress => ({
  key,
  ctrl: false,
  meta: false,
  alt: false,
  shift: false,
  typing: false,
  onControl: false,
  ...over,
})

describe('what a keypress means', () => {
  it('carries the game on space', () => {
    expect(intentFor(press(' '))).toEqual({ kind: 'confirm' })
    expect(intentFor(press('F2'))).toEqual({ kind: 'confirm' })
  })

  it('takes space even from a focused control', () => {
    // A player who just clicked a card still has that card focused and still means "go on".
    expect(intentFor(press(' ', { onControl: true }))).toEqual({ kind: 'confirm' })
  })

  it('yields Enter to a focused control', () => {
    // Otherwise every button does two things at once: what it is, and what Enter means here.
    expect(intentFor(press('Enter'))).toEqual({ kind: 'confirm' })
    expect(intentFor(press('Enter', { onControl: true }))).toBeUndefined()
  })

  it('backs out on escape', () => {
    expect(intentFor(press('Escape'))).toEqual({ kind: 'cancel' })
  })

  it('binds the stop ladder from most stops to fewest', () => {
    expect(intentFor(press('F3'))).toEqual({ kind: 'stops', preset: 'everywhere' })
    expect(intentFor(press('F4'))).toEqual({ kind: 'stops', preset: 'mains' })
    expect(intentFor(press('F5'))).toEqual({ kind: 'stops', preset: 'nowhere' })
  })

  it('means nothing at all while the player is typing', () => {
    // The X of an X spell is typed into a number field, and a space in it is a space.
    expect(intentFor(press(' ', { typing: true }))).toBeUndefined()
    expect(intentFor(press('Escape', { typing: true }))).toBeUndefined()
  })

  it('leaves modified keys to the browser and the system', () => {
    expect(intentFor(press(' ', { ctrl: true }))).toBeUndefined()
    expect(intentFor(press('F5', { meta: true }))).toBeUndefined()
    expect(intentFor(press('Enter', { alt: true }))).toBeUndefined()
  })

  it('picks the numbered row a digit prints', () => {
    // The numeral on a mode row *is* its binding (§6.7), which is what makes a row a pointer
    // can press reachable from the keyboard (§6.5 rule 4).
    expect(intentFor(press('1'))).toEqual({ kind: 'pick', index: 1 })
    expect(intentFor(press('4'))).toEqual({ kind: 'pick', index: 4 })
    // Whether there is an nth row is decided where the view is, so a digit always means this.
    expect(intentFor(press('9'))).toEqual({ kind: 'pick', index: 9 })
    // Zero numbers no row: the rows count from one.
    expect(intentFor(press('0'))).toBeUndefined()
    expect(intentFor(press('1', { typing: true }))).toBeUndefined()
  })

  it('is nothing for a key nothing is bound to', () => {
    expect(intentFor(press('k'))).toBeUndefined()
  })
})

describe('what the page takes from the browser', () => {
  it('claims only the keys it is actually using', () => {
    // Space scrolls the page and activates a focused button; F5 reloads. Both have to be
    // suppressed where this client acts on them, and nowhere else.
    expect(claims(press(' '), { kind: 'confirm' })).toBe(true)
    expect(claims(press('F5'), { kind: 'stops', preset: 'nowhere' })).toBe(true)
  })

  it('claims nothing when nothing was bound', () => {
    // A page that blocks keys it does not use is a page a player cannot escape.
    expect(claims(press(' ', { typing: true }), undefined)).toBe(false)
    expect(claims(press('Escape'), { kind: 'cancel' })).toBe(false)
    // A digit activates no control and scrolls nothing, so there is nothing to suppress.
    expect(claims(press('2'), { kind: 'pick', index: 2 })).toBe(false)
  })
})
