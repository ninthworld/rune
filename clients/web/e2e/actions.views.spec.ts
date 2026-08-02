/**
 * Acting on an object: where its actions are, and that reaching them changes nothing about what
 * is sent.
 *
 * Part of the non-blocking view tier (ADR 0011); the harness is `frames.ts`. What is asserted
 * here is the input surface rather than the drawing — one click, the list it opens beside the
 * object, and the single `choose_action` that comes out of it, which must be identical to the
 * one the dock's own button produces.
 *
 * The frames are the committed fixtures with an extra action bolted on. `gameview.json` attaches
 * exactly one action to each object, which is the case where a click simply *takes* it — so a
 * second action on one permanent is what creates the case this file is about, and building it in
 * the test keeps the shared fixtures describing the wire rather than describing a UI state.
 */
import { expect, test, type Page } from '@playwright/test'

import { DESKTOP, fixture, serveFrames, submissions } from './frames'

test.use({ viewport: DESKTOP })

/** The base view, with a second and third action attached to the Grizzly Bears on the board. */
const twoActions = () => {
  const base = fixture('gameview.json')
  return {
    ...base,
    valid_actions: [
      ...(base.valid_actions as unknown[]),
      { id: 'a5', type: 'activate_ability', label: 'Pump', subject: ['perm_bear'] },
    ],
  }
}

const open = async (page: Page, frame: unknown) => {
  const served = await serveFrames(page, [frame])
  await page.goto('/')
  return served
}

/** The card frame on the board, addressed the way every other spec addresses one. */
const card = (page: Page, name: string) =>
  page.getByRole('region', { name: 'Your battlefield' }).getByRole('button', {
    name: new RegExp(`^${name}`),
  })

test.describe('an object with more than one action', () => {
  test('offers them beside the object, and nothing the server did not attach', async ({ page }) => {
    await open(page, twoActions())

    // The click that already meant "select" — an object the server attached two actions to has
    // no single meaning, so the click opens the list instead of guessing which one was meant.
    await card(page, 'Grizzly Bears').click()

    const menu = page.getByRole('group', { name: 'Grizzly Bears actions' })
    await expect(menu.getByRole('button', { name: 'Tap for mana' })).toBeVisible()
    await expect(menu.getByRole('button', { name: 'Pump' })).toBeVisible()
    // Exactly those, plus reading and leaving. An action attached to a different object is not
    // this object's action, however close the two are drawn.
    await expect(menu.getByRole('button', { name: 'Cast Lightning Bolt' })).toHaveCount(0)
    await expect(menu.getByRole('button', { name: 'Pass' })).toHaveCount(0)
    await expect(menu.getByRole('button', { name: 'Inspect' })).toBeVisible()

    // Beside the object rather than over it: the card it belongs to stays visible, which is the
    // entire reason the list moved off the bottom of the screen.
    await expect(card(page, 'Grizzly Bears')).toBeVisible()
  })

  test('sends exactly the submission the dock’s own button would', async ({ page }) => {
    const { sent } = await open(page, twoActions())

    await card(page, 'Grizzly Bears').click()
    await page
      .getByRole('group', { name: 'Grizzly Bears actions' })
      .getByRole('button', {
        name: 'Pump',
      })
      .click()

    // One `choose_action`, naming the id the server issued. The menu is a place to click, not a
    // second path through the client.
    const [message] = submissions(sent)
    expect(message).toMatchObject({ type: 'choose_action', action_id: 'a5' })
    expect(submissions(sent)).toHaveLength(1)
  })

  test('keeps the whole list in the dock as well', async ({ page }) => {
    await open(page, twoActions())

    // #626's rule, unchanged: a subject can sit in a collapsed pile or in no rendered zone at
    // all, so nothing may become reachable *only* by finding the card that owns it.
    const actions = page.getByRole('region', { name: 'Actions' })
    await actions.getByText('Every action (5)').click()
    const every = actions.getByRole('list', { name: 'Every action' })
    await expect(every.getByRole('button', { name: 'Pump' })).toBeVisible()
    await expect(every.getByRole('button', { name: 'Tap for mana' })).toBeVisible()
  })
})

test.describe('an object with exactly one action', () => {
  test('takes it on the click, with no list to traverse', async ({ page }) => {
    const { sent } = await open(page, fixture('gameview.json'))

    // The rule that makes a one-click table worth having. One action means the click has one
    // meaning, so it is that meaning — and a menu in front of it would be a click tax on every
    // land drop in the game.
    await card(page, 'Grizzly Bears').click()

    expect(submissions(sent)).toMatchObject([{ action_id: 'a4' }])
    await expect(page.getByRole('group', { name: 'Grizzly Bears actions' })).toHaveCount(0)
  })
})

test.describe('reaching an object’s actions without a mouse', () => {
  test('takes the keyboard when it opens and hands it back when it closes', async ({ page }) => {
    await open(page, twoActions())

    // Every affordance here is a button, reached the same way by both devices: tab to the card,
    // press it, and the list that opens already has focus. This is the whole argument against a
    // native context menu — a gesture with no keyboard equivalent has to be reinvented for
    // every control scheme this client is ever ported to.
    await card(page, 'Grizzly Bears').focus()
    await page.keyboard.press('Enter')

    const menu = page.getByRole('group', { name: 'Grizzly Bears actions' })
    await expect(menu.getByRole('button', { name: 'Tap for mana' })).toBeFocused()
    await page.keyboard.press('ArrowDown')
    await expect(menu.getByRole('button', { name: 'Pump' })).toBeFocused()

    // And backing out returns the keyboard where it came from, rather than to the top of the
    // document — which is what makes the next Tab press mean what a player expects.
    await page.keyboard.press('Escape')
    await expect(menu).toHaveCount(0)
    await expect(card(page, 'Grizzly Bears')).toBeFocused()
  })

  test('leaves priority to the board while a list is open', async ({ page }) => {
    const { sent } = await open(page, twoActions())

    await card(page, 'Grizzly Bears').click()
    // Space is the key that carries the game everywhere else, including on a focused control.
    // With a list open it belongs to the list: a player mid-choice has not asked to pass.
    await page.keyboard.press('Escape')
    await page.keyboard.press(' ')

    expect(submissions(sent)).toMatchObject([{ action_id: 'a1' }])
  })
})

/**
 * `docs/client-design.md` §6.5: the board answers, and the dock carries what the board cannot.
 *
 * The two halves are one rule and are asserted together, because either alone is a defect. A
 * subject that is drawn must not *also* be a button in the dock — that second copy is what made a
 * question grow until `overflow: hidden` cut the controls under it (#678). A subject that is not
 * drawn must still be a button in the dock — a card in a closed pile has nothing on the table to
 * click, and an action reachable only by finding its object is an action a player cannot take.
 *
 * The frame is written out here rather than committed, because no fixture has a graveyard the
 * server is offering as a candidate.
 */
const RAISE = {
  you: 'p0',
  phase: 'precombat_main',
  turn: 3,
  me: { life: 20, library_size: 40 },
  opponents: [{ player_id: 'p1', life: 20, hand_size: 3, library_size: 40, graveyard_size: 0 }],
  seat_order: ['p0', 'p1'],
  active_player: 'p0',
  priority_player: 'p0',
  my_hand: [
    {
      id: 'h_raise',
      name: 'Raise Dead',
      type_line: 'Sorcery',
      card_types: ['sorcery'],
      mana_cost: '{B}',
    },
  ],
  battlefield: [
    {
      id: 'perm_bear',
      controller: 'p0',
      owner: 'p0',
      card: {
        id: 'perm_bear',
        name: 'Grizzly Bears',
        type_line: 'Creature — Bear',
        card_types: ['creature'],
        power: '2',
        toughness: '2',
      },
    },
  ],
  graveyards: [
    {
      player_id: 'p0',
      cards: [
        {
          id: 'g_zombie',
          name: 'Walking Corpse',
          type_line: 'Creature — Zombie',
          card_types: ['creature'],
          mana_cost: '{2}{B}',
          power: '2',
          toughness: '2',
        },
      ],
    },
  ],
  valid_actions: [
    { id: 'a0', type: 'pass_priority', label: 'Pass' },
    {
      id: 'raise',
      type: 'cast_spell',
      label: 'Cast Raise Dead',
      subject: ['h_raise'],
      token: 'traise',
      requirements: [
        {
          slot: 't0',
          prompt: 'Return target creature card from your graveyard',
          // One candidate the table draws and one it does not, in a single slot, so the rule is
          // asserted on the same question rather than on two convenient ones.
          candidates: ['g_zombie', 'perm_bear'],
        },
      ],
    },
  ],
}

test.describe('a subject the board draws, and one it does not', () => {
  const armed = async (page: Page) => {
    const served = await open(page, RAISE)
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Raise Dead/ })
      .click()
    await expect(page.getByRole('region', { name: 'Choices' })).toBeVisible()
    return served
  }

  test('lists only the one the board cannot answer', async ({ page }) => {
    await armed(page)
    const choices = page.getByRole('region', { name: 'Choices' })

    // In a closed pile, so there is nothing on the table to click: the dock is its only path.
    await expect(choices.getByRole('button', { name: 'Walking Corpse' })).toBeVisible()
    // On the board and highlighted, so the board is its path and the dock does not repeat it.
    await expect(choices.getByRole('button', { name: 'Grizzly Bears' })).toHaveCount(0)
    await expect(
      page
        .getByRole('region', { name: 'Your battlefield' })
        .getByRole('button', { name: /^Grizzly Bears/ }),
    ).toHaveClass(/card--candidate/)
  })

  test('sends the same submission from the dock as the board would', async ({ page }) => {
    const { sent } = await armed(page)

    await page
      .getByRole('region', { name: 'Choices' })
      .getByRole('button', {
        name: 'Walking Corpse',
      })
      .click()
    await expect(page.getByRole('region', { name: 'Choices' })).toContainText('1 chosen')
    await page.getByRole('button', { name: 'Confirm' }).click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'raise',
      token: 'traise',
      targets: [{ slot: 't0', chosen: ['g_zombie'] }],
    })
  })

  test('answers on the board with the keyboard, as it does with the pointer', async ({ page }) => {
    const { sent } = await armed(page)

    // §6.5 rule 4. Everything the pointer reaches the keyboard reaches, *including answering on
    // the board* — which is the path that carries most of a question now that the dock has
    // stopped listing the subjects the table already drew. Enter is deliberately the browser's on
    // a focused control (`keys.ts`), which is what makes the card behave like the button it is.
    await page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .focus()
    await page.keyboard.press('Enter')
    await expect(page.getByRole('region', { name: 'Choices' })).toContainText('1 chosen')

    // And Space still commits the finished answer, from wherever the focus happens to be.
    await page.keyboard.press(' ')
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'raise',
      targets: [{ slot: 't0', chosen: ['perm_bear'] }],
    })
  })
})
