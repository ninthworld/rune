/**
 * The same board, across the range of screens it has to work on.
 *
 * `docs/client-design.md` §1 and §3: **zoom, resolution and aspect are the same problem.** A
 * 200% zoom on a 1440-wide screen is a 720-wide viewport, so sweeping viewports sweeps zoom too,
 * and this is where the rules that must hold *at every* size are checked rather than at one.
 *
 * The two that matter most are the ones a player notices immediately:
 *
 * - **The page never scrolls**, in either axis, at any size.
 * - **No region of the board scrolls vertically or grows a scrollbar.** A full row pans sideways
 *   instead, at full card size — which is a horizontal overflow that is deliberate, and the only
 *   one on the board.
 *
 * Nothing here asserts a pixel size for a card. The card takes the height of the region it is
 * in, at any size, and §10 records "how small is too small" as an open question rather than a
 * threshold — so a test asserting one would be inventing the answer.
 */
import { expect, test } from '@playwright/test'

import { fixture, pageFits, serveFrames } from './frames'

/** The corners of the supported range, plus the shapes that broke earlier drafts. */
const SIZES = [
  { name: 'a wide desktop', width: 1920, height: 1080 },
  { name: 'a laptop', width: 1440, height: 900 },
  { name: 'a small laptop', width: 1180, height: 720 },
  { name: 'a square window', width: 900, height: 900 },
  { name: 'a tablet', width: 834, height: 1112 },
  { name: 'a phone', width: 390, height: 844 },
  { name: 'a very short window', width: 1280, height: 480 },
]

for (const size of SIZES) {
  test.describe(size.name, () => {
    test.use({ viewport: { width: size.width, height: size.height } })

    test('draws the whole board without the page scrolling', async ({ page }) => {
      await serveFrames(page, [fixture('gameview-board.json')])
      await page.goto('/')

      // Everything that must be on screen at every size: the turn, both halves, the hand, and
      // the bar that moves the game (§2, tier 1).
      await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()
      await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()
      await expect(page.getByRole('region', { name: 'Your battlefield' })).toBeVisible()
      await expect(page.getByRole('list', { name: 'Turn steps' }).first()).toBeVisible()

      expect(await pageFits(page)).toEqual({ x: true, y: true })
    })

    test('lets a full row pan sideways and nothing scroll down', async ({ page }) => {
      await serveFrames(page, [fixture('gameview-board.json')])
      await page.goto('/')
      await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()

      const overflow = await page.evaluate(() => {
        const board = document.querySelector('.layout')
        if (!board) return []
        return [...board.querySelectorAll<HTMLElement>('*')]
          .filter((el) => el.scrollHeight > el.clientHeight + 1)
          .map((el) => el.className)
      })
      // A board a player has to scroll is a board they cannot read. The one region allowed to
      // overflow is the side column's own lists, which are not the board.
      expect(overflow.filter((name) => !/panel-body|log|zone-body/.test(name))).toEqual([])
    })
  })
}

/**
 * The one run of text on the board whose length is a *player's* choice.
 *
 * Every other fitting question is about a card, and a card is drawn in its own grid at a size
 * the region hands it. A seat's name is typed at the front door, into a bar whose height is the
 * seat's, so the shortest window is where "fitted, never truncated" (§3) has to hold against
 * something the client did not choose. The fixture's own seats are called `Ada` and `Bo`, which
 * is exactly the length at which nothing is being tested.
 */
test.describe('a seat called what a player typed', () => {
  test.use({ viewport: { width: 1280, height: 480 } })

  test('sets a long name smaller rather than clipping it or scrolling the bar', async ({
    page,
  }) => {
    const view = fixture('gameview-board.json')
    await serveFrames(page, [
      { ...view, player_names: { p1: 'Katherine Johnson', p2: 'Bartholomew' } },
    ])
    await page.goto('/')
    await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()

    const seatName = page.locator('.player-bar[data-seat="p1"] .player-name')
    // Whole, and drawn inside its own box — the two halves of "not truncated".
    await expect(seatName).toHaveText('Katherine Johnson')
    expect(await seatName.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true)
    // Fitted between the stylesheet's size and the floor a name stops being one at.
    const size = await seatName.evaluate((el) => Number.parseFloat(getComputedStyle(el).fontSize))
    expect(size).toBeGreaterThanOrEqual(8)
    expect(size).toBeLessThanOrEqual(11)

    // And the bar it was fitted to is still a bar nobody has to scroll.
    const overflow = await page.evaluate(() => {
      const board = document.querySelector('.layout')
      if (!board) return []
      return [...board.querySelectorAll<HTMLElement>('*')]
        .filter((el) => el.scrollHeight > el.clientHeight + 1)
        .map((el) => el.className)
    })
    expect(overflow.filter((name) => !/panel-body|log|zone-body/.test(name))).toEqual([])
  })
})

/**
 * The one control that is not at every table.
 *
 * Undo (issue #648) adds a fifth button to the side column's helper strip, which is the only
 * place in the client where the *number* of controls depends on what the server said. A strip
 * that grew a row has to take that room from the panel under it — which is allowed to scroll its
 * own lists — and never from the board, at the sizes where there is least of it to take.
 */
for (const size of [
  { name: 'a phone', width: 390, height: 844 },
  { name: 'a very short window', width: 1280, height: 480 },
]) {
  test.describe(`an undo table on ${size.name}`, () => {
    test.use({ viewport: { width: size.width, height: size.height } })

    test('fits the extra control without the board scrolling', async ({ page }) => {
      const view = fixture('gameview-board.json') as Record<string, unknown>
      await serveFrames(page, [{ ...view, undo: { available: 4, limit: 20 } }])
      await page.goto('/')
      await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()

      expect(await pageFits(page)).toEqual({ x: true, y: true })
      const overflow = await page.evaluate(() => {
        const board = document.querySelector('.layout')
        if (!board) return []
        return [...board.querySelectorAll<HTMLElement>('*')]
          .filter((el) => el.scrollHeight > el.clientHeight + 1)
          .map((el) => el.className)
      })
      expect(overflow.filter((name) => !/panel-body|log|zone-body/.test(name))).toEqual([])
    })
  })
}
