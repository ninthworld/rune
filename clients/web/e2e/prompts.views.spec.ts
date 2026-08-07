/**
 * The two committed prompt fixtures, in a real browser (issue #774).
 *
 * The unit suite proves these views *parse* and the Rust tests prove the server *sends* them.
 * Neither answers the question a player has: can I see what I am being asked, can I reach the
 * cards that answer it, and does clicking them send the answer? A mid-resolution choice is drawn
 * in a dialog over the board and a mulligan is drawn in the bar, so the two shapes fail in
 * completely different ways — and both were rendered by nothing before this file.
 *
 * This is the **non-blocking** tier (ADR 0011): breadth lives here, and the one blocking path
 * stays `smoke.spec.ts`.
 */
import { expect, test } from '@playwright/test'

import { DESKTOP, fixture, pageFits, serveFrames, submissions } from './frames'

test.use({ viewport: DESKTOP })

/** The question's own words, which the bar carries as the way into it. */
const SCRY = 'Choose up to 2 cards to put on the bottom of your library, in that order'

test.describe('a mid-resolution choice, from gameview-choice.json', () => {
  test('is discoverable before it is opened: the bar asks, the panel shows', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-choice.json')])
    await page.goto('/')

    // The question is reachable from the bar in the server's own words (§6.5) — nothing is
    // hidden behind a menu, and nothing had to be clicked to learn that the game is waiting.
    await expect(
      page.getByRole('region', { name: 'Actions' }).getByRole('button', { name: SCRY }),
    ).toBeVisible()

    // And the cards it is about are already on screen: `revealed` is what this seat alone is
    // being shown, so it is drawn beside the stack rather than only inside the question.
    const side = page.getByRole('region', { name: 'Stack', exact: true })
    await expect(side).toContainText('shown to you')
    await expect(side.getByRole('button', { name: /^Island/ })).toBeVisible()
    await expect(side.getByRole('button', { name: /^Swamp/ })).toBeVisible()
  })

  test('opens as the dialog the cards are in, carrying the question and a tally', async ({
    page,
  }) => {
    await serveFrames(page, [fixture('gameview-choice.json')])
    await page.goto('/')
    await page.getByRole('button', { name: SCRY }).click()

    const asked = page.getByRole('dialog', { name: 'Shown to you' })
    await expect(asked).toBeVisible()
    await expect(asked.getByRole('button', { name: /^Island/ })).toBeVisible()
    await expect(asked.getByRole('button', { name: /^Swamp/ })).toBeVisible()

    // The server's own words, and a tally of what has been chosen against what is needed —
    // the two things the cards themselves cannot say.
    const note = asked.getByRole('status')
    await expect(note).toContainText('Choose up to 2 cards')
    await expect(note).toContainText('0 of 2')
  })

  test('answers by clicking a candidate, and sends the action it belongs to', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('gameview-choice.json')])
    await page.goto('/')
    await page.getByRole('button', { name: SCRY }).click()

    const asked = page.getByRole('dialog', { name: 'Shown to you' })
    const confirm = asked.getByRole('button', { name: 'Confirm' })
    // `min` is 0, so choosing nothing is already a legal answer and Confirm is live from the
    // start — declining to scry is an answer, not an absence of one.
    await expect(confirm).toBeEnabled()

    await asked.getByRole('button', { name: /^Island/ }).click()
    await expect(asked.getByRole('status')).toContainText('1 of 2')

    await confirm.click()
    await expect
      .poll(() => submissions(socket.sent).map((message) => message.action_id))
      .toEqual(['a0'])
    // The chosen card rides on the submission: the client sends back the id the view gave it,
    // never a card it looked up for itself.
    expect(JSON.stringify(submissions(socket.sent)[0])).toContain('card_20')
  })

  test('fits the viewport it is drawn in, with no region scrolling', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-choice.json')])
    await page.setViewportSize({ width: 1024, height: 640 })
    await page.goto('/')
    await page.getByRole('button', { name: SCRY }).click()

    await expect(page.getByRole('dialog', { name: 'Shown to you' })).toBeVisible()
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })
})

test.describe('a mulligan and its bottoming, from gameview-prompts.json', () => {
  test('draws the two options as the question itself', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-prompts.json')])
    await page.goto('/')
    await page.getByRole('button', { name: 'Keep or mulligan' }).click()

    // Where the server's options state the question, the options *are* the question and nothing
    // is written above them (§6.5).
    await expect(page.getByRole('button', { name: 'Keep this hand' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Mulligan' })).toBeVisible()
  })

  test('sends a mulligan on its own', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('gameview-prompts.json')])
    await page.goto('/')
    await page.getByRole('button', { name: 'Keep or mulligan' }).click()

    await page.getByRole('button', { name: 'Mulligan' }).click()
    await page.getByRole('button', { name: 'Confirm' }).click()

    await expect
      .poll(() => submissions(socket.sent).map((message) => message.action_id))
      .toEqual(['a0'])
    const sent = JSON.stringify(submissions(socket.sent)[0])
    expect(sent).toContain('mulligan')
    // Keeping owes a card to the bottom; a mulligan owes nothing, so nothing from the hand
    // rides along with it.
    expect(sent).not.toContain('card_10')
  })

  test('keeping owes a card to the bottom before it can be sent', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('gameview-prompts.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Keep or mulligan' }).click()
    const confirm = page.getByRole('button', { name: 'Confirm' })
    await page.getByRole('button', { name: 'Keep this hand' }).click()
    // `requires: ["bottom"]` — the second slot exists only once *keep* is the answer, and the
    // answer is incomplete until it is filled.
    await expect(confirm).toBeDisabled()

    await page.getByRole('region', { name: 'Your hand' }).getByRole('button').first().click()
    await expect(confirm).toBeEnabled()
    await confirm.click()

    const sent = JSON.stringify(submissions(socket.sent)[0])
    expect(sent).toContain('keep')
    expect(sent).toContain('card_1')
  })
})
