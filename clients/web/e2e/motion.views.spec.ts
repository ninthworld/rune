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
