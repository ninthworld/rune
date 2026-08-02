/**
 * The client shell before a game: connect, the rail, the tables, the table you are at, the deck.
 * Driven by committed fixtures over an intercepted WebSocket — no server, no engine, no game.
 *
 * The recurring assertion is not "the button is there", it is **what the client sent**. Every
 * control in this screen exists because the server advertised its command, and the value it
 * sends has to be the one the protocol specifies — a decklist as a flat list with duplicates
 * repeated, an edit as a whole config rather than a patch. A screen that looks right and sends
 * the wrong frame is the failure this tier is for.
 *
 * Two things `docs/client-design.md` §9 added are asserted here rather than eyeballed. **Which
 * destination you are on is the client's answer and which contract you are on is the server's**:
 * walking to Decks and back must change nothing about the connection, and a `GameView` arriving
 * must replace the whole shell. And **the composition holds at every supported size** — the last
 * lobby broke at 120% zoom on the maintainer's own screen, which is a 1067×600 layout viewport,
 * so the sweep at the bottom of this file measures every piece of text against the box it was
 * given at five viewports rather than at the one desktop this was built on.
 *
 * The **non-blocking** tier (ADR 0011); the one blocking path is `smoke.spec.ts`.
 */
import { expect, test, type Page } from '@playwright/test'

import { DESKTOP, fixture, messages, open, pageFits, serveFrames } from './frames'

const OPEN = () => fixture('lobbyview-open.json')
const SEATED = () => fixture('lobbyview.json')
const CATALOG = () => fixture('catalogview.json')

test.use({ viewport: DESKTOP })

test.describe('connect', () => {
  test('asks who you are and where, then sends the name as the command it always was', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [OPEN()])
    await page.goto('/')

    // The server list is client-side configuration and every entry carries where it is. There
    // is no published server yet, so what is really reachable is this device and an address you
    // type — which is the honest list rather than a short one.
    const servers = page.getByRole('radiogroup', { name: 'Server' })
    await expect(servers.getByRole('radio')).toHaveText([/Localhost/, /Another address/])
    await expect(servers).toContainText('This device')

    await page.getByLabel('Name').fill('Ari')
    await page.getByRole('button', { name: 'Connect' }).click()

    // Neither field is a wire change: the name is `set_name`, and `hello` still carries a token
    // and nothing else.
    await expect(page.getByRole('navigation', { name: 'Destinations' })).toBeVisible()
    await expect.poll(() => messages(sent, 'set_name')).toEqual([{ type: 'set_name', name: 'Ari' }])
    expect(messages(sent, 'hello')).toEqual([{ type: 'hello' }])
  })

  test('opens on the name this device used last', async ({ page }) => {
    // Written by a previous visit, in the manner of ADR 0012's art preference: device-local,
    // never sent, and a returning player presses one key.
    await page.addInitScript(() =>
      window.localStorage.setItem('sage.connect.v1', JSON.stringify({ name: 'Ari' })),
    )
    await serveFrames(page, [OPEN()])
    await page.goto('/')

    await expect(page.getByLabel('Name')).toHaveValue('Ari')
  })

  test('takes an address the list does not carry', async ({ page }) => {
    await serveFrames(page, [OPEN()])
    await page.goto('/')

    await page.getByRole('radio', { name: 'Another address' }).click()
    // Nothing to connect to until an address is typed, and the control says so by being off.
    await expect(page.getByRole('button', { name: 'Connect' })).toBeDisabled()
    await page.getByLabel('Address').fill('ws://127.0.0.1:9000')
    await expect(page.getByRole('button', { name: 'Connect' })).toBeEnabled()
  })

  test('reaches settings before ever joining a table', async ({ page }) => {
    await serveFrames(page, [OPEN()])
    await page.goto('/')

    // §9.3: the gear is here so card art can be set up before a game exists.
    await page.getByRole('button', { name: 'Settings' }).click()
    await expect(page.getByRole('heading', { name: 'Card art' })).toBeVisible()
    await page.getByRole('button', { name: 'Back' }).click()
    await expect(page.getByRole('button', { name: 'Connect' })).toBeVisible()
  })
})

test.describe('the shell', () => {
  test('carries the three destinations, in order, and marks the one in force', async ({ page }) => {
    await serveFrames(page, [OPEN(), CATALOG()])
    await open(page, 'Ari')

    const rail = page.getByRole('navigation', { name: 'Destinations' })
    await expect(rail.getByRole('button')).toHaveText([/Play/, /Decks/, /Settings/])
    // Stated, not only drawn: an accent on a button is not a fact a screen reader can read.
    await expect(rail.getByRole('button', { name: 'Play' })).toHaveAttribute('aria-current', 'page')
    await expect(rail).toContainText('Ari')
  })

  test('walks to another destination without disturbing the connection', async ({ page }) => {
    const { sent, sockets } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page, 'Ari')

    const before = sent.length
    await page.getByRole('button', { name: 'Decks' }).click()
    await expect(page.getByRole('region', { name: 'Deck builder' })).toBeVisible()

    // Choosing a destination is the client's answer and nothing else: no frame is sent, no
    // socket is opened, and the table the server says you are at is still the table.
    expect(sent.length).toBe(before)
    expect(sockets.length).toBe(1)
    await page.getByRole('button', { name: 'Play' }).click()
    await expect(page.getByRole('heading', { name: 'Kitchen table' })).toBeVisible()
  })

  test('is replaced whole when the server changes the contract', async ({ page }) => {
    const { push } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page, 'Ari')
    await expect(page.getByRole('navigation', { name: 'Destinations' })).toBeVisible()

    await push(fixture('gameview.json'))

    // The contract changed, so the shell goes — destination and all. That is the half of §9.4
    // the client does not get a say in.
    await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()
    await expect(page.getByRole('navigation', { name: 'Destinations' })).toHaveCount(0)
  })
})

test.describe('the tables list', () => {
  test('is what the table is, how full it is, and one button', async ({ page }) => {
    await serveFrames(page, [OPEN()])
    await open(page)

    const rows = page.getByRole('region', { name: 'Tables' }).getByRole('listitem')
    await expect(rows).toHaveCount(3)

    // A named table reads as its name; an unnamed one falls back to its format, because the
    // server never invents a name and this client will not either.
    await expect(rows.nth(0)).toContainText('Kitchen table')
    await expect(rows.nth(1)).toContainText('starter-1v1')

    // How full is drawn as pips and stated as a count; the fraction is nowhere in the text.
    await expect(rows.nth(0).getByLabel('1 of 2 seats taken')).toBeVisible()
    await expect(rows.nth(1).getByLabel('2 of 2 seats taken')).toBeVisible()
    await expect(rows.nth(2)).toContainText('In progress')
    await expect(rows.nth(2)).toContainText('3 watching')

    // One button per row, and no room id anywhere in the list: §9.2 rule 3, a player has no use
    // for an identifier they never type.
    for (const index of [0, 1, 2]) await expect(rows.nth(index).getByRole('button')).toHaveCount(1)
    await expect(page.getByRole('region', { name: 'Tables' })).not.toContainText('r_31')
  })

  test('joins a table with a seat and watches one without', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN()])
    await open(page)

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
    await open(page)

    await page.getByLabel('Table id').fill('r_999')
    await page.getByRole('button', { name: 'Join by id' }).click()
    expect(messages(sent, 'join_room')).toEqual([{ type: 'join_room', room_id: 'r_999' }])
  })

  test('offers no control the server did not advertise', async ({ page }) => {
    // The same directory, sent to a connection offered neither command. Every row still
    // informs; none of them acts.
    await serveFrames(page, [{ ...OPEN(), valid_commands: ['set_name'] }])
    await open(page)

    const directory = page.getByRole('region', { name: 'Tables' })
    await expect(directory.getByRole('listitem')).toHaveCount(3)
    await expect(directory.getByRole('button')).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'New table' })).toHaveCount(0)
  })
})

test.describe('creating a table', () => {
  test('offers the formats and seat counts the catalog advertised', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN(), CATALOG()])
    await open(page)

    // The catalog is asked for per socket; nothing about a format is hardcoded in this client.
    await expect.poll(() => messages(sent, 'request_catalog').length).toBe(1)

    await page.getByRole('button', { name: 'New table' }).click()
    const format = page.getByRole('radiogroup', { name: 'Format' })
    // Sorted by id: the catalog lists them in the other order, and the wire promises none.
    await expect(format.getByRole('radio')).toHaveText([/^commander/, /^starter-1v1/])
    // What a format requires rides on the format's own option, where a player choosing between
    // two of them can read both without holding one in their head.
    await expect(format).toContainText('A commander must be designated')

    // Commander advertises 2–4 seats, and the control offers exactly those.
    const seats = page.getByRole('radiogroup', { name: 'Seats' })
    await expect(seats.getByRole('radio')).toHaveText(['2', '3', '4'])
    // Switching to a format with a narrower range narrows the control to what it allows.
    await format.getByRole('radio', { name: /^starter-1v1/ }).click()
    await expect(seats.getByRole('radio')).toHaveText(['2'])
  })

  test('sends the whole config, with the defaults left off the wire', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'New table' }).click()
    await page
      .getByRole('radiogroup', { name: 'Format' })
      .getByRole('radio', { name: /^starter-1v1/ })
      .click()
    await page.getByRole('button', { name: 'Create the table' }).click()
    // An unnamed public table is the pre-#546 shape: neither field rides the wire.
    expect(messages(sent, 'create_room')).toEqual([
      { type: 'create_room', config: { seats: 2, game_setup: 'starter-1v1' } },
    ])
  })

  test('carries a name and a private visibility when they are chosen', async ({ page }) => {
    const { sent } = await serveFrames(page, [OPEN(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'New table' }).click()
    await page
      .getByRole('radiogroup', { name: 'Format' })
      .getByRole('radio', { name: /^commander/ })
      .click()
    await page.getByRole('radiogroup', { name: 'Seats' }).getByRole('radio', { name: '4' }).click()
    await page.getByLabel('Table name').fill('Casual Commander')
    await page.getByRole('radio', { name: /^Private/ }).click()
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
    await open(page)

    await page.getByRole('button', { name: 'New table' }).click()
    await expect(page.getByText(/Waiting for the server’s format list/)).toBeVisible()
    await expect(page.getByRole('button', { name: 'Create the table' })).toHaveCount(0)
  })
})

test.describe('the table you are at', () => {
  test('draws what each seat owes on that seat, and nowhere else', async ({ page }) => {
    await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    const table = page.getByRole('region', { name: 'Table', exact: true })
    await expect(table.getByRole('heading', { name: 'Kitchen table' })).toBeVisible()
    // A private table is the one place a room id has a use: it is what its host passes on.
    await expect(table).toContainText('private')
    await expect(table).toContainText('r_204')

    const seats = page.getByRole('region', { name: 'Seats' }).getByRole('listitem')
    await expect(seats.nth(0)).toContainText('Ari')
    await expect(seats.nth(0)).toContainText('Deck submitted')
    await expect(seats.nth(0)).toContainText('Ready')

    // The AI seat is decked but reports no readiness, and that is shown as sent — on the seat,
    // as an unlit mark, rather than as a sentence underneath restating the list above it.
    await expect(seats.nth(1)).toContainText('Practice bot')
    await expect(seats.nth(1)).toContainText('AI')
    await expect(seats.nth(1)).toContainText('Not ready')
    await expect(page.getByText('Waiting on —')).toHaveCount(0)
  })

  test('edits the table as a whole config, opened with the one it has', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    // `update_room` is advertised, so Edit table exists. The client never decides it is host.
    await page.getByRole('button', { name: 'Edit table' }).click()
    await expect(page.getByLabel('Table name')).toHaveValue('Kitchen table')
    await expect(page.getByRole('radio', { name: /^Private/ })).toHaveAttribute(
      'aria-checked',
      'true',
    )

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
    await open(page)

    // The choice opens on the seat being filled, and what each kind plays like is on the
    // option — not printed beside a control forever.
    await page.getByRole('button', { name: 'Seat an AI opponent' }).click()
    const opponent = page.getByRole('radiogroup', { name: 'Opponent' })
    await expect(opponent.getByRole('radio')).toHaveText([/Practice bot/])
    await expect(opponent).toContainText('Picks a legal action at random')
    await page.getByRole('button', { name: 'Seat', exact: true }).click()

    const seated = messages(sent, 'add_ai')
    expect(seated).toHaveLength(1)
    expect(seated[0]).toMatchObject({ type: 'add_ai', seat: 1, kind: 'random' })
    // The bot's deck is a real decklist, validated by the server exactly like a human's.
    expect((seated[0]!.cards as string[]).length).toBeGreaterThan(0)
  })

  test('readies up and leaves through the commands the server offered', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Ready' }).click()
    expect(messages(sent, 'ready')).toEqual([{ type: 'ready', ready: true }])
    await page.getByRole('button', { name: 'Leave' }).click()
    expect(messages(sent, 'leave')).toEqual([{ type: 'leave' }])

    // The same seat with `unready` advertised instead gets the opposite control, and sends the
    // opposite value — neither is inferred from the seat's own flag.
    await serveFrames(page, [{ ...SEATED(), valid_commands: ['unready', 'leave'] }])
    await open(page)
    await expect(page.getByRole('button', { name: 'Ready', exact: true })).toHaveCount(0)
    await page.getByRole('button', { name: 'Not ready' }).click()
  })
})

test.describe('the deck', () => {
  test('takes a starter deck in one step and submits it as a flat list', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    const deck = page.getByRole('region', { name: 'Deck', exact: true })
    await expect(deck).toContainText('0')
    await page.getByRole('button', { name: 'Starter deck' }).click()
    await page.getByRole('option').first().click()

    await expect(deck).toContainText('different')
    await page.getByRole('button', { name: 'Submit deck' }).click()

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
    await open(page)

    const deck = page.getByRole('region', { name: 'Deck', exact: true })
    await expect(deck).toContainText('At least 40 cards')
    await expect(deck).toContainText('At most 4 copies of a card, basic lands exempt')
  })

  test('reaches the builder as a destination, and comes back', async ({ page }) => {
    await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Build a deck' }).click()
    await expect(page.getByRole('region', { name: 'Deck builder' })).toBeVisible()
    // The rail agrees: the builder is a destination, not a panel that unfolded.
    await expect(page.getByRole('button', { name: 'Decks' })).toHaveAttribute(
      'aria-current',
      'page',
    )
    await page.getByRole('button', { name: 'Done' }).click()
    await expect(page.getByRole('heading', { name: 'Kitchen table' })).toBeVisible()
  })

  test('searches the pool and edits the deck a copy at a time', async ({ page }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Decks' }).click()
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

    // The keyword filter is built from the keywords the cards themselves state, and it is the
    // shell's own listbox rather than a `<select>` that clips its arrow at 120% zoom.
    await page.getByLabel('Search cards').fill('')
    await page.getByRole('button', { name: 'Keyword' }).click()
    await page.getByRole('option', { name: 'flying' }).click()
    await expect(builder.getByText('1 card.')).toBeVisible()

    await page.getByRole('button', { name: 'Play' }).click()
    await page.getByRole('button', { name: 'Submit deck' }).click()
    expect(messages(sent, 'submit_deck')[0]!.cards).toEqual(['serra_angel'])
  })

  test('advises on deck size without ever standing in the way of a submission', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Decks' }).click()
    await page
      .getByRole('region', { name: 'Deck builder' })
      .getByRole('button', { name: 'Add', exact: true })
      .first()
      .click()

    // The advice is arithmetic on two numbers the server sent. It is advice: the submit
    // control is offered because `submit_deck` is advertised, and a deck the client has
    // something to say about goes to the server anyway, which is where the verdict is.
    await expect(page.getByText('39 short of the 40-card minimum').first()).toBeVisible()
    await page.getByRole('button', { name: 'Play' }).click()
    await page.getByRole('button', { name: 'Submit deck' }).click()
    expect(messages(sent, 'submit_deck')).toHaveLength(1)
  })

  test('designates a commander only where the format asks for one', async ({ page }) => {
    const commanderRoom = SEATED() as { room: { config: { game_setup: string } } }
    commanderRoom.room.config.game_setup = 'commander'
    const { sent } = await serveFrames(page, [commanderRoom, CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Decks' }).click()
    const builder = page.getByRole('region', { name: 'Deck builder' })
    await builder.getByRole('button', { name: 'Add', exact: true }).first().click()
    await builder.getByRole('button', { name: 'Commander' }).click()

    await page.getByRole('button', { name: 'Play' }).click()
    await expect(page.getByRole('region', { name: 'Deck', exact: true })).toContainText(
      'Llanowar Elves',
    )
    await page.getByRole('button', { name: 'Submit deck' }).click()
    expect(messages(sent, 'submit_deck')).toEqual([
      { type: 'submit_deck', cards: ['llanowar_elves'], commander: 'llanowar_elves' },
    ])
  })

  test('shows the server’s rejection and keeps the deck that was refused', async ({ page }) => {
    const { push } = await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Decks' }).click()
    await page
      .getByRole('region', { name: 'Deck builder' })
      .getByRole('button', { name: 'Add', exact: true })
      .first()
      .click()
    await page.getByRole('button', { name: 'Play' }).click()
    await page.getByRole('button', { name: 'Submit deck' }).click()

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
    await page.getByRole('button', { name: 'Decks' }).click()
    await expect(page.getByRole('region', { name: 'Deck builder' })).toContainText('Llanowar Elves')
  })

  test('opens the full face of a card from the pool and from the deck', async ({ page }) => {
    await serveFrames(page, [SEATED(), CATALOG()])
    await open(page)

    await page.getByRole('button', { name: 'Decks' }).click()
    const builder = page.getByRole('region', { name: 'Deck builder' })

    // The same inspector the game table opens, over the same face model.
    await builder.getByRole('button', { name: /^Serra Angel/ }).click()
    const inspector = page.getByRole('dialog', { name: /Serra Angel/ })
    await expect(inspector).toContainText('Creature — Angel')
    await expect(inspector).toContainText('flying · vigilance')
    await inspector.getByRole('button', { name: 'Close' }).click()

    await builder.getByRole('button', { name: /^Add/ }).first().click()
    await builder.getByRole('button', { name: 'Llanowar Elves', exact: true }).click()
    // The same face the table draws, symbols and all.
    const opened = page.getByRole('dialog', { name: /Llanowar Elves/ })
    await expect(opened).toContainText(': Add')
    await expect(opened.getByRole('img', { name: /tap/ })).toBeVisible()
  })
})

// ---------------------------------------------------------------------------
// The composition, at every supported size
// ---------------------------------------------------------------------------

/**
 * The screens this shell claims to work on.
 *
 * 1067×600 is in the list because it is the one the maintainer reported: a browser at 120% zoom
 * does not scale the page, it shrinks the layout viewport, so 1280×720 at 120% *is* 1067×600.
 * That is where the old lobby's `<select>` clipped its own arrow, and it is the size this
 * composition has to answer for before any of the others.
 */
const VIEWPORTS = [
  { width: 1920, height: 1080 },
  { width: 1440, height: 900 },
  { width: 1280, height: 720 },
  { width: 1067, height: 600 },
  { width: 640, height: 360 },
  { width: 390, height: 844 },
] as const

interface Measured {
  element: string
  text: string
  fontSize: number
  scrollWidth: number
  clientWidth: number
  scrollHeight: number
  clientHeight: number
}

/**
 * Every piece of text on the screen, measured against the box it was given.
 *
 * *Every* element that directly owns text, rather than a list of the classes text is known to be
 * drawn in: the failure this exists to catch is a control clipping its own contents, and a list
 * of classes only catches it where somebody remembered to list it. A wrapper's overflow is its
 * children's problem, so only elements owning a text node of their own are reported, which is
 * also what keeps the shell's one scrolling region — the content region, which owns no text —
 * out of a measurement about clipping. Elements a pixel or less on either edge are skipped,
 * which is how the text a screen reader is given but nobody sees stays out of it.
 */
const textOnScreen = (page: Page): Promise<Measured[]> =>
  page.getByRole('main').evaluate((root) => {
    const found: Measured[] = []
    for (const node of [root, ...root.querySelectorAll('*')]) {
      const el = node as HTMLElement
      const owns = [...el.childNodes].some(
        (child) => child.nodeType === 3 && (child.textContent ?? '').trim() !== '',
      )
      if (!owns) continue
      const style = getComputedStyle(el)
      if (style.visibility === 'hidden' || style.display === 'none') continue
      const rect = el.getBoundingClientRect()
      if (rect.width <= 1 || rect.height <= 1) continue
      const classes =
        typeof el.className === 'string' && el.className.trim() !== ''
          ? `.${el.className.trim().split(/\s+/).join('.')}`
          : ''
      found.push({
        element: `${el.tagName.toLowerCase()}${classes}`,
        text: (el.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 40),
        fontSize: Math.round(parseFloat(style.fontSize) * 100) / 100,
        scrollWidth: el.scrollWidth,
        clientWidth: el.clientWidth,
        scrollHeight: el.scrollHeight,
        clientHeight: el.clientHeight,
      })
    }
    return found
  })

/** The three pre-game screens, each opened at one viewport. */
const SCREENS = [
  {
    name: 'the tables list',
    frames: () => [OPEN(), CATALOG()],
    reach: async (page: Page) => {
      await expect(page.getByRole('region', { name: 'Tables' })).toBeVisible()
    },
  },
  {
    name: 'the table you are at',
    frames: () => [SEATED(), CATALOG()],
    reach: async (page: Page) => {
      await expect(page.getByRole('region', { name: 'Table', exact: true })).toBeVisible()
    },
  },
  {
    name: 'settings',
    frames: () => [OPEN(), CATALOG()],
    reach: async (page: Page) => {
      await page.getByRole('button', { name: 'Settings' }).click()
      await expect(page.getByRole('heading', { name: 'Card art' })).toBeVisible()
    },
  },
] as const

for (const viewport of VIEWPORTS) {
  const at = `${viewport.width}×${viewport.height}`

  test.describe(`the shell at ${at}`, () => {
    test.use({ viewport: { width: viewport.width, height: viewport.height } })

    test('draws the connect screen whole, above the type floor', async ({ page }) => {
      await serveFrames(page, [OPEN()])
      await page.goto('/')
      await expect(page.getByRole('button', { name: 'Connect' })).toBeVisible()

      const text = await textOnScreen(page)
      expect(text.length).toBeGreaterThan(4)
      expect(clipped(text)).toEqual([])
      expect(belowFloor(text)).toEqual([])
      expect(await pageFits(page)).toEqual({ x: true, y: true })
    })

    for (const screen of SCREENS) {
      test(`draws ${screen.name} whole, above the type floor`, async ({ page }) => {
        await serveFrames(page, screen.frames())
        await open(page, 'Ari')
        await screen.reach(page)

        const text = await textOnScreen(page)
        // A sweep that found nothing would pass every assertion under it.
        expect(text.length).toBeGreaterThan(8)
        expect(clipped(text)).toEqual([])
        // §7: 11px on chrome, and the shell is all chrome.
        expect(belowFloor(text)).toEqual([])
        // The page itself holds still. What scrolls is the content region, and only it.
        expect(await pageFits(page)).toEqual({ x: true, y: true })
      })
    }
  })
}

/** Text that needed more room than it was given, in either axis, as a readable report. */
const clipped = (text: readonly Measured[]): string[] =>
  text
    .filter(
      (item) =>
        item.scrollWidth > item.clientWidth + 1 || item.scrollHeight > item.clientHeight + 1,
    )
    .map(
      (item) =>
        `${item.element} “${item.text}” needs ${item.scrollWidth}×${item.scrollHeight} in ` +
        `${item.clientWidth}×${item.clientHeight}`,
    )

/** Text drawn below §7's 11px chrome floor. */
const belowFloor = (text: readonly Measured[]): string[] =>
  text
    .filter((item) => item.fontSize < 10.99)
    .map((item) => `${item.element} “${item.text}” at ${item.fontSize}px`)
