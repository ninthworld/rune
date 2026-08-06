/**
 * Announcing, ordering, and a card with two faces (`docs/client-design.md` §6.7), in a real
 * browser.
 *
 * Five surfaces the rules reached before the client did, and each one is a rule already in the
 * document applied to a question the engine did not used to ask. What this tier is for is the
 * half a unit test cannot reach: that the rows, the stepper, the badges and the mark are really
 * on screen, reachable by pointer *and* by keyboard, and that the bar they are in does not
 * change size when they appear.
 *
 * The views here are committed fixtures with a `valid_actions` list put on them, because no
 * fixture carries a modal cast yet. Everything else about them — the hand, the seats, the
 * phase — is the fixture's, so what is being driven is still a view the wire could produce.
 */
import { expect, test } from '@playwright/test'

import { DESKTOP, fixture, pageFits, serveFrames, submissions } from './frames'

test.use({ viewport: DESKTOP })

/** The prompts fixture's own seats and hand, with an action list of this test's choosing. */
const asking = (actions: readonly unknown[]) => ({
  ...fixture('gameview-prompts.json'),
  phase: 'precombat_main',
  valid_actions: actions,
})

const pass = { id: 'a_pass', type: 'pass_priority', label: 'Pass' }

/** A modal cast on the fixture's own Grizzly Bears, with two modes and one target slot each. */
const MODAL = {
  id: 'a_modal',
  type: 'cast_spell',
  label: 'Cast Sagelight Charm',
  subject: ['card_11'],
  token: 't',
  requirements: [
    { slot: 'm0t0', prompt: 'Choose target creature', optional: true, candidates: ['card_10'] },
    { slot: 'm1t0', prompt: 'Choose target player', optional: true, candidates: ['p1'] },
  ],
  prompts: [
    {
      kind: 'option',
      slot: 'mode',
      prompt: 'Choose one',
      options: [
        { id: 'mode_0', label: 'Destroy target creature.', requires: ['m0t0'] },
        { id: 'mode_1', label: 'Target player draws a card.', requires: ['m1t0'] },
      ],
    },
  ],
}

test.describe('a mode', () => {
  test('is a numbered row per mode, and the numeral is the key', async ({ page }) => {
    const socket = await serveFrames(page, [asking([pass, MODAL])])
    await page.goto('/')

    // One click on the card arms it, because the server offered exactly one action for it.
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()

    const rows = page.locator('.mode-row')
    await expect(rows).toHaveCount(2)
    // The mode's own generated sentence, whole — a player picks between the words the card
    // prints rather than between "Mode 1" and "Mode 2".
    await expect(rows.first()).toContainText('Destroy target creature.')
    await expect(rows.first()).toHaveAttribute('aria-keyshortcuts', '1')
    await expect(rows.nth(1)).toHaveAttribute('aria-keyshortcuts', '2')

    // The numeral *is* the binding, so everything the pointer reaches the keyboard reaches
    // (§6.5 rule 4).
    await page.keyboard.press('2')
    await expect(rows.nth(1)).toHaveAttribute('aria-pressed', 'true')
    await expect(rows.first()).toHaveAttribute('aria-pressed', 'false')

    // And a mode decides which target slots the spell has: the second mode's is the one now
    // being asked, and answering it is what the submission carries.
    await expect(page.getByRole('group', { name: 'Choose target player' })).toBeVisible()
    await expect(page.getByRole('group', { name: 'Choose target creature' })).toHaveCount(0)

    await page.getByRole('button', { name: 'Opponent' }).first().click()
    await page.getByRole('button', { name: 'Confirm' }).click()
    await expect.poll(() => submissions(socket.sent).length).toBe(1)
    expect(submissions(socket.sent)[0]?.targets).toEqual([
      { slot: 'mode', chosen: ['mode_1'] },
      { slot: 'm1t0', chosen: ['p1'] },
    ])
  })

  test('does not change the size of the band it is asked in', async ({ page }) => {
    // §6.5 rule 5: the dock's band is fixed and responds neither to how much there is to ask
    // about nor to whether anything is being asked at all. A band that grew for a modal cast
    // would move the board out from under the cards a player was reading.
    await serveFrames(page, [asking([pass, MODAL])])
    await page.goto('/')

    const bar = page.getByRole('region', { name: 'Actions' })
    const quiet = await bar.boundingBox()
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()
    await expect(page.locator('.mode-row')).toHaveCount(2)
    const busy = await bar.boundingBox()

    expect(busy?.height).toBe(quiet?.height)
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })
})

test.describe('three modes, in a short window', () => {
  // 200% zoom on a laptop, which is where the band has least room to give (§1). Three is
  // the bound the catalog enforces and the Charm cycle's own count — no *choose one* card
  // prints four (§6.7).
  test.use({ viewport: { width: 1280, height: 480 } })

  test('holds the bound the catalog enforces without growing or clipping', async ({ page }) => {
    const three = {
      ...MODAL,
      requirements: [],
      prompts: [
        {
          kind: 'option',
          slot: 'mode',
          prompt: 'Choose one',
          options: [
            { id: 'mode_0', label: 'Destroy target creature an opponent controls.' },
            { id: 'mode_1', label: 'Target player draws two cards and loses two life.' },
            { id: 'mode_2', label: 'Put a +1/+1 counter on each creature you control.' },
          ],
        },
      ],
    }
    await serveFrames(page, [asking([pass, three])])
    await page.goto('/')

    const bar = page.getByRole('region', { name: 'Actions' })
    const quiet = await bar.boundingBox()
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()
    await expect(page.locator('.mode-row')).toHaveCount(3)

    // Fixed band, three rows in it, and every sentence whole: the type gives way, never the row
    // and never the end of a mode's own words (§3, §7).
    expect((await bar.boundingBox())?.height).toBe(quiet?.height)
    const clipped = await page.evaluate(() => {
      const rows = [...document.querySelectorAll<HTMLElement>('.action-bar, .mode-row, .mode-text')]
      return rows
        .filter(
          (el) => el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1,
        )
        .map((el) => el.className)
    })
    expect(clipped).toEqual([])
    const size = await page
      .locator('.mode-text')
      .first()
      .evaluate((el) => Number.parseFloat(getComputedStyle(el).fontSize))
    expect(size).toBeGreaterThanOrEqual(11)
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })
})

test.describe('the value of X', () => {
  const FIREBALL = {
    id: 'a_x',
    type: 'cast_spell',
    label: 'Cast Fireball',
    subject: ['card_11'],
    token: 't',
    cost: { printed: '{X}{R}', modified: '{X}{R}' },
    prompts: [
      {
        kind: 'number',
        slot: 'x',
        prompt: 'Choose a value for X',
        min: 0,
        max: 2,
        values: [
          { value: 0, cost: '{R}' },
          { value: 1, cost: '{1}{R}' },
          { value: 2, cost: '{2}{R}' },
        ],
      },
    ],
  }

  test('walks the values the server enumerated and stops at their ends', async ({ page }) => {
    const socket = await serveFrames(page, [asking([pass, FIREBALL])])
    await page.goto('/')
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()

    const value = page.locator('.step-value')
    const lower = page.getByRole('button', { name: 'Lower' })
    const higher = page.getByRole('button', { name: 'Higher' })

    // It stands on the server's first value, and that value's stated cost is beside it. The
    // client worked out neither: both came off the wire.
    await expect(value).toHaveText('0')
    await expect(page.locator('.slot-step .c-pip')).toHaveCount(1)
    await expect(lower).toBeDisabled()

    await higher.click()
    await expect(value).toHaveText('1')
    // `{1}{R}` is two pips where `{R}` was one — the cost the server stated for this value.
    await expect(page.locator('.slot-step .c-pip')).toHaveCount(2)

    await higher.click()
    await expect(value).toHaveText('2')
    await expect(higher).toBeDisabled()

    await page.getByRole('button', { name: 'Confirm' }).click()
    await expect.poll(() => submissions(socket.sent).length).toBe(1)
    expect(submissions(socket.sent)[0]?.targets).toEqual([{ slot: 'x', chosen: ['2'] }])
  })
})

test.describe('a cost the game has changed', () => {
  test('carries the modified cost in the bar, marked, with the printed one beside it', async ({
    page,
  }) => {
    // §6.7: the card keeps its printed cost — there is one drawing and no variant to pass — and
    // the surface a player acts on carries what the game will charge.
    await serveFrames(page, [
      asking([
        pass,
        {
          id: 'a_cheap',
          type: 'cast_spell',
          label: 'Cast Grizzly Bears',
          subject: ['card_11'],
          token: 't',
          cost: { printed: '{1}{G}', modified: '{G}' },
          prompts: [
            {
              kind: 'pay_mana',
              slot: 'pay_0',
              prompt: 'Pay {G}',
              pip: '{G}',
              candidates: [{ id: 'x#0', source: 'card_10', label: '{G}', taps: true }],
            },
          ],
        },
      ]),
    ])
    await page.goto('/')
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()

    const cost = page.getByRole('group', { name: 'Cost' })
    await expect(cost).toBeVisible()
    // Both numbers are on screen, so the difference is legible without the mark (§5.5) — and
    // the mark is on the one a player acts on.
    await expect(cost.locator('.pay-changed')).toContainText('costs now')
    await expect(cost.locator('.pay-printed')).toContainText('card says')
    // Both costs, drawn as pips: `{1}{G}` is two and `{G}` is one, so which way it went is
    // read off the numbers rather than off a colour the client would have to work out.
    await expect(cost.locator('.pay-changed .c-pip')).toHaveCount(1)
    await expect(cost.locator('.pay-printed .c-pip')).toHaveCount(2)

    // The card itself still says what is printed on it: the frame is not a variant.
    const card = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
    await expect(card).toHaveAttribute('aria-label', /\{1\}\{G\}/)
  })

  test('marks a dearer cost exactly as it marks a cheaper one', async ({ page }) => {
    // A commander tax, or a permanent taxing a class of spells (CR 903.8, CR 601.2f). One
    // mark for both directions: the mark says *that* the cost changed and the two numbers
    // say which way, which is the whole of what the client can honestly draw.
    await serveFrames(page, [
      asking([
        pass,
        {
          id: 'a_dear',
          type: 'cast_spell',
          label: 'Cast Grizzly Bears',
          subject: ['card_11'],
          token: 't',
          cost: { printed: '{1}{G}', modified: '{3}{G}' },
          prompts: [
            {
              kind: 'pay_mana',
              slot: 'pay_0',
              prompt: 'Pay {G}',
              pip: '{G}',
              candidates: [{ id: 'x#0', source: 'card_10', label: '{G}', taps: true }],
            },
          ],
        },
      ]),
    ])
    await page.goto('/')
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()

    const cost = page.getByRole('group', { name: 'Cost' })
    await expect(cost.locator('.pay-changed')).toContainText('costs now')
    await expect(cost.locator('.pay-printed')).toContainText('card says')
    await expect(cost.locator('.pay-changed .c-pip')).toHaveCount(2)
  })
})

test.describe('putting cards back in an order', () => {
  const revealed = [
    { id: 'look_a', name: 'Forest', type_line: 'Basic Land — Forest' },
    { id: 'look_b', name: 'Mountain', type_line: 'Basic Land — Mountain' },
    { id: 'look_c', name: 'Island', type_line: 'Basic Land — Island' },
  ]

  const LOOKING = {
    ...asking([
      {
        id: 'a_order',
        type: 'player_choice',
        label: 'Choose the order',
        token: 't',
        prompts: [
          {
            kind: 'order',
            slot: 'choice',
            prompt: 'Choose the order these go on the bottom of your library, deepest first',
            items: ['look_a', 'look_b', 'look_c'],
          },
        ],
      },
    ]),
    revealed,
  }

  test('is answered by clicking, in order, in the pile', async ({ page }) => {
    const socket = await serveFrames(page, [LOOKING])
    await page.goto('/')

    // The question is one the game will not proceed past, so the dock lists it; arming it opens
    // the pile the question is about (§6.6).
    await page.getByRole('region', { name: 'Actions' }).getByRole('button').first().click()

    const pile = page.getByRole('dialog', { name: 'Shown to you' })
    await expect(pile).toBeVisible()
    // The one sentence the surface needs: a badge saying `1` does not say what `1` means.
    await expect(pile).toContainText('The first you pick goes deepest')

    const confirm = pile.getByRole('button', { name: 'Confirm' })
    await expect(confirm).toBeDisabled()

    await pile.getByRole('button', { name: /^Island/ }).click()
    await pile.getByRole('button', { name: /^Forest/ }).click()
    await expect(pile.locator('.zone-ord')).toHaveCount(2)
    await expect(pile.locator('.zone-slot', { hasText: 'Island' }).locator('.zone-ord')).toHaveText(
      '1',
    )

    // Clicking a badged card again takes it back out and renumbers the rest.
    await pile.getByRole('button', { name: /^Island/ }).click()
    await expect(pile.locator('.zone-ord')).toHaveCount(1)
    await expect(pile.locator('.zone-slot', { hasText: 'Forest' }).locator('.zone-ord')).toHaveText(
      '1',
    )

    await pile.getByRole('button', { name: /^Mountain/ }).click()
    await pile.getByRole('button', { name: /^Island/ }).click()
    await expect(confirm).toBeEnabled()
    await confirm.click()

    // The order is the whole answer, and the first id sent is the one that ends up deepest.
    await expect.poll(() => submissions(socket.sent).length).toBe(1)
    expect(submissions(socket.sent)[0]?.targets).toEqual([
      { slot: 'choice', chosen: ['look_a', 'look_b', 'look_c'] },
    ])
  })
})

test.describe('a card with two faces', () => {
  const TWO_FACED = {
    ...fixture('gameview-prompts.json'),
    my_hand: [
      {
        id: 'card_bolas',
        name: 'Nicol Bolas, the Ravager',
        type_line: 'Legendary Creature — Elder Dragon',
        mana_cost: '{1}{U}{B}{R}',
        power: '4',
        toughness: '4',
        card_types: ['creature'],
        other_face: {
          name: 'Nicol Bolas, the Arisen',
          type_line: 'Legendary Planeswalker — Bolas',
          loyalty: '7',
          rules_text: '+2: Draw two cards.',
        },
      },
    ],
    valid_actions: [pass],
  }

  test('draws the face that is up, marks that there is another, and turns over when pinned', async ({
    page,
  }) => {
    await serveFrames(page, [TWO_FACED])
    await page.goto('/')

    const card = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Nicol Bolas, the Ravager/ })
    await expect(card).toBeVisible()
    // The board draws one face and says only *that* there is another — never what is on it.
    await expect(card).toHaveAttribute('aria-label', /has another face/)
    await expect(card).not.toHaveAttribute('aria-label', /Arisen/)

    // Holding it pins it, and the pin is where the card turns over (§6.6, §6.7).
    await card.hover()
    await page.mouse.down()
    await page.waitForTimeout(700)
    await page.mouse.up()

    const pinned = page.locator('.peek')
    await expect(pinned).toBeVisible()
    await expect(pinned.locator('.card')).toHaveAttribute('aria-label', /Ravager/)
    await pinned.getByRole('button', { name: 'Turn over' }).click()
    // The other side, whole — including the fact that a back face has no mana cost, so the
    // title band's trailing slot is simply empty and the name has the whole band.
    await expect(pinned.locator('.card')).toHaveAttribute('aria-label', /Arisen/)
    await expect(pinned).toContainText('Draw two cards.')
    const pips = await pinned.locator('.c-cost .c-pip').count()
    expect(pips).toBe(0)
    // And the name is still drawn inside its own box rather than clipped.
    const fits = await pinned
      .locator('.c-name')
      .evaluate((el) => el.scrollWidth <= el.clientWidth + 1)
    expect(fits).toBe(true)
  })
})
