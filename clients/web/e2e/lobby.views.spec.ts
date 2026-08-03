/**
 * The three screens in front of a game, driven by committed fixtures.
 *
 * `docs/client-design.md` §9: connect, the lobby, and the table room. There is no shell — the
 * topbar of each screen is its navigation, and settings is a dialog over whatever you were
 * already on.
 *
 * What is worth pinning here is the boundary rather than the drawing: a control exists because
 * the server advertised the command behind it, a table's occupancy is what the server counted,
 * and the two panels with nothing behind them say so instead of pretending.
 */
import { expect, test } from '@playwright/test'

import { DESKTOP, fixture, messages, open, serveFrames } from './frames'

test.use({ viewport: DESKTOP })

test.describe('connect', () => {
  test('asks who you are before the lobby, and says it once', async ({ page }) => {
    const socket = await serveFrames(page, [
      fixture('lobbyview-open.json'),
      fixture('catalogview.json'),
    ])
    await page.goto('/')

    // The one place the product says its own name (§9.3).
    await expect(page.getByRole('heading', { name: 'SAGE' })).toBeVisible()
    // The button is a recess until the form is complete: a disabled state is a state of the
    // material, not a greyed-out button.
    await expect(page.getByRole('button', { name: 'Connect' })).toBeDisabled()

    await page.getByLabel('Name').fill('Ada')
    await page.getByRole('button', { name: 'Connect' }).click()

    await expect(page.getByRole('heading', { name: 'Open tables' })).toBeVisible()
    // Said once per socket, on the command the server is currently advertising.
    await expect
      .poll(() => messages(socket.sent, 'set_name'))
      .toEqual([{ type: 'set_name', name: 'Ada' }])
    // And the catalog is asked for per socket, because its answer is a one-shot frame.
    expect(messages(socket.sent, 'request_catalog')).toHaveLength(1)
  })

  test('reaches settings before ever joining a table', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview.json')])
    await page.goto('/')
    await page.getByTitle('Settings').click()

    const settings = page.getByRole('dialog', { name: 'Settings' })
    await expect(settings).toBeVisible()
    // Card art is off until it is chosen (ADR 0012), so the two faces that need pictures are
    // not offerable yet and say what would make them so.
    await expect(settings.getByRole('button', { name: /Frame and art/ })).toBeDisabled()
    await expect(settings).toContainText('Turn on card art')
    await page.keyboard.press('Escape')
    await expect(settings).toHaveCount(0)
  })
})

test.describe('the lobby', () => {
  test('draws a table as one row of the same columns, and leads with what is offered', async ({
    page,
  }) => {
    const socket = await serveFrames(page, [
      fixture('lobbyview-open.json'),
      fixture('catalogview.json'),
    ])
    await open(page, 'Ada')

    const list = page.getByRole('region', { name: 'Tables' })
    await expect(list).toContainText('Kitchen table')
    // Occupancy as a count beside the pips, and the state where the host would be named — the
    // wire carries no host.
    await expect(list).toContainText('1/2')
    await expect(list).toContainText('Gathering')

    // A full table is not pressable; a joinable one leads with the command the server offered.
    const rows = list.locator('.table-row')
    await expect(rows.nth(0).getByRole('button')).toHaveText('Join')
    await expect(rows.nth(1).getByRole('button')).toHaveText('Watch')

    await rows.nth(0).getByRole('button').click()
    await expect
      .poll(() => messages(socket.sent, 'join_room'))
      .toEqual([{ type: 'join_room', room_id: 'r_310' }])
  })

  test('filters the list it was sent, and asks the server nothing to do it', async ({ page }) => {
    const socket = await serveFrames(page, [
      fixture('lobbyview-open.json'),
      fixture('catalogview.json'),
    ])
    await open(page, 'Ada')

    const list = page.getByRole('region', { name: 'Tables' })
    await page.getByLabel('Search tables').fill('kitchen')
    await expect(list.locator('.table-row')).toHaveCount(1)
    await page.getByLabel('Search tables').fill('nothing here')
    await expect(list).toContainText('No table matches')

    // Nothing was asked of the server: filtering is this client reading rows it already has.
    expect(messages(socket.sent, 'join_room')).toHaveLength(0)
  })

  test('offers the two panels it has nothing to fill yet, saying so', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview-open.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    await expect(page.getByText('The lobby carries no chat yet.')).toBeVisible()
    // The field is drawn and cannot be typed into: there is no command to carry a message.
    await expect(page.getByLabel('Say something')).toBeDisabled()
    await page.getByRole('button', { name: 'Players' }).click()
    await expect(page.getByText('not carried on the wire yet')).toBeVisible()
  })

  test('makes a table out of the catalog the server sent', async ({ page }) => {
    const socket = await serveFrames(page, [
      fixture('lobbyview-open.json'),
      fixture('catalogview.json'),
    ])
    await open(page, 'Ada')

    await page.getByRole('button', { name: '+ Create table' }).click()
    const form = page.getByRole('dialog', { name: 'New table' })
    // Every format the server published, and no id this client made up.
    await expect(
      form.getByRole('radiogroup', { name: 'Format' }).getByRole('radio'),
    ).not.toHaveCount(0)
    await form.getByRole('button', { name: 'Create the table' }).click()

    await expect.poll(() => messages(socket.sent, 'create_room')).toHaveLength(1)
    const config = messages(socket.sent, 'create_room')[0]?.config as Record<string, unknown>
    expect(typeof config.game_setup).toBe('string')
    expect(typeof config.seats).toBe('number')
  })
})

test.describe('the table room', () => {
  test('replaces the list with the table, its rules, and its seats', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    // `lobbyview.json` is a connection that is already in a room, so the room is the screen.
    const table = page.getByRole('region', { name: 'Table' })
    await expect(table).toBeVisible()
    await expect(page.getByRole('heading', { name: /1v1|Kitchen/ })).toBeVisible()
    // The rules the table was made with, on show rather than remembered.
    await expect(page.locator('.room-facts')).toContainText('seats')
    // Chosen when the table was made, and shown where it is played — including that it is not
    // open to anyone.
    await expect(page.locator('.room-facts')).toContainText('Invite only')

    // Every seat carries its own state, on the seat rather than summarised underneath.
    await expect(table).toContainText('Practice bot')
    await expect(table).toContainText('Ready')
  })

  test('offers only what the server is currently advertising', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    // This seat is already ready, so the control offers the other direction — and what it sends
    // is the state being asked for rather than a toggle the server has to work out.
    await page.getByRole('button', { name: 'Not ready' }).click()
    await expect
      .poll(() => messages(socket.sent, 'ready'))
      .toEqual([{ type: 'ready', ready: false }])
  })

  test('chooses a deck at the table, and submits it', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    await page.getByRole('button', { name: 'Change' }).click()
    const picker = page.getByRole('dialog', { name: 'Choose a deck' })
    await expect(picker.getByRole('button', { name: /cards$/ }).first()).toBeVisible()
    await picker.getByRole('button', { name: 'Choose' }).click()

    // Choosing is submitting: the server is what says a deck is legal, and there is nothing for
    // a second button to add.
    await expect.poll(() => messages(socket.sent, 'submit_deck')).toHaveLength(1)
    const deck = messages(socket.sent, 'submit_deck')[0]?.cards as string[]
    expect(deck.length).toBeGreaterThan(0)
  })

  test('builds a deck off the seat, from the catalog', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    await page.getByRole('button', { name: 'Edit' }).click()
    const editor = page.getByRole('dialog', { name: 'Deck' })
    await expect(editor).toBeVisible()

    // One click moves one copy across, and the count says so.
    const before = await editor.locator('.edit-count').first().textContent()
    await editor.locator('.edit-row').last().click()
    await expect(editor.locator('.edit-count').first()).not.toHaveText(before ?? '')
  })
})
