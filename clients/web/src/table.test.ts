/**
 * The seat join, against the committed fixtures.
 *
 * What is worth testing here is not that fields copy across — it is the asymmetry. Your own
 * projection and an opponent's are deliberately different shapes because one of them is
 * redacted, and the join has to keep that difference rather than average it into a single seat
 * that claims to know your opponent's hand or forgets that it knows yours.
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { GameView } from './protocol'
import { phaseLabel, seats } from './table'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const view = (name: string): GameView =>
  GameView.parse(JSON.parse(readFileSync(join(FIXTURES, name), 'utf8')))

const seat = (fixture: string, id: string) => {
  const found = seats(view(fixture)).find((s) => s.id === id)
  if (!found) throw new Error(`${fixture} seated nobody as ${id}`)
  return found
}

describe('seating the table', () => {
  it('follows the order the server stated', () => {
    expect(seats(view('gameview-commander.json')).map((s) => s.id)).toEqual(['p0', 'p1', 'p2'])
  })

  it('still seats everyone when the view carried no order', () => {
    // `gameview.json` has no `seat_order`. The seats are still known — you, then the opponents
    // the server listed — and a table with nobody at it would be worse than a stated order.
    const table = seats(view('gameview.json'))
    expect(table.map((s) => s.id)).toEqual(['p1', 'p2'])
    expect(table.filter((s) => s.isYou).map((s) => s.id)).toEqual(['p1'])
  })

  it('keeps your projection and an opponent’s as the different shapes they are', () => {
    const mine = seat('gameview-commander.json', 'p0')
    const theirs = seat('gameview-commander.json', 'p1')

    // An opponent's hand is a count. Yours is `my_hand`, and is never counted into the seat.
    expect(theirs.handSize).toBe(5)
    expect(mine.handSize).toBeUndefined()

    // Both project life and library; neither is invented for the other.
    expect(mine.life).toBe(0)
    expect(theirs.life).toBe(34)
    expect(mine.librarySize).toBe(31)
  })

  it('reads an absent `connected` as connected and an explicit false as not', () => {
    // The flag rides the wire only when false, so absence is the common case; reading it the
    // other way would paint every healthy seat as dropped.
    expect(seat('gameview-commander.json', 'p1').connected).toBe(false)
    expect(seat('gameview-commander.json', 'p2').connected).toBe(true)
    expect(seat('gameview-commander.json', 'p0').connected).toBe(true)
  })

  it('carries the states that change how a seat reads', () => {
    expect(seat('gameview-commander.json', 'p0').eliminated).toBe(true)
    expect(seat('gameview-commander.json', 'p2').ai).toBe(true)
    expect(seat('gameview.json', 'p2').statuses).toEqual(['monarch'])
  })

  it('gives the mana pool to you and to nobody else', () => {
    // The server sends no one else's floating mana, so no other seat can have one to show.
    expect(seat('gameview.json', 'p1').manaPool).toEqual(['{G}', '{G}'])
    expect(seat('gameview.json', 'p2').manaPool).toEqual([])
  })

  it('puts each public pile in front of the seat that owns it', () => {
    // In this fixture exile is yours and the graveyard is the opponent's — a client that
    // pooled them would show your opponent holding your exiled card.
    const mine = seat('gameview.json', 'p1')
    const theirs = seat('gameview.json', 'p2')

    expect(mine.piles.map((p) => p.zone)).toEqual(['exile'])
    expect(mine.piles[0]!.faces.map((f) => f.name)).toEqual(['Path to Exile'])
    expect(theirs.piles.map((p) => p.zone)).toEqual(['graveyard'])
  })

  it('omits an empty pile rather than showing an empty drawer', () => {
    expect(seat('gameview-emblem.json', 'p0').piles).toEqual([])
  })

  it('counts an itemized graveyard from the pile, not from the summary beside it', () => {
    // This seat's `graveyard_size` is 0 while its pile carries a card. A graveyard is public,
    // so the itemized pile is what a player can see and count — trusting the summary would
    // print `0 graveyard` directly above the card sitting in it.
    expect(view('gameview-commander.json').opponents?.[1]?.graveyard_size).toBe(0)
    expect(seat('gameview-commander.json', 'p2').graveyardSize).toBe(1)

    // With no pile sent, the summary is all there is, and it is used.
    expect(seat('gameview-commander.json', 'p1').graveyardSize).toBe(3)
    const you = seats(view('gameview.json')).find((s) => s.isYou)!
    expect(you.graveyardSize).toBeUndefined()
  })

  it('carries commander identity, tax, and damage to the seats they belong to', () => {
    const damaged = seat('gameview-commander.json', 'p0')
    const dealer = seat('gameview-commander.json', 'p1')

    expect(dealer.commanderName).toBe('Jedit Ojanen')
    expect(dealer.commanderCasts).toBe(1)
    expect(dealer.commanderTax).toBe(2)
    // Damage is filed against the seat that took it, named by who dealt it — it kills at 21
    // per source (CR 903.10a), so it can never be summed into one number.
    expect(damaged.commanderDamage).toEqual([{ from: 'p1', amount: 21 }])
    expect(dealer.commanderDamage).toEqual([])
  })

  it('leaves commander fields absent outside a commander game', () => {
    const you = seat('gameview.json', 'p1')
    expect(you.commanderName).toBeUndefined()
    expect(you.commanderTax).toBeUndefined()
    expect(you.commanderDamage).toEqual([])
  })
})

describe('naming a step', () => {
  it('spells out the steps this build knows', () => {
    expect(phaseLabel('precombat_main')).toBe('Precombat main')
    expect(phaseLabel('declare_blockers')).toBe('Declare blockers')
  })

  it('renders an unknown classifier as sent rather than guessing at it', () => {
    expect(phaseLabel('interstitial')).toBe('interstitial')
  })
})
