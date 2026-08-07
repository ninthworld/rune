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
  messages,
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

    // Every entry says **which** spell or ability it is (issue #715). This fixture holds the
    // case that made it necessary: an activated and a triggered ability whose names, controllers,
    // and thumbnails give a player nothing to tell them apart, and whose own text does. The
    // sentence is the server's — the client neither composes it nor parses it.
    await expect(stack).toContainText('Tap target creature.')
    await expect(stack).toContainText('Add {G}.')
    // And a spell does not repeat its own name underneath itself.
    await expect(stack.locator('.stack-detail', { hasText: /^Lightning Bolt$/ })).toHaveCount(0)

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

test.describe('the settle, made legible', () => {
  test('says where the game went and what happened while nobody was asked', async ({ page }) => {
    // `gameview-turn.json` is the issue's headline case, already on the wire: the server acted
    // for this seat across a turn boundary, and a spell resolved, dealt damage, and killed a
    // creature while it did. The board shows the result; this says how it got there (§6.9).
    await serveFrames(page, [fixture('gameview-turn.json')])
    await page.goto('/')

    const band = page.getByRole('status')
    await expect(band).toBeVisible()
    // Where it ended, and how far it went — the two questions a jump raises.
    await expect(band).toContainText('Passed 3 steps')
    await expect(band).toContainText('Upkeep')
    // And what a player would have watched happen. The words are the log's own.
    await expect(band).toContainText('dies')

    // It costs the board no height: the page still fits in both axes at this size (§3).
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })

  test('says nothing on a view the player was asked for', async ({ page }) => {
    // No mark, no band. This is also the reconnect case — a fresh view of the current state
    // carries no settle for this receiver, so there is nothing to suppress.
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()
    await expect(page.getByRole('status')).toHaveCount(0)
  })
})

test.describe('undo, where the table allows it', () => {
  test('offers undo where the table allows it, and nowhere else', async ({ page }) => {
    // `gameview-turn.json` is a table that allows undo with three checkpoints left, so the
    // control is drawn, pressable, and carries the count (issue #648). One press sends one
    // `undo` and asserts nothing locally — the restored state arrives as an ordinary view.
    const socket = await serveFrames(page, [fixture('gameview-turn.json')])
    await page.goto('/')
    await openSide(page)

    const undo = page.getByRole('button', { name: /^Undo/ })
    await expect(undo).toBeEnabled()
    await expect(undo).toContainText('3')
    await undo.click()
    await expect.poll(() => messages(socket.sent, 'undo')).toEqual([{ type: 'undo' }])
    // Nothing was sent as a play: an undo is not an action the rules offered.
    expect(submissions(socket.sent)).toHaveLength(0)
  })

  test('drops everything it was holding when it asks for a rollback', async ({ page }) => {
    // A rollback is the one moment where every piece of held interaction is stale at
    // once, so the board throws all of it away as it asks (issue #648). A concede
    // waiting for its second click is the visible case: it is local state, it is
    // destructive, and it must not survive into a state it was never asked about.
    await serveFrames(page, [fixture('gameview-turn.json')])
    await page.goto('/')
    await openSide(page)

    await page.getByRole('button', { name: 'Concede' }).click()
    await expect(page.getByRole('button', { name: 'Yes, concede the game' })).toBeVisible()

    await page.getByRole('button', { name: /^Undo/ }).click()
    // Back to asking, with nothing armed — and no `choose_action` ever left the tab.
    await expect(page.getByRole('button', { name: 'Concede' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Yes, concede the game' })).toHaveCount(0)
  })

  test('draws no undo control at a table that does not carry the rule', async ({ page }) => {
    // `gameview.json` has no `undo` field at all, which is every table by default — so there
    // is no control, rather than a disabled one implying the rule might exist.
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await openSide(page)

    await expect(page.getByRole('button', { name: /^Undo/ })).toHaveCount(0)
  })
})
