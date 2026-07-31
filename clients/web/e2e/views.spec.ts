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

    // Cards are named from the view; nothing is looked up client-side.
    const hand = page.getByRole('region', { name: 'Your hand' })
    await expect(hand.getByText('Llanowar Elves')).toBeVisible()
    await expect(hand.getByText('Lightning Bolt')).toBeVisible()

    // Bottom-first on the wire, and rendered in that order.
    const stack = page.getByRole('region', { name: 'Stack' })
    await expect(stack.getByRole('listitem').first()).toContainText('Lightning Bolt')

    await expect(page.getByRole('region', { name: 'Battlefield' })).toContainText('Grizzly Bears')
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
