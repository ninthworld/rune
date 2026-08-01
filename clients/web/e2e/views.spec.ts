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

    await expect(page.getByRole('region', { name: 'Battlefield' })).toContainText('Grizzly Bears')

    // A token (CR 111) is a permanent with no card behind it: it renders from the view's
    // characteristics like anything else, and is marked as a token so a player can tell.
    const battlefield = page.getByRole('region', { name: 'Battlefield' })
    await expect(battlefield.getByRole('listitem').filter({ hasText: 'Thopter' })).toContainText(
      'Token',
    )
  })

  test('draws counters, marked damage, and tap state as separate facts', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bear = page
      .getByRole('region', { name: 'Battlefield' })
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
      .getByRole('region', { name: 'Battlefield' })
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
    const battlefield = page.getByRole('region', { name: 'Battlefield' })
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
      .getByRole('region', { name: 'Battlefield' })
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
    await expect(page.getByRole('region', { name: 'Battlefield' })).toContainText('6/4')
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
