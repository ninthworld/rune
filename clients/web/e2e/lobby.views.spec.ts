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

  test('reaches the deck builder from the topbar, and comes back', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview-open.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    await page.getByRole('button', { name: 'Deck Editor' }).click()
    await expect(page.getByRole('main', { name: 'Deck builder' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Open tables' })).toHaveCount(0)

    // The topbar is the screen's navigation, so it is at the top of it, and it still says who
    // this device is and where it is connected.
    const bar = page.locator('.builder-topbar')
    expect((await bar.boundingBox())?.y).toBe(0)
    await expect(bar).toContainText('Ada')

    await page.getByRole('button', { name: '← Lobby' }).click()
    await expect(page.getByRole('heading', { name: 'Open tables' })).toBeVisible()
  })

  test('puts a copy in the deck on a double click, and keeps it on the way out', async ({
    page,
  }) => {
    await serveFrames(page, [fixture('lobbyview-open.json'), fixture('catalogview.json')])
    await open(page, 'Ada')
    await page.getByRole('button', { name: 'Deck Editor' }).click()

    const deck = page.getByRole('region', { name: 'The deck' })
    await expect(deck.locator('.deck-col')).toHaveCount(0)

    const first = page.getByRole('region', { name: 'Cards' }).locator('.pool-card').first()
    await first.dblclick()
    await first.dblclick()
    // Two copies, and they are in one column because the draft counts them as one entry.
    await expect(deck.locator('.deck-col')).toHaveCount(1)
    await expect(deck.locator('.deck-stacked')).toHaveCount(2)

    // Titles draws the card's own bar, so every one of them is washed in a palette the
    // stylesheet actually holds — a hand-composed class name resolves to none and draws black.
    const pool = page.getByRole('region', { name: 'Cards' }).locator('.pool-card')
    await pool.nth(1).dblclick()
    await pool.nth(2).dblclick()
    await page.getByRole('radio', { name: 'Titles' }).click()
    const bands = deck.locator('.c-band')
    await expect(bands).toHaveCount(4)
    for (const held of await bands.all()) {
      await expect(held).toHaveClass(/card-(w|u|b|r|g|gold|c)\b/)
    }

    // Double-clicking a card in the deck is the pool's gesture read backwards.
    await page.getByRole('radio', { name: 'Stacked' }).click()
    await deck.locator('.deck-stacked').last().dblclick()
    await expect(deck.locator('.deck-stacked')).toHaveCount(3)

    // The picked card moves into the pile the options bar put beside the deck, and out of the
    // deck with it; the arrow says which way it is about to go.
    await page.getByRole('radio', { name: 'Sideboard' }).click()
    const piles = deck.getByRole('complementary', { name: 'Piles' })
    // The last card in a stack is the one lying on top of it, and the one a pointer can reach.
    await deck.locator('.deck-columns .deck-stacked').last().click()
    await piles.getByRole('button', { name: 'Here →' }).click()
    await expect(piles.locator('.deck-stacked')).toHaveCount(1)
    await expect(deck.locator('.deck-columns .deck-stacked')).toHaveCount(2)

    // Picking that copy in its new home points the same button the other way.
    await piles.locator('.deck-stacked').last().click()
    await piles.getByRole('button', { name: '← Deck' }).click()
    await expect(piles).toContainText('Nothing beside the deck.')
    await expect(deck.locator('.deck-columns .deck-stacked')).toHaveCount(3)

    // The draft is the pre-game's, not this screen's, so walking out and back keeps it — the
    // deck, and the card now beside it.
    await page.getByRole('button', { name: '← Lobby' }).click()
    await page.getByRole('button', { name: 'Deck Editor' }).click()
    await expect(deck.locator('.deck-columns .deck-stacked')).toHaveCount(3)
  })

  test('keeps a deck on this device, loads it back, and asks before forgetting it', async ({
    page,
  }) => {
    await serveFrames(page, [fixture('lobbyview-open.json'), fixture('catalogview.json')])
    await open(page, 'Ada')
    await page.getByRole('button', { name: 'Deck Editor' }).click()

    const deck = page.getByRole('region', { name: 'The deck' })
    await page.getByRole('region', { name: 'Cards' }).locator('.pool-card').first().dblclick()
    await expect(deck.locator('.deck-stacked')).toHaveCount(1)

    await page.getByRole('button', { name: 'Save…' }).click()
    const saving = page.getByRole('dialog', { name: 'Save the deck' })
    // Nothing is kept under no name.
    await expect(saving.getByRole('button', { name: 'Keep on this device' })).toBeDisabled()
    await saving.getByLabel('Deck name').fill('Elves')
    await saving.getByRole('button', { name: 'Keep on this device' }).click()

    // A fresh draft, then the kept one back off the device.
    await page.getByRole('button', { name: 'Load…' }).click()
    const loading = page.getByRole('dialog', { name: 'Load a deck' })
    await expect(loading).toContainText('Elves')
    await loading.getByRole('button', { name: 'Load' }).first().click()
    await expect(deck.locator('.deck-stacked')).toHaveCount(1)

    // Deleting is not reversible, so the row asks in place before it goes.
    await page.getByRole('button', { name: 'Load…' }).click()
    await loading.getByRole('button', { name: 'Delete' }).first().click()
    await expect(loading).toContainText('Delete for good?')
    await loading.getByRole('button', { name: 'Keep' }).click()
    await expect(loading).toContainText('Elves')

    await loading.getByRole('button', { name: 'Delete' }).first().click()
    await loading.getByRole('button', { name: 'Delete' }).first().click()
    await expect(loading).toContainText('This browser is keeping no decks yet.')
  })

  test('reads a .dck file, and says which cards this catalog does not hold', async ({ page }) => {
    await serveFrames(page, [fixture('lobbyview-open.json'), fixture('catalogview.json')])
    await open(page, 'Ada')
    await page.getByRole('button', { name: 'Deck Editor' }).click()
    await page.getByRole('button', { name: 'Load…' }).click()

    await page.getByLabel('Deck file').setInputFiles({
      name: 'burn.dck',
      mimeType: 'text/plain',
      buffer: Buffer.from(
        '[Main]\n2 Llanowar Elves|LEA\n4 Black Lotus|LEA\n\n[Sideboard]\n1 Forest\n',
      ),
    })

    const deck = page.getByRole('region', { name: 'The deck' })
    // Two Elves in the deck; the Forest is beside it, in the pile the options bar puts there.
    await expect(deck.locator('.deck-columns .deck-stacked')).toHaveCount(2)
    await page.getByRole('radio', { name: 'Sideboard' }).click()
    await expect(
      deck.getByRole('complementary', { name: 'Piles' }).locator('.deck-stacked'),
    ).toHaveCount(1)
    // The file asked for a card this catalog has no identity for, and says so by name rather
    // than handing back a deck that is quietly two cards short.
    await expect(page.getByRole('status')).toContainText('Black Lotus')
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

    // Undo is a table rule chosen here (§9.5, issue #648). A new table takes nothing back
    // until somebody asks it to, so `Not allowed` is the answer already selected.
    const undo = form.locator('.undo-field')
    await expect(undo.getByRole('radio', { name: 'Not allowed' })).toHaveAttribute(
      'aria-checked',
      'true',
    )
    await expect(undo.getByRole('radio', { name: 'Allowed', exact: true })).toHaveAttribute(
      'aria-checked',
      'false',
    )
    // The footer restates what will be made, so the rule is read where the commit is.
    await expect(form.locator('.zone-hint')).toContainText('no undo')

    await form.getByRole('button', { name: 'Create the table' }).click()

    await expect.poll(() => messages(socket.sent, 'create_room')).toHaveLength(1)
    const config = messages(socket.sent, 'create_room')[0]?.config as Record<string, unknown>
    expect(typeof config.game_setup).toBe('string')
    expect(typeof config.seats).toBe('number')
    // Off is the default and elides: the table that is made is the one the wire describes.
    expect(config.undo_enabled).toBeUndefined()
  })

  test('sends the undo rule only when the table was made with it', async ({ page }) => {
    const socket = await serveFrames(page, [
      fixture('lobbyview-open.json'),
      fixture('catalogview.json'),
    ])
    await open(page, 'Ada')

    await page.getByRole('button', { name: '+ Create table' }).click()
    const form = page.getByRole('dialog', { name: 'New table' })
    await form.locator('.undo-field').getByRole('radio', { name: 'Allowed', exact: true }).click()
    await expect(form.locator('.zone-hint')).toContainText('undo allowed')
    await form.getByRole('button', { name: 'Create the table' }).click()

    await expect.poll(() => messages(socket.sent, 'create_room')).toHaveLength(1)
    const config = messages(socket.sent, 'create_room')[0]?.config as Record<string, unknown>
    expect(config.undo_enabled).toBe(true)
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
    // A rule that changes how the game plays is drawn in its own colour beside the words that
    // say the same thing (§9.5, issue #648). This table was made without undo, and the strip
    // says so from `RoomConfig` rather than from any client assumption.
    const facts = page.locator('.room-facts')
    await expect(facts).toContainText('No undo')
    await expect(facts).not.toContainText('Undo allowed')
    await expect(facts.locator('.fact-off')).toHaveText('No undo')

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

  test('shows what every seat brought: its colours, and the commander it named', async ({
    page,
  }) => {
    await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    // Another seat's deck is the server's summary of it — the colours it is in and the card
    // it is built around — drawn from `SeatView`, not from anything on this device.
    const theirs = page.getByRole('region', { name: 'Table' }).locator('.seat').nth(1)
    await expect(theirs.locator('.seat-cmdr .card')).toBeVisible()
    await expect(theirs).toContainText('Serra Angel')
    await expect(theirs.locator('.seat-colors .c-pip')).toHaveCount(1)
  })

  test('seats a bot with the deck it was given, commander and all', async ({ page }) => {
    // The same table with a seat still open, so there is a hole to seat a bot in.
    const view = fixture('lobbyview.json')
    const room = view.room as { config: { seats: number } }
    const socket = await serveFrames(page, [
      { ...view, room: { ...room, config: { ...room.config, seats: 3 } } },
      fixture('catalogview.json'),
    ])
    await open(page, 'Ada')

    await page.getByRole('button', { name: 'Seat an AI opponent' }).click()
    const picker = page.getByRole('dialog', { name: 'Seat an AI opponent' })
    await picker.getByRole('button', { name: /Dragon Sovereign/ }).click()
    await picker.getByRole('button', { name: 'Seat' }).click()

    // A deck that names a commander is seated with it: dropping the designation would seat a
    // commander deck as an ordinary one, and the table would show no commander for that seat.
    await expect.poll(() => messages(socket.sent, 'add_ai')).toHaveLength(1)
    expect(messages(socket.sent, 'add_ai')[0]?.commander).toBe('lathliss_dragon_queen')
  })

  test('chooses a deck at the table, and submits it', async ({ page }) => {
    const socket = await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    // The seat opens the same chooser the deck editor loads from: this device's decks, the
    // starters, and a file.
    await page.getByRole('button', { name: 'Change' }).click()
    const picker = page.getByRole('dialog', { name: 'Load a deck' })
    await expect(picker).toContainText('Starter decks')
    await picker.getByRole('button', { name: 'Load' }).first().click()

    // Choosing is submitting: the server is what says a deck is legal, and there is nothing for
    // a second button to add.
    await expect.poll(() => messages(socket.sent, 'submit_deck')).toHaveLength(1)
    const deck = messages(socket.sent, 'submit_deck')[0]?.cards as string[]
    expect(deck.length).toBeGreaterThan(0)
  })

  test('moves a card across the sideboard line at the seat, and reaches the builder', async ({
    page,
  }) => {
    const socket = await serveFrames(page, [fixture('lobbyview.json'), fixture('catalogview.json')])
    await open(page, 'Ada')

    // The bigger changes are one button away, on the screen built for them.
    await page.getByRole('button', { name: 'Edit' }).click()
    const editor = page.getByRole('dialog', { name: 'Deck' })
    await editor.getByRole('button', { name: 'Deck editor…' }).click()
    await expect(page.getByRole('main', { name: 'Deck builder' })).toBeVisible()

    await page.getByRole('region', { name: 'Cards' }).locator('.pool-card').first().dblclick()

    // Leaving returns to the table it was opened from, and the deck goes with it.
    await page.getByRole('button', { name: '← Table' }).click()
    await expect(page.getByRole('region', { name: 'Table' })).toBeVisible()
    await expect.poll(() => messages(socket.sent, 'submit_deck')).toHaveLength(1)

    await page.getByRole('button', { name: 'Edit' }).click()
    // Rows are the cards' own title bars, as the builder's Titles view draws them.
    await expect(editor.locator('.edit-row .c-band').first()).toBeVisible()

    // One click sends a copy across the line, and both counts say so.
    const deckCount = editor.locator('.edit-count').first()
    await expect(deckCount).toHaveText('1')
    await editor.locator('.edit-row').first().click()
    await expect(deckCount).toHaveText('0')
    await expect(editor.locator('.edit-count').last()).toHaveText('1')

    // The pane beside the deck is either pile, one at a time.
    await editor.getByRole('radio', { name: 'Commander' }).click()
    await expect(editor).toContainText('No commander designated.')
    await editor.getByRole('radio', { name: 'Sideboard' }).click()
    await expect(editor.locator('.edit-row .c-band')).toHaveCount(1)

    // Submitting sends the deck without the cards beside it — the wire has no sideboard.
    await editor.getByRole('button', { name: 'Submit deck' }).click()
    await expect.poll(() => messages(socket.sent, 'submit_deck')).toHaveLength(2)
    expect(messages(socket.sent, 'submit_deck')[1]?.cards).toEqual([])
  })
})
