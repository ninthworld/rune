/**
 * The board, driven by committed fixtures over an intercepted WebSocket.
 *
 * No server, no engine, no game — just "given exactly this view, the browser renders this". The
 * fixtures are the same files the Rust tests pin and the unit suite parses, so this tier cannot
 * drift from the wire shape; what it adds over a unit test is a real build, in a real browser,
 * painting real DOM.
 *
 * This is the **non-blocking** tier (ADR 0011). Breadth lives here so breadth never gates a merge
 * on browser flake; the one blocking path is `smoke.spec.ts`.
 *
 * What this file covers is the board itself: the regions the design puts on screen, what a click
 * on one means, and what a dropped socket does to a click in flight. The pre-game screens are in
 * `lobby.views.spec.ts`, the card is in `card.views.spec.ts`, and the sweep across supported
 * viewports is in `scale.views.spec.ts`.
 */
import { expect, test } from '@playwright/test'

import {
  DESKTOP,
  fixture,
  openSide,
  pageFits,
  serveFrames,
  serveSockets,
  submissions,
} from './frames'

test.use({ viewport: DESKTOP })

test.describe('the board, from one view', () => {
  test('draws the turn, both halves of the table, the hand, and the stack', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // The whole turn, always drawn, and the step the game is in marked as the current one
    // (§4.1). Twelve steps, from the mirror's own enum rather than a list written out again.
    const steps = page.getByRole('list', { name: 'Turn steps' })
    await expect(steps.getByRole('button')).toHaveCount(12)
    await expect(steps.locator('[aria-current="step"]')).toHaveText('Main 1')
    await expect(page.getByRole('heading', { name: /^Turn 3 — / })).toBeVisible()

    // Where a permanent is answers whose it is. Both halves exist, and the cards in them are
    // named from the view — nothing is looked up client-side.
    const mine = page.getByRole('region', { name: 'Your battlefield' })
    await expect(mine.getByRole('button', { name: /^Grizzly Bears/ })).toBeVisible()
    await expect(mine.getByRole('button', { name: /^Thopter/ })).toBeVisible()

    const hand = page.getByRole('region', { name: 'Your hand' })
    await expect(hand.getByRole('button', { name: /^Llanowar Elves/ })).toBeVisible()
    await expect(hand.getByRole('button', { name: /^Lightning Bolt/ })).toBeVisible()

    // A token (CR 111) is a permanent with no card behind it: it renders from the view's
    // characteristics like anything else, and says it is a token so a player can tell.
    await expect(mine.getByRole('button', { name: /Thopter.*Token/ })).toBeVisible()

    // Topmost first: what a player needs from this column is what resolves next.
    await openSide(page)
    const stack = page.getByRole('region', { name: 'Stack', exact: true })
    await expect(stack).toContainText('resolves next')
    await expect(stack).toContainText('Lightning Bolt')

    // §3: the page itself never scrolls, in either axis.
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })

  test('draws a seat as its counts, and says what the view did not state', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // The opponent's projection carries life, hand size, library and graveyard; this seat is
    // also the monarch, which the view states and the bar restates in words.
    await expect(page.getByRole('button', { name: /Opponent, 20 life/ })).toBeVisible()
    await expect(page.getByLabel('Opponent: Hand, 7')).toBeVisible()
    await expect(page.getByLabel('Opponent: Library, 53')).toBeVisible()

    // `me` is absent in this fixture, so your own totals were never sent. A count nobody stated
    // is drawn as no count rather than as a zero that would read as a real number.
    await expect(page.getByLabel('You: Library')).toContainText('—')
  })

  test('takes the one action the server offered on the object that owns it', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // `play_land` names the Forest in hand and nothing else, so one click on the card takes it —
    // the same path the bar's own button takes (`interaction.ts`).
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Forest/ })
      .click()

    await expect
      .poll(() => submissions(socket.sent).map((message) => message.action_id))
      .toEqual(['a2'])
    // The submission carries a client-generated correlation id the server echoes back verbatim.
    expect(submissions(socket.sent)[0]).toHaveProperty('submission')
  })

  test('holds a click as pending until the server answers it', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const actions = page.getByRole('region', { name: 'Actions' })
    await actions.getByRole('button', { name: 'Pass' }).click()
    await expect(actions).toContainText('waiting')

    // The ack closes the correlation, and the fresh view is the answer to what happened.
    socket.push({ ...fixture('gameview.json'), action_ack: { submission: 's:1', accepted: true } })
    await expect(actions).not.toContainText('waiting')
  })

  test('keeps the board on screen when the socket drops, and releases the wait', async ({
    page,
  }) => {
    const socket = await serveSockets(page, [
      [fixture('gameview.json')],
      [fixture('gameview.json')],
    ])
    await page.goto('/')

    const actions = page.getByRole('region', { name: 'Actions' })
    await actions.getByRole('button', { name: 'Pass' }).click()
    await expect(actions).toContainText('waiting')

    // A dropped socket is not a dead end: the seat is held open server-side and the client
    // reconnects on its own. The board it was already showing stays — blanking it would throw
    // away the only accurate picture the player has.
    socket.drop()
    await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()
    // The server drops a seat's `action_ack` when it reconnects, so the ack that would have
    // answered this click is never coming and the wait is released rather than held forever.
    await expect(actions).not.toContainText('waiting')
  })

  test('opens a public pile beside the table, and closes it again', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')

    // The seat the view named, and the one pile in this fixture that has cards in it: a zone
    // with nothing itemized has nothing to open, and says so by not being pressable.
    const graveyard = page.getByLabel(/Graveyard, 10/)
    await graveyard.click()
    const pile = page.getByRole('dialog', { name: /Graveyard/ })
    await expect(pile).toBeVisible()
    await expect(pile).toContainText('card')

    // The control that opened it is the way out, and so is Escape.
    await page.keyboard.press('Escape')
    await expect(pile).toHaveCount(0)
  })

  test('says the game is over, and offers the way out', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-over.json')])
    await page.goto('/')

    const result = page.getByRole('dialog', { name: 'Game over' })
    await expect(result).toBeVisible()
    await expect(result).toContainText('By ')
    // Dismissable, because the final board is worth reading.
    await result.getByRole('button', { name: 'Look at the board' }).click()
    await expect(result).toHaveCount(0)
  })

  test('says plainly when it could not read what the server sent', async ({ page }) => {
    // A frame that is not a JSON object at all: every object-shaped frame is a `LobbyView` by
    // the protocol's last rule, so this is what "this build cannot read it" looks like.
    await serveFrames(page, [[1, 2, 3]])
    await page.goto('/')
    await expect(page.getByRole('status')).toContainText('could not be read')
  })
})
