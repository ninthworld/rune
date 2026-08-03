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
