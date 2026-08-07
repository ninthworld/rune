/**
 * Watching a table, driven by the committed spectator fixtures.
 *
 * The same tier and the same argument as `views.spec.ts`: no server, no engine, no game — given
 * exactly this `SpectatorView`, the browser renders this. Non-blocking (ADR 0011).
 *
 * What this file is about is the half of the spectator surface a unit test cannot reach: that a
 * whole public game is reconstructed from one frame, that **no control capable of moving the game
 * is on screen**, and that a spectator who loses their socket is put back in front of the table
 * rather than left watching a board that has stopped.
 */
import { expect, test, type Page } from '@playwright/test'

import {
  DESKTOP,
  fixture,
  messages,
  open,
  openSide,
  pageFits,
  serveFrames,
  serveSockets,
} from './frames'

test.use({ viewport: DESKTOP })

/** Watch the last table in the directory — `lobbyview-open.json`'s in-progress room. */
const watchLast = (page: Page) =>
  page.getByRole('region', { name: 'Tables' }).getByRole('button', { name: 'Watch' }).last().click()

test.describe('a spectator joining mid-game', () => {
  test('reconstructs the whole public board from its first view', async ({ page }) => {
    await serveFrames(page, [fixture('spectatorview.json')])
    await page.goto('/')

    // The turn, drawn from the same strip a player reads.
    await expect(page.getByRole('heading', { name: /^Turn 5 — Ari/ }).first()).toBeVisible()

    // Both seats, each with its own half of the table and its own public counts. The near
    // chair is the first seat in seat order, the rest sit across from it (§6.10).
    await expect(page.getByRole('region', { name: 'Ari: battlefield' })).toBeVisible()
    await expect(page.getByRole('region', { name: 'Sam: battlefield' })).toBeVisible()
    await expect(page.getByRole('button', { name: /Ari, 18 life/ })).toBeVisible()
    await expect(page.getByRole('button', { name: /Sam, 20 life/ })).toBeVisible()
    await expect(page.getByLabel('Sam: Hand, 6')).toBeVisible()
    await expect(page.getByLabel('Sam: Library, 31')).toBeVisible()

    // The permanents each seat controls, named from the view.
    await expect(
      page.getByRole('region', { name: 'Ari: battlefield' }).getByRole('button', {
        name: /^Llanowar Elves/,
      }),
    ).toBeVisible()
    await expect(
      page.getByRole('region', { name: 'Sam: battlefield' }).getByRole('button', {
        name: /^Serra Angel/,
      }),
    ).toBeVisible()

    // The public stack and the public log, in the column they share with a seated board.
    await openSide(page)
    const stack = page.getByRole('region', { name: 'Stack', exact: true })
    await expect(stack).toContainText('resolves next')
    await expect(stack).toContainText('Lightning Strike')
    await page.getByRole('button', { name: 'Log' }).click()
    await expect(page.getByRole('region', { name: 'Log' })).toContainText('Ari')

    // What the strip says: that this is watching, whose move it is, and where in the turn.
    const strip = page.getByRole('region', { name: 'Watching' })
    await expect(strip).toContainText('Watching — Ari to act')
    await expect(strip).toContainText('Turn 5 · Precombat main')

    // §3: the page itself never scrolls, in either axis.
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })

  test('renders no control that could move the game', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('spectatorview.json')])
    await page.goto('/')
    await expect(page.getByRole('region', { name: 'Watching' })).toBeVisible()
    await openSide(page)

    // None of the player-only affordances exists: there is no hand, no action bar, no pace
    // preference (a preference belongs to a seat), and no concede.
    await expect(page.getByRole('region', { name: 'Your hand' })).toHaveCount(0)
    await expect(page.getByRole('region', { name: 'Actions' })).toHaveCount(0)
    await expect(page.getByRole('group', { name: 'Pace' })).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Concede' })).toHaveCount(0)

    // The turn strip is still drawn whole — and every step in it is a read-out rather than the
    // control that sets a stop there, because a spectator holds no stop preference.
    const steps = page.getByRole('list', { name: 'Turn steps' }).first()
    await expect(steps.getByRole('listitem')).toHaveCount(12)
    await expect(steps.getByRole('button')).toHaveCount(0)

    // Reading is free and costs no click, exactly as it does at a seat: clicking a permanent
    // pins its face in the column, and sends nothing.
    await page
      .getByRole('region', { name: 'Sam: battlefield' })
      .getByRole('button', { name: /^Serra Angel/ })
      .click()
    await expect(page.getByRole('button', { name: 'Unpin' })).toBeVisible()

    // And the socket has carried nothing but the greeting — a spectator connection ignores what
    // a client writes to it, so a screen that could write is a screen that lies about itself.
    expect(messages(sent, 'choose_action')).toHaveLength(0)
    expect(messages(sent, 'set_stops')).toHaveLength(0)
  })

  test('moves the spectator behind whichever chair they click', async ({ page }) => {
    await serveFrames(page, [fixture('spectatorview-commander.json')])
    await page.goto('/')
    // This fixture is a finished game, and its verdict layers over the table until it is put
    // aside — which is the point of it being dismissible.
    await page.getByRole('button', { name: 'Look at the board' }).click()

    // Four seats. The near chair is the first seat still in the game — Alice sits first in seat
    // order and is out, and the nearest half of the table is not spent on a seat that controls
    // nothing.
    const near = page.locator('.field-mine')
    await expect(near.getByRole('button', { name: 'Bob, 27 life' })).toBeVisible()
    await expect(page.locator('.field-opponent')).toHaveCount(3)

    // Clicking somebody else's seat is the one gesture watching has that playing does not, and
    // a seat that is out is still one a spectator may go and read.
    await page.locator('.field-opponent').getByRole('button', { name: 'Alice, 0 life' }).click()
    await expect(near.getByRole('button', { name: 'Alice, 0 life' })).toBeVisible()
    await expect(near.getByRole('button', { name: 'Bob, 27 life' })).toHaveCount(0)
    await expect(page.locator('.field-opponent')).toHaveCount(3)
  })
})

test.describe('a table a spectator can read all of', () => {
  test('opens a public pile and states the Commander facts and the result', async ({ page }) => {
    await serveFrames(page, [fixture('spectatorview-commander.json')])
    await page.goto('/')

    // A finished game announces itself over the board, in the third person: no seat is the
    // reader's, so the outcome is never stated as "You win".
    const over = page.getByRole('dialog', { name: 'Game over' })
    await expect(over).toContainText('Bob wins.')
    await expect(over).toContainText('21 commander damage')
    await expect(over).not.toContainText('You win')
    await over.getByRole('button', { name: 'Look at the board' }).click()

    // The command zone rides only in a game that has one, and it opens as the same dialog a
    // seated player opens — a pile is not the board.
    await page.getByLabel('Random: Command zone').click()
    const pile = page.getByRole('dialog', { name: /Random — Command/ })
    await expect(pile).toContainText('Thraximundar')
    await pile.getByRole('button', { name: '✕' }).click()

    // An exiled card in front of a seat that is out is still public and still reachable.
    await page.getByLabel('Alice: Exile').click()
    await expect(page.getByRole('dialog', { name: /Alice — Exile/ })).toContainText(
      'Karn, Silver Golem',
    )
  })
})

test.describe('a spectator whose socket drops', () => {
  test('asks to watch the same room again and resumes on the next view', async ({ page }) => {
    // The route this takes matters: a spectator holds no seat, so the server drops it on
    // disconnect and the reconnect is a fresh `spectate_room` rather than a reclaimed token
    // (`docs/protocol.md`). Which means the tab has to remember the room, so the test goes in
    // through the lobby the way a spectator really arrives.
    const { sent, drop } = await serveSockets(page, [
      [fixture('lobbyview-open.json')],
      [fixture('lobbyview-open.json'), fixture('spectatorview.json')],
    ])
    await open(page, 'Watcher')

    // The last row is the in-progress table; the one above it is full but still gathering, and
    // both are reachable only by watching, which is the point of the `Watch` button.
    await watchLast(page)
    expect(messages(sent, 'spectate_room')).toEqual([{ type: 'spectate_room', room_id: 'r_312' }])

    await drop()

    // The board comes back, and the request that brought it was the client's own: the second
    // socket saw a lobby view, recognised this tab was watching, and asked again.
    await expect(page.getByRole('region', { name: 'Watching' })).toBeVisible()
    await expect(page.getByRole('region', { name: 'Ari: battlefield' })).toBeVisible()
    expect(messages(sent, 'spectate_room')).toHaveLength(2)
  })

  test('asks exactly once per socket, so a room that has ended is not asked forever', async ({
    page,
  }) => {
    const { sent, drop, push } = await serveSockets(page, [
      [fixture('lobbyview-open.json')],
      [
        fixture('lobbyview-open.json'),
        { lobby_error: { code: 'room_not_found', reason: 'That table is gone.' } },
        fixture('lobbyview-open.json'),
      ],
    ])
    await open(page, 'Watcher')
    await watchLast(page)
    push(fixture('spectatorview.json'))
    await expect(page.getByRole('region', { name: 'Watching' })).toBeVisible()

    await drop()

    // The room has ended while the socket was down. One request went out on the new socket, the
    // error came back, and the board — which will never move again — is given up rather than
    // left on screen pretending to be a game. The lobby is what the spectator is left in front
    // of, and the request is not repeated at every lobby view that follows.
    await expect(page.getByRole('heading', { name: 'Open tables' })).toBeVisible()
    await expect(page.getByRole('region', { name: 'Watching' })).toHaveCount(0)
    expect(messages(sent, 'spectate_room')).toHaveLength(2)
  })
})
