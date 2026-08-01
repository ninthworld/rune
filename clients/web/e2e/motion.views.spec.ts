/**
 * What the table does when a message changes something: the life total that moved, and the object
 * that was not there before.
 *
 * Part of the non-blocking view tier (ADR 0011); the harness is `frames.ts`. Two frames rather
 * than one, because everything here is a *transition* — a pure function of the last two views
 * (`motion.ts`) — and a tier that only ever showed one frame could not see any of it.
 *
 * The rule every case is really checking is that none of it is load-bearing. A transition is
 * something a player watches; the board underneath it is the one the latest `GameView` describes,
 * whether the animation played, was interrupted, or was never allowed to run at all.
 */
import { expect, test, type Page } from '@playwright/test'

import { DESKTOP, fixture, serveFrames, serveSockets } from './frames'

test.use({ viewport: DESKTOP })

/**
 * The base view, with both seats' life totals set to whatever the case is about.
 *
 * `gameview.json` carries no `me` at all — a seat whose life the server did not state — so one
 * is supplied here in full. A partial `SelfView` would simply fail to parse and the page would
 * render nothing, which is the schema doing its job and not a case this file is about.
 */
const at = (mine: number, theirs: number) => {
  const base = fixture('gameview.json')
  const [opponent] = base.opponents as Record<string, unknown>[]
  return {
    ...base,
    me: { life: mine, library_size: 40 },
    opponents: [{ ...opponent, life: theirs }],
  }
}

const mySeat = (page: Page) => page.getByRole('region', { name: 'Your seat' })

test.describe('a life total that moved', () => {
  test('says how far it moved, and which way, in a sign and in words', async ({ page }) => {
    const served = await serveFrames(page, [at(20, 20)])
    await page.goto('/')
    await expect(mySeat(page)).toContainText('20')

    served.push(at(17, 22))

    // The total is what is played off; the delta is why it changed. Both are on the panel, and
    // the direction is a sign rather than only a colour.
    await expect(mySeat(page)).toContainText('17')
    await expect(mySeat(page).getByText('−3')).toBeVisible()
    await expect(mySeat(page).getByText('lost 3 life')).toBeAttached()

    const theirs = page.getByRole('region', { name: 'p2 seat' })
    await expect(theirs.getByText('+2')).toBeVisible()
    await expect(theirs.getByText('gained 2 life')).toBeAttached()
  })

  test('says nothing on the first view of a game', async ({ page }) => {
    // A player arriving at a board has not watched anything change, and a delta against nothing
    // would be the client inventing an event.
    await serveFrames(page, [at(20, 20)])
    await page.goto('/')

    await expect(mySeat(page)).toContainText('20')
    await expect(mySeat(page).locator('.seat__delta')).toHaveCount(0)
  })

  test('says nothing about a total that did not move', async ({ page }) => {
    const served = await serveFrames(page, [at(20, 20)])
    await page.goto('/')
    // Awaited before pushing, always: a frame sent before the page has opened its socket goes
    // nowhere, and the test would be asserting against the first view.
    await expect(mySeat(page)).toContainText('20')
    served.push({ ...at(20, 18), turn: 4 })

    await expect(page.getByRole('region', { name: 'p2 seat' }).getByText('−2')).toBeVisible()
    await expect(mySeat(page).locator('.seat__delta')).toHaveCount(0)
  })
})

test.describe('an object that was not there before', () => {
  const withThopter = () => {
    const base = fixture('gameview.json')
    return {
      ...base,
      battlefield: [
        ...(base.battlefield as unknown[]),
        {
          id: 'perm_new',
          controller: 'p1',
          owner: 'p1',
          card: { id: 'perm_new', name: 'Angel of the Dawn', type_line: 'Creature — Angel' },
        },
      ],
    }
  }

  test('arrives, and is a fully drawn card the moment the animation is over', async ({ page }) => {
    const served = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('region', { name: 'Your battlefield' })).toBeVisible()
    served.push(withThopter())

    const arrival = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Angel of the Dawn/ })
    await expect(arrival).toBeVisible()

    // The animation moves an element that is already in its final place, so what it leaves
    // behind is nothing at all: no running animation, no scale, no lingering opacity. And the
    // transform the board itself put on the card — centred in its slot — is untouched, because
    // an arrival that animated `transform` would replace it and slide every card sideways.
    await expect(async () => {
      const style = await arrival.evaluate((element) => {
        const computed = getComputedStyle(element)
        return {
          opacity: computed.opacity,
          scale: computed.scale,
          transform: computed.transform,
          running: element.getAnimations().length,
        }
      })
      expect(style).toMatchObject({ opacity: '1', scale: 'none', running: 0 })
      expect(style.transform).toBe('matrix(1, 0, 0, 1, -54, 0)')
    }).toPass()
  })

  test('reaches the same board for a device that asked for no motion', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' })
    const served = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('region', { name: 'Your battlefield' })).toBeVisible()
    served.push(withThopter())

    // The request is honoured by arriving at the same state at once, never by arriving
    // somewhere else — so the assertion is that the card is simply *there*.
    const arrival = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Angel of the Dawn/ })
    await expect(arrival).toBeVisible()
    expect(await arrival.evaluate((element) => element.getAnimations().length)).toBe(0)
  })
})

test.describe('a card that moved between two zones', () => {
  /**
   * The same game, one message later: the Llanowar Elves that was in hand is now a permanent.
   *
   * Two *objects* — `c1` and `perm_elves` — joined only by the physical card the server named
   * (CR 400.7: they are not the same object, and nothing here claims they are).
   */
  const cast = () => {
    const base = fixture('gameview.json')
    const hand = base.my_hand as Record<string, unknown>[]
    return {
      ...base,
      my_hand: hand.filter((card) => card.id !== 'c1'),
      battlefield: [
        ...(base.battlefield as unknown[]),
        {
          id: 'perm_elves',
          controller: 'p1',
          owner: 'p1',
          physical_card: 'c1',
          card: {
            id: 'perm_elves',
            name: 'Llanowar Elves',
            type_line: 'Creature — Elf Druid',
            card_types: ['creature'],
            power: '1',
            toughness: '1',
          },
        },
      ],
    }
  }

  const elves = (page: Page) =>
    page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Llanowar Elves/ })

  /**
   * Record what the page asks the browser to animate, keyed by the object it animated.
   *
   * The alternative is catching a 320ms animation mid-flight, which is a race dressed up as an
   * assertion. This reads the *decision* instead: a card that travelled is animated from an
   * offset, and one that merely appeared is faded in place, and the two are distinguishable
   * without waiting on a particular frame.
   */
  const recordAnimations = (page: Page) =>
    page.addInitScript(() => {
      const played: Record<string, unknown[]> = {}
      ;(window as unknown as { __motion: typeof played }).__motion = played
      const original = Element.prototype.animate
      Element.prototype.animate = function (keyframes, options) {
        const id = (this as HTMLElement).dataset?.entity
        if (id !== undefined) played[id] = keyframes as unknown[]
        return original.call(this, keyframes, options)
      }
    })

  const animationOf = (page: Page, id: string) =>
    page.evaluate(
      (entity) => (window as unknown as { __motion: Record<string, unknown[]> }).__motion[entity],
      id,
    )

  test('travels from the zone it was drawn in to the one it is drawn in now', async ({ page }) => {
    await recordAnimations(page)
    const served = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    // The card is in hand on the first frame, which is where the flight has to start from.
    await expect(page.getByRole('button', { name: /^Llanowar Elves/ }).first()).toBeVisible()

    served.push(cast())
    await expect(elves(page)).toBeVisible()

    // It moved: the first keyframe offsets the card from where it now sits, and the last puts
    // it exactly there. A card that had merely appeared would be a fade with no translation.
    const frames = (await animationOf(page, 'perm_elves')) as { translate?: string }[]
    expect(frames).toHaveLength(2)
    const [start, end] = frames
    expect(start?.translate).toMatch(/^-?[\d.]+px -?[\d.]+px$/)
    expect(start?.translate).not.toBe('0px 0px')
    expect(end?.translate).toBe('0px 0px')

    // And what it leaves behind is nothing: the board underneath is the one this view
    // describes, with no residual offset, scale, or opacity from the journey.
    await expect(async () => {
      const style = await elves(page).evaluate((element) => {
        const computed = getComputedStyle(element)
        return {
          opacity: computed.opacity,
          scale: computed.scale,
          translate: computed.translate,
          running: element.getAnimations().length,
        }
      })
      expect(style).toMatchObject({ opacity: '1', scale: 'none', translate: 'none', running: 0 })
    }).toPass()
  })

  test('lands on the latest view when a second message interrupts the flight', async ({ page }) => {
    const served = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('button', { name: /^Llanowar Elves/ }).first()).toBeVisible()

    // Two messages back to back, the second arriving while the first one's flight would still
    // be running. The board is the newest view's and the interrupted transition costs nothing.
    served.push(cast())
    served.push({ ...cast(), turn: 4 })

    await expect(elves(page)).toBeVisible()
    await expect(page.getByRole('heading', { name: /^Turn 4 — / })).toBeVisible()
  })

  test('reaches the same board for a device that asked for no motion', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await recordAnimations(page)
    const served = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('button', { name: /^Llanowar Elves/ }).first()).toBeVisible()

    served.push(cast())

    // The request is honoured by arriving at the same board at once: the card is simply there,
    // nothing was animated, and nothing is left running.
    await expect(elves(page)).toBeVisible()
    expect(await animationOf(page, 'perm_elves')).toBeUndefined()
    expect(await elves(page).evaluate((element) => element.getAnimations().length)).toBe(0)
  })
})

test.describe('a board that moved while the socket was down', () => {
  test('is arrived at rather than watched, so nothing is claimed to have just happened', async ({
    page,
  }) => {
    // Two sockets, and the second one answers with a board a whole turn further on — which is
    // what a real reconnect delivers, because the server sends the current state to whoever
    // reclaims the seat.
    const served = await serveSockets(page, [[at(20, 20)], [at(11, 20)]])
    await page.goto('/')
    await expect(mySeat(page)).toContainText('20')

    served.drop()
    await expect(mySeat(page)).toContainText('11')

    // Nine life is the arithmetic and not the event: the player watched none of it happen, and
    // a delta here would claim they did.
    await expect(mySeat(page).locator('.seat__delta')).toHaveCount(0)
  })
})
