/**
 * The pre-game screen, driven by committed fixtures over an intercepted WebSocket.
 *
 * Everything before a game exists: the table directory, creating and editing a table, the seat
 * roster of the table you are at, and building a deck out of the server's own catalog. Nothing
 * here is a game — the frames are a `LobbyView`, a `CatalogView`, and a `lobby_error`, which is
 * the whole of what the lobby contract carries.
 *
 * The recurring assertion is not "the button is there", it is **what the client sent**. Every
 * control in this screen exists because the server advertised its command, and the value it
 * sends has to be the one the protocol specifies — a decklist as a flat list with duplicates
 * repeated, an edit as a whole config rather than a patch. A screen that looks right and sends
 * the wrong frame is the failure this tier is for.
 *
 * The **non-blocking** tier (ADR 0011); the one blocking path is `smoke.spec.ts`.
 */
import { expect, test } from '@playwright/test'

import { DESKTOP, fixture, messages, serveFrames } from './frames'

const OPEN = () => fixture('lobbyview-open.json')
const SEATED = () => fixture('lobbyview.json')
const CATALOG = () => fixture('catalogview.json')

test.use({ viewport: DESKTOP })

test.describe('the table directory', () => {
  test('lists every browsable room with its occupancy, state, and id', async ({ page }) => {
    await serveFrames(page, [OPEN()])
    await page.goto('/')

    const rows = page.getByRole('region', { name: 'Tables' }).getByRole('listitem')
    await expect(rows).toHaveCount(3)

    // A named table reads as its name; an unnamed one falls back to its format, because the
    // server never invents a name and this client will not either.
    await expect(rows.nth(0)).toContainText('Kitchen table')
    await expect(rows.nth(0)).toContainText('1/2 seats')
    await expect(rows.nth(0)).toContainText('1 open')
    await expect(rows.nth(1)).toContainText('starter-1v1')
    await expect(rows.nth(1)).toContainText('2/2 seats')

    // A running room advertises its watchers, and the id every row carries is what a private
    // table is reached by.
    await expect(rows.nth(2)).toContainText('In progress')
    await expect(rows.nth(2)).toContainText('3 watching')
    await expect(rows.nth(2)).toContainText('r_312')
  })

  test('joins a table with a seat and watches one without', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN()])
    await page.goto('/')

    const rows = page.getByRole('region', { name: 'Tables' }).getByRole('listitem')
    await rows.nth(0).getByRole('button', { name: 'Join' }).click()
    expect(messages(sent, 'join_room')).toEqual([{ type: 'join_room', room_id: 'r_310' }])

    // The full table is still gathering, so it is not joinable — but spectating consumes no
    // seat, and the server advertised `spectate_room`, so that is what the row offers.
    await rows.nth(1).getByRole('button', { name: 'Watch' }).click()
    await rows.nth(2).getByRole('button', { name: 'Watch' }).click()
    expect(messages(sent, 'spectate_room')).toEqual([
      { type: 'spectate_room', room_id: 'r_311' },
      { type: 'spectate_room', room_id: 'r_312' },
    ])
  })

  test('reaches an unlisted table by the id its host shared', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN()])
    await page.goto('/')

    await page.getByLabel('Table id').fill('r_999')
    await page.getByRole('button', { name: 'Join by id' }).click()
    expect(messages(sent, 'join_room')).toEqual([{ type: 'join_room', room_id: 'r_999' }])
  })

  test('offers no control the server did not advertise', async ({ page }) => {
    // The same directory, sent to a connection offered neither command. Every row still
    // informs; none of them acts.
    await serveFrames(page, [{ ...OPEN(), valid_commands: ['set_name'] }])
    await page.goto('/')

    const directory = page.getByRole('region', { name: 'Tables' })
    await expect(directory.getByRole('listitem')).toHaveCount(3)
    await expect(directory.getByRole('button')).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Create a table' })).toHaveCount(0)
  })
})

test.describe('creating a table', () => {
  test('offers the formats and seat counts the catalog advertised', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN(), CATALOG()])
    await page.goto('/')

    // The catalog is asked for per socket; nothing about a format is hardcoded in this client.
    await expect.poll(() => messages(sent, 'request_catalog').length).toBe(1)

    await page.getByRole('button', { name: 'Create a table' }).click()
    const format = page.getByLabel('Format')
    // Sorted by id: the catalog lists them in the other order, and the wire promises none.
    await expect(format.getByRole('option')).toHaveText(['commander', 'starter-1v1'])

    // Commander advertises 2–4 seats, and its published rules ride alongside.
    await expect(page.getByLabel('Seats').getByRole('option')).toHaveText(['2', '3', '4'])
    await expect(page.getByText('A commander must be designated')).toBeVisible()
    // Switching to a format with a narrower range narrows the control to what it allows.
    await format.selectOption('starter-1v1')
    await expect(page.getByLabel('Seats').getByRole('option')).toHaveText(['2'])
    await expect(page.getByText('A commander must be designated')).toHaveCount(0)
  })

  test('sends the whole config, with the defaults left off the wire', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Create a table' }).click()
    await page.getByLabel('Format').selectOption('starter-1v1')
    await page.getByRole('button', { name: 'Create the table' }).click()
    // An unnamed public table is the pre-#546 shape: neither field rides the wire.
    expect(messages(sent, 'create_room')).toEqual([
      { type: 'create_room', config: { seats: 2, game_setup: 'starter-1v1' } },
    ])
  })

  test('carries a name and a private visibility when they are chosen', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Create a table' }).click()
    await page.getByLabel('Format').selectOption('commander')
    await page.getByLabel('Seats').selectOption('4')
    await page.getByLabel('Table name').fill('Casual Commander')
    await page.getByLabel('Private table').check()
    await page.getByRole('button', { name: 'Create the table' }).click()

    expect(messages(sent, 'create_room')).toEqual([
      {
        type: 'create_room',
        config: {
          seats: 4,
          game_setup: 'commander',
          name: 'Casual Commander',
          visibility: 'private',
        },
      },
    ])
  })

  test('offers no format at all when no catalog has arrived', async ({ page }) => {
    // Honest rather than hopeful: a client that has not been handed the format list guesses
    // no `game_setup` id, because a guessed one is a table the server refuses to create.
    await serveFrames(page, [OPEN()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Create a table' }).click()
    await expect(page.getByText(/Waiting for the server’s format list/)).toBeVisible()
    await expect(page.getByRole('button', { name: 'Create the table' })).toHaveCount(0)
  })
})

test.describe('the table you are at', () => {
  test('shows the roster, the flags each seat reported, and what is still owed', async ({
    page,
  }) => {
    await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    const table = page.getByRole('region', { name: 'Table', exact: true })
    await expect(table.getByRole('heading', { name: 'Kitchen table' })).toBeVisible()
    await expect(table).toContainText('r_204')
    await expect(table).toContainText('private')

    const seats = page.getByRole('list').filter({ hasText: 'Seat 1' }).first().getByRole('listitem')
    await expect(seats.nth(0)).toContainText('Ari (you)')
    await expect(seats.nth(0)).toContainText('Deck submitted')
    await expect(seats.nth(0)).toContainText('Ready')

    // The AI seat in this fixture is decked but reports no readiness, and that is shown as
    // sent — the wait is stated, not guessed at.
    await expect(seats.nth(1)).toContainText('Practice bot')
    await expect(seats.nth(1)).toContainText('AI')
    await expect(seats.nth(1)).toContainText('Not ready')
    await expect(page.getByText('Waiting on — Seat 2 — Not ready')).toBeVisible()
  })

  test('edits the table as a whole config, opened with the one it has', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    // `update_room` is advertised, so Edit Table exists. The client never decides it is host.
    await page.getByRole('button', { name: 'Edit table' }).click()
    await expect(page.getByLabel('Table name')).toHaveValue('Kitchen table')
    await expect(page.getByLabel('Private table')).toBeChecked()

    await page.getByLabel('Table name').fill('Kitchen table II')
    await page.getByRole('button', { name: 'Save the table' }).click()
    expect(messages(sent, 'update_room')).toEqual([
      {
        type: 'update_room',
        config: {
          seats: 2,
          game_setup: 'starter-1v1',
          name: 'Kitchen table II',
          visibility: 'private',
        },
      },
    ])
  })

  test('seats a bot from the kinds the catalog advertised', async ({ page }) => {
    // A room with one open seat, and the AI commands advertised for it.
    const view = SEATED() as { room: { seats: unknown[] } }
    view.room.seats = [{ seat: 0, occupied_by: 'p0', name: 'Ari', decked: true }, { seat: 1 }]
    const { sent } = await serveFrames(page, [view, CATALOG()])
    await page.goto('/')

    await expect(page.getByLabel('Opponent').getByRole('option')).toHaveText(['Practice bot'])
    await page.getByRole('button', { name: 'Seat an AI opponent' }).click()

    const seated = messages(sent, 'add_ai')
    expect(seated).toHaveLength(1)
    expect(seated[0]).toMatchObject({ type: 'add_ai', seat: 1, kind: 'random' })
    // The bot's deck is a real decklist, validated by the server exactly like a human's.
    expect((seated[0]!.cards as string[]).length).toBeGreaterThan(0)
  })

  test('readies up and leaves through the commands the server offered', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Ready' }).click()
    expect(messages(sent, 'ready')).toEqual([{ type: 'ready', ready: true }])
    await page.getByRole('button', { name: 'Leave' }).click()
    expect(messages(sent, 'leave')).toEqual([{ type: 'leave' }])

    // The same seat with `unready` advertised instead gets the opposite control, and sends the
    // opposite value — neither is inferred from the seat's own flag.
    await serveFrames(page, [{ ...SEATED(), valid_commands: ['unready', 'leave'] }])
    await page.goto('/')
    await expect(page.getByRole('button', { name: 'Ready', exact: true })).toHaveCount(0)
    await page.getByRole('button', { name: 'Not ready' }).click()
  })
})

test.describe('building a deck', () => {
  test('takes a starter deck in one step and submits it as a flat list', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    await expect(page.getByText('No deck yet.')).toBeVisible()
    await page.getByLabel('Starter deck').selectOption({ index: 1 })

    const deck = page.getByRole('region', { name: 'Deck', exact: true })
    await expect(deck).toContainText('cards')
    await page.getByRole('button', { name: /^Submit deck/ }).click()

    const submitted = messages(sent, 'submit_deck')
    expect(submitted).toHaveLength(1)
    const cards = submitted[0]!.cards as string[]
    // Duplicates repeated, as `submit_deck` specifies — the counted view is the client's.
    expect(cards.length).toBeGreaterThan(new Set(cards).size)
    // No commander is designated for a non-commander format, so the field stays off the wire.
    expect(submitted[0]).not.toHaveProperty('commander')
  })

  test('quotes the format rules the catalog published', async ({ page }) => {
    await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    const deck = page.getByRole('region', { name: 'Deck', exact: true })
    await expect(deck).toContainText('At least 40 cards')
    await expect(deck).toContainText('At most 4 copies of a card, basic lands exempt')
  })

  test('searches the pool and edits the deck a copy at a time', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Build a deck' }).click()
    const builder = page.getByRole('region', { name: 'Deck builder' })
    await expect(builder.getByRole('button', { name: /^Serra Angel/ })).toBeVisible()

    await page.getByLabel('Search cards').fill('angel')
    await expect(builder.getByText('1 card.')).toBeVisible()
    await expect(builder.getByRole('button', { name: /^Llanowar Elves/ })).toHaveCount(0)

    // Adding twice is allowed; whether two copies are legal is not this client's question.
    await builder.getByRole('button', { name: 'Add', exact: true }).click()
    await builder.getByRole('button', { name: /^Add \(1\)/ }).click()
    await expect(builder.getByText('2×')).toBeVisible()
    await builder.getByRole('button', { name: 'Remove a copy of Serra Angel' }).click()
    await expect(builder.getByText('1×')).toBeVisible()

    // The keyword filter is built from the keywords the cards themselves state.
    await page.getByLabel('Search cards').fill('')
    await page.getByLabel('Keyword').selectOption('flying')
    await expect(builder.getByText('1 card.')).toBeVisible()

    await page.getByRole('button', { name: /^Submit deck/ }).click()
    expect(messages(sent, 'submit_deck')[0]!.cards).toEqual(['serra_angel'])
  })

  test('advises on deck size without ever standing in the way of a submission', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Build a deck' }).click()
    await page
      .getByRole('region', { name: 'Deck builder' })
      .getByRole('button', { name: 'Add', exact: true })
      .first()
      .click()

    // The advice is arithmetic on two numbers the server sent. It is advice: the submit
    // control is offered because `submit_deck` is advertised, and a deck the client has
    // something to say about goes to the server anyway, which is where the verdict is.
    await expect(page.getByText('39 short of the 40-card minimum').first()).toBeVisible()
    await page.getByRole('button', { name: /^Submit deck/ }).click()
    expect(messages(sent, 'submit_deck')).toHaveLength(1)
  })

  test('designates a commander only where the format asks for one', async ({ page }) => {
    const commanderRoom = SEATED() as { room: { config: { game_setup: string } } }
    commanderRoom.room.config.game_setup = 'commander'
    const { sent } = await serveFrames(page, [commanderRoom, CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Build a deck' }).click()
    const builder = page.getByRole('region', { name: 'Deck builder' })
    await builder.getByRole('button', { name: 'Add', exact: true }).first().click()
    await builder.getByRole('button', { name: 'Commander' }).click()

    await expect(page.getByRole('region', { name: 'Deck', exact: true })).toContainText(
      'commander Llanowar Elves',
    )
    await page.getByRole('button', { name: /^Submit deck/ }).click()
    expect(messages(sent, 'submit_deck')).toEqual([
      { type: 'submit_deck', cards: ['llanowar_elves'], commander: 'llanowar_elves' },
    ])
  })

  test('shows the server’s rejection and keeps the deck that was refused', async ({ page }) => {
    const { push } = await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Build a deck' }).click()
    await page
      .getByRole('region', { name: 'Deck builder' })
      .getByRole('button', { name: 'Add', exact: true })
      .first()
      .click()
    await page.getByRole('button', { name: /^Submit deck/ }).click()

    await push({
      lobby_error: {
        code: 'below_minimum',
        reason: 'deck has 1 card, below the 40-card minimum',
        card: 'llanowar_elves',
      },
    })

    // The server's own wording, verbatim, with the identity it named resolved to a card name.
    const alert = page.getByRole('alert')
    await expect(alert).toContainText('deck has 1 card, below the 40-card minimum')
    await expect(alert).toContainText('Llanowar Elves')
    // And the builder keeps what was refused, so it can be corrected and sent again.
    await expect(page.getByRole('region', { name: 'Deck builder' })).toContainText('Llanowar Elves')
  })

  test('opens the full face of a card from the pool and from the deck', async ({ page }) => {
    await serveFrames(page, [SEATED(), CATALOG()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Build a deck' }).click()
    const builder = page.getByRole('region', { name: 'Deck builder' })

    // The same inspector the game table opens, over the same face model.
    await builder.getByRole('button', { name: /^Serra Angel/ }).click()
    const inspector = page.getByRole('dialog', { name: /Serra Angel/ })
    await expect(inspector).toContainText('Creature — Angel')
    await expect(inspector).toContainText('flying · vigilance')
    await inspector.getByRole('button', { name: 'Close' }).click()

    await builder.getByRole('button', { name: /^Add/ }).first().click()
    await builder.getByRole('button', { name: 'Llanowar Elves', exact: true }).click()
    await expect(page.getByRole('dialog', { name: /Llanowar Elves/ })).toContainText(
      '{T}: Add {G}.',
    )
  })
})
