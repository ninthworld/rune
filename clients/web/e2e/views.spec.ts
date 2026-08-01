/**
 * Rendering, driven by committed fixtures over an intercepted WebSocket.
 *
 * No server, no engine, no game — just "given exactly this view, the browser renders this".
 * The fixtures are the same files the Rust tests pin and the unit suite parses, so this tier
 * cannot drift from the wire shape; what it adds over a unit test is a real build, in a real
 * browser, painting real DOM.
 *
 * This is the **non-blocking** tier (ADR 0011). Breadth lives here so breadth never gates a
 * merge on browser flake; the one blocking path is `smoke.spec.ts`.
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { expect, test, type Page } from '@playwright/test'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const fixture = (name: string): Record<string, unknown> =>
  JSON.parse(readFileSync(join(FIXTURES, name), 'utf8'))

/**
 * Serve `frames` to the client instead of a server, and record what it sends back.
 *
 * The client opens its socket and says `hello`; everything after that is whatever this hands
 * it. Interception happens in the page, so no port is bound and the two tiers never collide.
 */
async function serveFrames(page: Page, frames: readonly unknown[]) {
  // The route handler runs in Node, so what the client sends is captured directly.
  const sent: string[] = []
  await page.routeWebSocket(/.*/, (ws) => {
    ws.onMessage((message) => sent.push(String(message)))
    for (const frame of frames) ws.send(JSON.stringify(frame))
  })
  return { sent }
}

test.describe('the board, from one view', () => {
  test('renders the turn, the hand, and the stack the server sent', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await expect(page.getByRole('heading', { name: /Turn 3 — Precombat main/ })).toBeVisible()

    // Cards are named from the view; nothing is looked up client-side. Addressed as tiles
    // rather than as loose text, because a card's own name also appears inside its rules text.
    const hand = page.getByRole('region', { name: 'Your hand' })
    await expect(hand.getByRole('button', { name: /^Llanowar Elves/ })).toBeVisible()
    await expect(hand.getByRole('button', { name: /^Lightning Bolt/ })).toBeVisible()

    // Bottom-first on the wire, and rendered in that order.
    const stack = page.getByRole('region', { name: 'Stack' })
    await expect(stack.getByRole('listitem').first()).toContainText('Lightning Bolt')

    await expect(page.getByRole('region', { name: 'Your battlefield' })).toContainText(
      'Grizzly Bears',
    )

    // A token (CR 111) is a permanent with no card behind it: it renders from the view's
    // characteristics like anything else, and is marked as a token so a player can tell.
    const battlefield = page.getByRole('region', { name: 'Your battlefield' })
    await expect(battlefield.getByRole('listitem').filter({ hasText: 'Thopter' })).toContainText(
      'Token',
    )
  })

  test('draws counters, marked damage, and tap state as separate facts', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bear = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('listitem')
      .filter({ hasText: 'Grizzly Bears' })

    // Three different things about one permanent, and each has to be readable on its own:
    // power/toughness arrives already computed, damage is marked separately, and the counter
    // is neither of them.
    await expect(bear).toContainText('2/2')
    await expect(bear).toContainText('1 damage')
    await expect(bear).toContainText('2× +1/+1')
    await expect(bear).toContainText('Tapped')
  })

  test('shows a planeswalker the loyalty it has, not the loyalty it was printed with', async ({
    page,
  }) => {
    // The fixture's Nissa is printed 5 and currently 5, which cannot tell the two apart — so
    // spend her down. The board must follow the counter; the printed number is a different
    // question and must not appear in its place.
    const base = fixture('gameview.json')
    const battlefield = (base.battlefield as Record<string, unknown>[]).map((permanent) =>
      permanent.id === 'perm_nissa'
        ? { ...permanent, counters: [{ kind: 'loyalty', count: 2 }] }
        : permanent,
    )
    await serveFrames(page, [{ ...base, battlefield }])
    await page.goto('/')

    const nissa = page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('listitem')
      .filter({ hasText: 'Nissa' })

    await expect(nissa).toContainText('2')
    await expect(nissa).not.toContainText('5')
  })

  test('marks the objects the server named, and only those', async ({ page }) => {
    // `Play Forest` names `c2` and `Cast Lightning Bolt` names `c3`; nothing names the Elves.
    // The client is reading what the server pointed at, not working out what is playable —
    // which is why an object it did not name must stay unmarked.
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const hand = page.getByRole('region', { name: 'Your hand' })
    await expect(hand.getByRole('button', { name: /^Forest/ })).toHaveClass(/card--candidate/)
    await expect(hand.getByRole('button', { name: /^Lightning Bolt/ })).toHaveClass(
      /card--candidate/,
    )
    await expect(hand.getByRole('button', { name: /^Llanowar Elves/ })).not.toHaveClass(
      /card--candidate/,
    )

    // The board too: `Tap for mana` names the bear, and nothing names the token.
    const battlefield = page.getByRole('region', { name: 'Your battlefield' })
    await expect(battlefield.getByRole('button', { name: /^Grizzly Bears/ })).toHaveClass(
      /card--candidate/,
    )
    await expect(battlefield.getByRole('button', { name: /^Thopter/ })).not.toHaveClass(
      /card--candidate/,
    )
  })

  test('opens a card inspector from the hand without submitting anything', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page.getByRole('region', { name: 'Your hand' }).getByRole('button').first().click()

    // Everything the hand tile clamps or drops is here, in full.
    const inspector = page.getByRole('dialog')
    await expect(inspector).toContainText('Llanowar Elves')
    await expect(inspector).toContainText('{T}: Add {G}.')

    // Inspecting is not a game action. Asserted against what was *sent* rather than a message
    // count, because the client's own `hello` races the click and would count as traffic.
    expect(sent.map((s) => JSON.parse(s)).filter((m) => m.type === 'choose_action')).toEqual([])

    await page.keyboard.press('Escape')
    await expect(inspector).toHaveCount(0)
  })

  test('inspects a permanent the player cannot act on', async ({ page }) => {
    // Reading an object matters most when it cannot be acted on, so inspection is offered on
    // every surface rather than only where an action happens to exist.
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button')
      .filter({ hasText: 'Thopter' })
      .click()

    await expect(page.getByRole('dialog')).toContainText('Artifact Creature — Thopter')
    await page.getByRole('button', { name: 'Close' }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
  })

  test('renders an emblem beside the board, with its abilities', async ({ page }) => {
    // An emblem (CR 114) is in no zone and is never removed, so it has its own region
    // rather than a row on the battlefield. Everything shown comes from the view: the
    // controller's id and the server-composed ability sentences.
    await serveFrames(page, [fixture('gameview-emblem.json')])
    await page.goto('/')

    const emblems = page.getByRole('region', { name: 'Emblems' })
    await expect(emblems).toContainText('Creatures you control get +2/+2.')
    await expect(emblems).toContainText('Creatures you control have indestructible.')

    // The board it modifies renders from the same view — the client computes no anthem.
    await expect(page.getByRole('region', { name: 'Your battlefield' })).toContainText('6/4')
  })

  test('offers exactly the actions the server listed', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const actions = page.getByRole('region', { name: 'Actions' })
    await expect(actions.getByRole('button', { name: 'Pass' })).toBeVisible()
    await expect(actions.getByRole('button', { name: 'Play Forest' })).toBeVisible()
    // Nothing invented: a button the server did not list must not exist.
    await expect(actions.getByRole('button', { name: 'Concede' })).toHaveCount(0)
  })

  test('sends the action id and token the server issued', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Pass' }).click()
    await expect.poll(() => sent.length).toBeGreaterThan(1)

    const submitted = sent.map((s) => JSON.parse(s)).find((m) => m.type === 'choose_action')
    expect(submitted).toMatchObject({ type: 'choose_action', action_id: 'a1' })
  })
})

test.describe('the table as a composition', () => {
  // Desktop landscape is the one layout (`docs/brief.md`), so it is the one these are sized to.
  const DESKTOP = { width: 1440, height: 900 }

  /**
   * The page itself does not scroll, in either axis.
   *
   * Asked by trying to scroll rather than by comparing `scrollWidth` — a region that clips its
   * own overflow still inflates the root element's reported scroll width in Chrome, and what
   * actually matters to a player is whether the table can be scrolled out from under them.
   */
  const pageFits = (page: Page) =>
    page.evaluate(() => {
      window.scrollTo(9999, 9999)
      return { x: window.scrollX === 0, y: window.scrollY === 0 }
    })

  test('seats both players with their own half of the board', async ({ page }) => {
    // The point of a table over a state dump: a permanent's controller is answered by where
    // the card is. `gameview-commander.json` has one permanent per opponent and none of yours,
    // so a client that pooled them into one list would put Bob's commander on your side.
    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [fixture('gameview-commander.json')])
    await page.goto('/')

    await expect(page.getByRole('region', { name: /^Bob .* battlefield/ })).toContainText(
      'Jedit Ojanen',
    )
    await expect(page.getByRole('region', { name: /^Random .* battlefield/ })).toContainText(
      'Grizzly Bears',
    )
    // Yours is empty, and says so rather than borrowing someone else's permanents.
    await expect(page.getByRole('region', { name: 'Your battlefield' })).toContainText(
      'No permanents',
    )

    // Each seat's own totals sit with that seat.
    await expect(page.getByRole('region', { name: 'Your seat' })).toContainText('eliminated')
    await expect(page.getByRole('region', { name: /^Bob .* seat/ })).toContainText('disconnected')
    await expect(page.getByRole('region', { name: /^Random .* seat/ })).toContainText('AI')
  })

  test('lays out an empty board as a whole table, not a blank page', async ({ page }) => {
    // Turn one: no permanents, no stack, no graveyard. Every surface must still be in its
    // place, because a player learns where things are from the empty table.
    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [
      {
        you: 'p1',
        phase: 'upkeep',
        turn: 1,
        me: { life: 20, library_size: 53 },
        opponents: [
          { player_id: 'p2', hand_size: 7, life: 20, library_size: 53, graveyard_size: 0 },
        ],
        seat_order: ['p1', 'p2'],
        active_player: 'p1',
        priority_player: 'p1',
        valid_actions: [{ id: 'a1', type: 'pass_priority', label: 'Pass' }],
      },
    ])
    await page.goto('/')

    for (const name of ['Your seat', 'p2 seat', 'Your battlefield', 'p2 battlefield', 'Stack']) {
      await expect(page.getByRole('region', { name })).toBeVisible()
    }
    await expect(page.getByRole('region', { name: 'Your hand' })).toContainText('empty')
    await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })

  test('keeps the dock reachable under a board and hand that will not fit', async ({ page }) => {
    // The constraint the whole geometry exists for. Sixty permanents, a twenty-card hand, and
    // a name far longer than any real one: every region scrolls inside its own area, and the
    // controls that end the turn stay exactly where they were on an empty board. A player who
    // has to scroll the page to find `Pass` has lost the game to the layout.
    const base = fixture('gameview.json')
    const bear = (base.battlefield as Record<string, unknown>[])[0]!
    const card = (base.my_hand as Record<string, unknown>[])[0]!
    const long = 'Wolfhearted Thunderskald of the Everflowing Cascade, Third of Their Name'

    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [
      {
        ...base,
        player_names: { p1: long, p2: long },
        battlefield: Array.from({ length: 60 }, (_, i) => ({
          ...bear,
          id: `stress_${i}`,
          card: { ...(bear.card as object), id: `stress_${i}`, name: `${long} ${i}` },
        })),
        my_hand: Array.from({ length: 20 }, (_, i) => ({
          ...card,
          id: `hand_${i}`,
          name: `${long} ${i}`,
        })),
      },
    ])
    await page.goto('/')

    const actions = page.getByRole('region', { name: 'Actions' })
    await expect(actions.getByRole('button', { name: 'Pass' })).toBeInViewport()
    await expect(page.getByRole('region', { name: 'Your hand' })).toBeInViewport()

    // Sixty permanents are all rendered — they scroll within the board, they are not dropped.
    await expect(
      page.getByRole('region', { name: 'Your battlefield' }).getByRole('listitem'),
    ).toHaveCount(60)

    // And none of it grew the page.
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })

  test('puts the public piles in front of the seat that owns them', async ({ page }) => {
    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // Exile is yours in this fixture and the graveyard is the opponent's; each is counted and
    // opens in place rather than being listed somewhere central where ownership is a label.
    const mine = page.getByRole('region', { name: 'Your seat' })
    await expect(mine.getByText('Exile (1)')).toBeVisible()
    await mine.getByText('Exile (1)').click()
    await expect(mine).toContainText('Path to Exile')

    await expect(page.getByRole('region', { name: 'p2 seat' })).toContainText('Graveyard (1)')
  })
})

test.describe('a finished game', () => {
  test('reads as over, with nothing left to do', async ({ page }) => {
    // This fixture carries `valid_actions: []` explicitly while omitting `battlefield`. A
    // client that treated only absence as "nothing" would show a concluded game as still
    // offering moves, so both spellings must land in the same place.
    await serveFrames(page, [fixture('gameview-over.json')])
    await page.goto('/')

    await expect(page.getByRole('heading', { name: 'Game over' })).toBeVisible()
    await expect(page.getByText(/decked/)).toBeVisible()
    await expect(page.getByRole('region', { name: 'Actions' })).toContainText('Nothing to do')
  })
})

test.describe('making a settle legible', () => {
  test('names every step the server passed on your behalf, with its turn', async ({ page }) => {
    // The product hypothesis: one message can cover a whole turn, and a player must be able to
    // tell what happened. A path, not a set — a revisited position appears twice, and each
    // entry carries its own turn because an extra combat phase revisits a step within a turn.
    const view = {
      ...fixture('gameview.json'),
      auto_passed: true,
      auto_passed_steps: [
        { turn: 3, phase: 'begin_combat' },
        { turn: 3, phase: 'declare_attackers' },
        { turn: 3, phase: 'begin_combat' },
        { turn: 4, phase: 'upkeep' },
      ],
    }
    await serveFrames(page, [view])
    await page.goto('/')

    const settle = page.getByRole('region', { name: 'Passed for you' })
    await expect(settle).toBeVisible()
    const steps = settle.getByRole('listitem')
    await expect(steps).toHaveCount(4)
    await expect(steps.nth(0)).toHaveText('Turn 3 — Begin combat')
    // The repeat is preserved rather than collapsed.
    await expect(steps.nth(2)).toHaveText('Turn 3 — Begin combat')
    await expect(steps.nth(3)).toHaveText('Turn 4 — Upkeep')
  })
})

test.describe('the pre-game screen', () => {
  test('renders a lobby and offers only the commands the server allows', async ({ page }) => {
    await serveFrames(page, [
      { session: 's1', you: 'p0', valid_commands: ['create_room', 'join_room'] },
    ])
    await page.goto('/')

    await expect(page.getByRole('button', { name: /Create a two-seat table/ })).toBeVisible()
    // `set_name` was not offered, so the control must not appear.
    await expect(page.getByRole('button', { name: 'Set name' })).toHaveCount(0)
  })

  test('shows a rejected deck without losing the lobby', async ({ page }) => {
    // The error rides alongside an otherwise unchanged lobby view, so both must survive.
    await serveFrames(page, [
      { session: 's1', you: 'p0', valid_commands: ['create_room'] },
      {
        lobby_error: {
          code: 'copy_limit',
          reason: 'Onakke Ogre appears 5 times',
          card: 'onakke_ogre',
        },
      },
    ])
    await page.goto('/')

    await expect(page.getByRole('alert')).toContainText('Onakke Ogre appears 5 times')
    await expect(page.getByRole('button', { name: /Create a two-seat table/ })).toBeVisible()
  })
})

test.describe('a message this client cannot read', () => {
  test('says so and keeps the screen', async ({ page }) => {
    await serveFrames(page, [
      { session: 's1', you: 'p0', valid_commands: ['create_room'] },
      { phase: 'interstitial', you: 'p0' },
    ])
    await page.goto('/')

    await expect(page.getByRole('status')).toContainText('could not be read')
    await expect(page.getByRole('button', { name: /Create a two-seat table/ })).toBeVisible()
  })
})
