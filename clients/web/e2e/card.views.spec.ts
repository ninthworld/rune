/**
 * The card, drawn — the one atom the hand, both battlefields, the stack, opened piles, and the
 * deck builder all render.
 *
 * What this tier adds over `fit.test.ts` is the half that arithmetic cannot answer: whether the
 * browser agrees. `fit.ts` estimates how wide a string draws and decides what to put on a card
 * from that estimate; only a real engine, with the real font, at the real size, can say whether
 * what it chose actually fits. So the assertions here are mostly about *overflow* — nothing
 * clipped, nothing under the type floor, no box drawn around nothing — and they hold at both
 * ends of the supported range (`docs/client-design.md` §1) rather than at one comfortable size.
 */
import { expect, test, type Page } from '@playwright/test'

import { DESKTOP, fixture, serveFrames } from './frames'

/** The Short band of §4 — phone landscape, and a 1280×720 desktop at 200% zoom. */
const SHORT = { width: 640, height: 360 }

/** Everything on a card that is drawn as text, and therefore everything that could be cut. */
const TEXT = '.card__name, .card__type, .card__rules, .card__keywords, .card__stat, .card .badge'

/**
 * Every drawn string that runs past the box it is in.
 *
 * `scrollWidth` over `clientWidth` is the question "was this cut", asked of the browser rather
 * than of the estimate that decided to draw it. One pixel of slack, because a fractional layout
 * rounds and a card is not wrong by a rounding.
 */
const clipped = (page: Page) =>
  page.evaluate((selector) => {
    const over: string[] = []
    for (const element of document.querySelectorAll<HTMLElement>(selector)) {
      if (element.scrollWidth > element.clientWidth + 1) {
        over.push(`${element.className}: ${element.textContent?.slice(0, 40)}`)
      }
    }
    return over
  }, TEXT)

/** Every drawn string below the 9px floor §7 sets for text on a card. */
const belowFloor = (page: Page) =>
  page.evaluate((selector) => {
    const small: string[] = []
    for (const element of document.querySelectorAll<HTMLElement>(selector)) {
      const size = Number.parseFloat(getComputedStyle(element).fontSize)
      if (size < 9) small.push(`${element.className}: ${size}px`)
    }
    return small
  }, TEXT)

test.describe('a card says what it is', () => {
  test('draws every name in the hand complete, at both ends of the range', async ({ page }) => {
    // The defect this replaces: a hand in which every card read `C…`, `Dis…`, `Sna…`. The hand
    // is where a player chooses, so §6 forbids abbreviating a name there at *any* supported
    // size — not only at the size the layout was designed against.
    for (const size of [DESKTOP, SHORT]) {
      await page.setViewportSize(size)
      await serveFrames(page, [fixture('gameview.json')])
      await page.goto('/')

      const hand = page.getByRole('region', { name: 'Your hand' })
      await expect(hand.getByRole('button', { name: /^Llanowar Elves/ })).toBeVisible()

      for (const name of ['Llanowar Elves', 'Forest', 'Lightning Bolt']) {
        // The whole name, on screen, in the card's own text — not only in a tooltip.
        await expect(hand.getByRole('button', { name: new RegExp(`^${name}`) })).toContainText(name)
      }
    }
  })

  test('cuts no drawn string, at any supported size', async ({ page }) => {
    for (const size of [DESKTOP, SHORT, { width: 960, height: 600 }]) {
      await page.setViewportSize(size)
      await serveFrames(page, [fixture('gameview.json')])
      await page.goto('/')
      await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()

      expect(await clipped(page)).toEqual([])
      expect(await belowFloor(page)).toEqual([])
    }
  })

  test('degrades a type line to a whole word rather than to an ellipsis', async ({ page }) => {
    // A phone in portrait rather than the Short band: at Short every tile on the board and in
    // the hand is a chip, and a chip has no type line to degrade (§6). This is the smallest box
    // that still draws one — the 72×100 tile — which is where the degrading is worth asserting.
    await page.setViewportSize({ width: 390, height: 844 })
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()

    // `Legendary Creature — Elf Druid` becomes `Legendary Creature`, then `Creature`, and never
    // `Legendary Cr…`. Whichever rung a given box lands on, no drawn type line is ever cut.
    const drawn = await page
      .locator('.card__type')
      .evaluateAll((nodes) => nodes.map((node) => node.textContent ?? ''))
    expect(drawn.length).toBeGreaterThan(0)
    for (const line of drawn) {
      expect(line).not.toContain('…')
      expect(line).not.toMatch(/\.\.\.$/)
    }
  })

  test('never draws a box around nothing', async ({ page }) => {
    // A hand card used to render a blank black band where the rules text did not fit — a
    // container outliving its content. The box is drawn only when what goes in it fits whole.
    for (const size of [DESKTOP, SHORT]) {
      await page.setViewportSize(size)
      await serveFrames(page, [fixture('gameview.json')])
      await page.goto('/')
      await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()

      const empty = await page
        .locator('.card__text')
        .evaluateAll((nodes) => nodes.filter((node) => !node.textContent?.trim()).length)
      expect(empty).toBe(0)
    }
  })

  test('draws the cost in the name band, not over the art', async ({ page }) => {
    // §6: the cost belongs at the band's trailing edge, where a printed card puts it and where a
    // player's eye already goes. It spent a while over the art's corner, which is neither — and
    // where it tinted against whatever was behind it.
    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bolt = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })
    await expect(bolt.locator('.card__band .card__cost')).toHaveCount(1)
    await expect(bolt.locator('.card__window .card__cost')).toHaveCount(0)

    // Beside the name rather than under it or over it: same row, and after it.
    const placed = await bolt.evaluate((card: HTMLElement) => {
      const name = card.querySelector('.card__name')!.getBoundingClientRect()
      const cost = card.querySelector('.card__cost')!.getBoundingClientRect()
      return { after: cost.left >= name.right - 1, sameRow: cost.top < name.bottom }
    })
    expect(placed).toEqual({ after: true, sameRow: true })
  })

  test('fits a complete name, a type line, and a P/T into a 72×100 tile', async ({ page }) => {
    // The §5 minimum tile — the box XMage fits all five of these into, and the bar the whole
    // redraw is measured against. It is reached where the *room* runs out rather than wherever a
    // viewport-derived token happened to bottom out, so it takes a crowded board as well as a
    // small screen: the dense fixture on a phone in portrait, seven permanents in a 382px row.
    // Three permanents at the same size draw a 124px tile, and that is the packing working
    // (`pack.ts`) rather than a different minimum.
    await page.setViewportSize({ width: 390, height: 844 })
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')

    const digger = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Gravedigger/ })

    // The laid-out box, not the painted one: an object that has just arrived is animated in
    // from a scale of its own, and its painted rectangle during that quarter second is not the
    // box the fitting was planned against.
    const box = await digger.evaluate((node: HTMLElement) => ({
      width: node.offsetWidth,
      height: node.offsetHeight,
    }))
    expect(box.width).toBeLessThanOrEqual(80)
    expect(box.height).toBeLessThanOrEqual(115)

    await expect(digger).toContainText('Gravedigger')
    await expect(digger.locator('.card__type')).toHaveText(/Creature/)
    await expect(digger.locator('.card__stat')).toContainText('2/2')
  })

  test('keeps the whole name for assistive technology whatever it drew', async ({ page }) => {
    // An abbreviation is a drawing. A screen reader is told the card, not the stem — and the
    // same holds for a type line degraded by rule and for rules text the box could not hold.
    await page.setViewportSize(SHORT)
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const nissa = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /Nissa, Who Shakes the World/ })
    await expect(nissa).toBeVisible()

    // The complete rules text too, whether or not there was room to draw it.
    await expect(nissa).toHaveAccessibleName(/Whenever you tap a Forest for mana/)
  })

  test('says a tapped permanent is tapped without taking its name away', async ({ page }) => {
    // The whole of §6's tapped rule, at both ends of the supported range. The turn is what a
    // player reads a board by, and what it must not cost is the card's identity: the name is still
    // complete on the frame, the whole of it is still on the tile for a screen reader, and the
    // word is there for anyone who can perceive neither a turn nor a mark.
    for (const size of [DESKTOP, SHORT]) {
      await page.setViewportSize(size)
      await serveFrames(page, [fixture('gameview.json')])
      await page.goto('/')

      const bear = page
        .getByRole('region', { name: 'Your battlefield' })
        .getByRole('button', { name: /^Grizzly Bears/ })
      await expect(bear).toBeVisible()

      // Whole where there is room, and a recognisable stem of it where there is not — never a
      // fact traded away for the tap. The complete name is stated either way.
      const drawn = (await bear.locator('.card__name').innerText()).replace(/…$/, '')
      expect('Grizzly Bears'.startsWith(drawn)).toBe(true)
      expect(drawn.length).toBeGreaterThanOrEqual(10)
      await expect(bear).toHaveAccessibleName(/Grizzly Bears/)
      await expect(bear).toHaveAccessibleName(/Tapped/)

      // A quarter turn where the tile is a card, and a mark where it is a chip — a 96×30 chip is
      // already lying down, so rotating it would say nothing. Read off the matrix the browser
      // resolved rather than off a declaration, so it is the drawn state that is asserted.
      const drawnState = await bear.evaluate((card: HTMLElement) => ({
        chip: card.classList.contains('card--chip'),
        transform: getComputedStyle(card).transform,
        mark: getComputedStyle(card, '::after').opacity,
      }))
      if (drawnState.chip) {
        expect(drawnState.transform, `${size.width}×${size.height}`).toBe('none')
        expect(drawnState.mark).toBe('1')
      } else {
        // `rotate(90deg)` resolves to `matrix(0, 1, -1, 0, 0, 0)`.
        expect(drawnState.transform, `${size.width}×${size.height}`).toMatch(/^matrix\(0, 1, -1, 0/)
        expect(drawnState.mark).toBe('0')
      }
    }
  })

  test('is the same card at the same size whichever surface drew it', async ({ page }) => {
    // §6's rule, and the reason there is no variant to pass: presentation follows the box. A
    // card in an opened graveyard is drawn at the board's own width, so it gets the board's
    // presentation — nothing about being in a pile changes it.
    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const board = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Grizzly Bears/ })
    await expect(board).toHaveClass(/card--designed|card--compact/)

    // And the inspector, which is the one place everything is shown, is the full presentation.
    await board.click({ button: 'right' })
    await expect(page.getByRole('dialog').locator('.card')).toHaveClass(/card--full/)
  })
})
