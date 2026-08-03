/**
 * The card, in a real browser (`docs/client-design.md` §6).
 *
 * **One drawing, one presentation, at every size.** A card in the hand, a permanent on the
 * battlefield and the preview beside the board are the same SVG scaled by the region it is in,
 * so what this tier checks is that the drawing carries everything the server stated and invents
 * nothing — and that the same object drawn in two places says the same thing.
 *
 * The fitting is deliberately not asserted in pixels. Type is set in the card's own 207×291 grid
 * and bisected against a measured box, so a threshold here would be a test of Chromium's text
 * metrics; what matters is that nothing is dropped and nothing overflows its box.
 */
import { expect, test } from '@playwright/test'

import { DESKTOP, fixture, open, openSide, serveFrames } from './frames'

test.use({ viewport: DESKTOP })

test.describe('the card', () => {
  test('carries the name, the cost, the type line, the text and the stat', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // The accessible name is built from the same face the frame draws (`card-face.ts`), so what
    // a screen reader hears and what the board shows cannot disagree.
    const bolt = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })
    await expect(bolt).toHaveAttribute('aria-label', /\{R\}/)
    await expect(bolt).toHaveAttribute('aria-label', /Instant/)
    // And the rules text is the server's sentence, drawn rather than summarised.
    await expect(bolt).toContainText('deals 3 damage to any target')

    const elves = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Llanowar Elves/ })
    await expect(elves).toHaveAttribute('aria-label', /Power\/toughness 1\/1/)
    // `{T}` in a sentence is a pip, not four characters of prose.
    await expect(elves.getByRole('img', { name: 'tap' })).toBeVisible()
  })

  test('draws what is true of the permanent over the card it is printed on', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bears = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Grizzly Bears/ })

    // Counters, marked damage and tap state are three separate facts, each said in words as
    // well as drawn: none of them is available only as a colour or a rotation.
    await expect(bears).toContainText('+1/+1')
    await expect(bears).toContainText('damage')
    await expect(bears).toHaveAttribute('aria-label', /tapped/)
    // A permanent that is turned is turned bodily, so its box is wider than it is in hand.
    const box = await bears.boundingBox()
    expect(box?.width ?? 0).toBeGreaterThan(box?.height ?? 0)
  })

  test('is the same card wherever it is drawn', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await openSide(page)

    const inHand = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })
    const name = await inHand.getAttribute('aria-label')

    // The pointer previews; reading costs no click at all (§6.6). What the column draws is the
    // same face, at the size that region affords.
    await inHand.hover()
    const preview = page.locator('.preview-section .card')
    await expect(preview).toBeVisible()
    await expect(preview).toHaveAttribute('aria-label', name ?? '')

    const small = await inHand.boundingBox()
    const large = await preview.boundingBox()
    expect(large?.width ?? 0).toBeGreaterThan((small?.width ?? 0) * 0.5)
  })

  test('draws a printed card and a catalog card the same way', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')
    await page.getByRole('button', { name: 'Edit' }).click()

    // The builder's preview is the board's card: one drawing, from one `CardFace`, whether the
    // object came from a `CardView` in a game or a `CatalogCard` before one.
    const preview = page.getByRole('dialog', { name: 'Deck' }).locator('.edit-preview .card')
    await expect(preview).toBeVisible()
    await expect(preview).toHaveAttribute('aria-label', /·/)
  })

  test('keeps every run of text inside its own box', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')

    // Nothing is truncated and nothing overflows: each run is fitted to the box it was given,
    // in the card's own grid, so a long name and a long paragraph both fit rather than clip.
    const overflowing = await page.evaluate(
      () =>
        [...document.querySelectorAll('.c-name, .c-typeline, .c-text, .c-pt-num')].filter(
          (el) => el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1,
        ).length,
    )
    expect(overflowing).toBe(0)
  })
})
