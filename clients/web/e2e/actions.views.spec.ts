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
