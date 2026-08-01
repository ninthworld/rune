/**
 * Turn flow, from committed fixtures: where the game is, who it waits for, where it will stop,
 * and how a match ends.
 *
 * The other half of this tier is `views.spec.ts` (the board and its prompts) and
 * `relationships.views.spec.ts` (what points at what). This file is the *lifecycle*: the strip
 * and its stops, the settle's path, a rejected click, a lost socket, a concession, and the
 * result. Non-blocking, like the rest of the tier — the one blocking path is `smoke.spec.ts`.
 */
import { expect, test } from '@playwright/test'

import {
  DESKTOP,
  fixture,
  messages,
  openHistory,
  serveFrames,
  serveSockets,
  submissions,
} from './frames'

const turn = () => fixture('gameview-turn.json')

test.describe('the turn strip', () => {
  test('draws every step, marks the one the game is in, and shows where stops are set', async ({
    page,
  }) => {
    await serveFrames(page, [turn()])
    await page.goto('/')

    const strip = page.getByRole('list', { name: 'Turn steps' })
    await expect(strip.getByRole('button')).toHaveCount(12)

    // The fixture is in the upkeep of turn 4. Exactly one step is current, and it is that one.
    await expect(strip.getByRole('button', { name: /^Upkeep/ })).toHaveAttribute(
      'aria-current',
      'step',
    )
    await expect(strip.locator('[aria-current="step"]')).toHaveCount(1)

    // The two stop lists, read straight off the view: `end` on every turn, the main phases on
    // this seat's own. Everything else is unset. The state is in the name, because a mark on a
    // twelve-step row cannot be read on its own.
    await expect(strip.getByRole('button', { name: 'End — stops on every turn' })).toBeVisible()
    await expect(
      strip.getByRole('button', { name: 'Precombat main — stops on your turn' }),
    ).toBeVisible()
    await expect(strip.getByRole('button', { name: 'Draw — never stops' })).toBeVisible()
  })

  test('marks only the passed steps that belong to the turn on screen', async ({ page }) => {
    // The fixture's settle crossed a boundary: it passed turn 3's end and cleanup, then turn
    // 4's untap. Marking turn 3's positions in turn 4's strip would claim the game skipped
    // steps it has not reached yet.
    await serveFrames(page, [turn()])
    await page.goto('/')

    const strip = page.getByRole('list', { name: 'Turn steps' })
    await expect(
      strip.getByRole('button', { name: /Untap.*passed for you this turn/ }),
    ).toBeVisible()
    await expect(strip.getByRole('button', { name: /^End —/ })).not.toHaveAccessibleName(/passed/)
    await expect(strip.getByRole('button', { name: /^Cleanup —/ })).not.toHaveAccessibleName(
      /passed/,
    )
  })

  test('sends the whole preference when one step is changed', async ({ page }) => {
    const { sent } = await serveFrames(page, [turn()])
    await page.goto('/')

    // `set_stops` replaces both lists at once and is never a delta, so a click on an unset step
    // has to carry the two stops the view was already reflecting.
    await page.getByRole('button', { name: 'Draw — never stops' }).click()
    await expect.poll(() => messages(sent, 'set_stops').length).toBe(1)
    expect(messages(sent, 'set_stops')[0]).toEqual({
      type: 'set_stops',
      stops: ['end'],
      own_turn: ['draw', 'precombat_main', 'postcombat_main'],
    })
  })

  test('clears a stop with the minimal message, so defaults can be turned off', async ({
    page,
  }) => {
    const { sent, push } = await serveFrames(page, [
      { ...turn(), stops: [], own_turn_stops: ['precombat_main'] },
    ])
    await page.goto('/')

    // Off, your turn, every turn, off. A seeded default is cleared by cycling it round — and
    // the control's position is the *server's* echo, never a count kept here, so each click
    // waits for the view that answers the last one. Two empty lists mean "stop nowhere" rather
    // than "leave my defaults alone", which is the only way a seeded default can be turned off.
    const step = page.getByRole('button', { name: /^Precombat main —/ })
    await step.click()
    await expect.poll(() => messages(sent, 'set_stops').length).toBe(1)
    expect(messages(sent, 'set_stops')[0]).toEqual({
      type: 'set_stops',
      stops: ['precombat_main'],
    })

    push({ ...turn(), stops: ['precombat_main'], own_turn_stops: [] })
    await expect(step).toHaveAccessibleName(/stops on every turn/)
    await step.click()
    await expect.poll(() => messages(sent, 'set_stops').length).toBe(2)
    expect(messages(sent, 'set_stops')[1]).toEqual({ type: 'set_stops' })
  })

  test('renders nothing the client decided: the strip follows the reflected view', async ({
    page,
  }) => {
    // The server is authoritative and its echo is the sole source of the toggle state, so a
    // click that the server answers with a *different* preference must show the server's.
    const { sent, push } = await serveFrames(page, [turn()])
    await page.goto('/')

    await page.getByRole('button', { name: 'Draw — never stops' }).click()
    await expect.poll(() => messages(sent, 'set_stops').length).toBe(1)
    push({ ...turn(), stops: [], own_turn_stops: [] })

    const strip = page.getByRole('list', { name: 'Turn steps' })
    await expect(strip.getByRole('button', { name: 'Draw — never stops' })).toBeVisible()
    await expect(strip.getByRole('button', { name: 'End — never stops' })).toBeVisible()
  })
})

test.describe('who the game is waiting for', () => {
  test('says it is your move, and counts the decision clock down', async ({ page }) => {
    await serveFrames(page, [turn()])
    await page.goto('/')

    const match = page.getByRole('banner').or(page.locator('.match'))
    await expect(match).toContainText('Your move.')
    await expect(match).toContainText('Turn: Alice (p0)')
    await expect(match).toContainText('Priority: Alice (p0)')

    // The clock is projected as seconds remaining at the moment the view was built; it ticks
    // locally so a stretch with no frames does not freeze it at a number that is no longer true.
    await expect(match).toContainText(/2[0-9]s to decide/)
    await expect
      .poll(async () => (await match.textContent()) ?? '', { timeout: 8000 })
      .toMatch(/1[0-9]s to decide|2[0-2]s to decide/)
  })

  test('names the seat it is waiting for rather than guessing what they are doing', async ({
    page,
  }) => {
    await serveFrames(page, [
      { ...turn(), valid_actions: [], priority_player: 'p1', action_deadline: undefined },
    ])
    await page.goto('/')

    await expect(page.locator('.match')).toContainText('Waiting for Bob (p1).')
  })

  test('leaves the game as the subject when no seat holds priority', async ({ page }) => {
    const { priority_player, action_deadline, ...rest } = turn()
    void priority_player
    void action_deadline
    await serveFrames(page, [{ ...rest, valid_actions: [] }])
    await page.goto('/')

    await expect(page.locator('.match')).toContainText('moving on its own')
  })
})

test.describe('what the settle did on your behalf', () => {
  test('groups the path per turn and says why it happened', async ({ page }) => {
    await serveFrames(page, [turn()])
    await page.goto('/')
    await openHistory(page)

    const passed = page.getByRole('region', { name: 'While you were passed' })
    await expect(passed).toContainText('had nothing to ask you')
    // One entry per turn the path crossed, each keeping its own steps in order.
    const path = passed.locator('.side__path').getByRole('listitem')
    await expect(path).toHaveCount(2)
    await expect(path.first()).toHaveText('Turn 3 — End → Cleanup')
    await expect(path.last()).toHaveText('Turn 4 — Untap')
  })

  test('says what happened while you were passed, not only which steps went by', async ({
    page,
  }) => {
    // The reported bug (#644): a spell was cast on the opponent's turn, resolved, and killed
    // a creature — all inside one settle — and the only trace was a dead creature and a step
    // list nobody recognises. The events were always in the log; what was missing was any
    // statement of *which* of them this seat never saw. The server marks it now.
    await serveFrames(page, [turn()])
    await page.goto('/')
    await openHistory(page)

    const passed = page.getByRole('region', { name: 'While you were passed' })
    const missed = passed.locator('.side__missed').getByRole('listitem')

    // Exactly the entries from the server's mark onward — the cast, the damage, the death,
    // and the step change that followed. The one before the mark stays out of it.
    await expect(missed).toHaveCount(4)
    // The whole point, in the fixture's own words: the spell, what it did, and the creature
    // that died of it — read off the panel instead of reconstructed from the log afterwards.
    await expect(missed.nth(0)).toContainText('casts Shock')
    await expect(missed.nth(1)).toContainText('takes 2 damage')
    await expect(missed.nth(2)).toContainText('Verdant Scout dies')
  })

  test('reads the log as turns rather than as a wall of sentences', async ({ page }) => {
    await serveFrames(page, [turn()])
    await page.goto('/')
    await openHistory(page)

    const log = page.getByRole('region', { name: 'Log' })
    // A step change divides the column; the entries between two of them belong to that turn.
    await expect(log.locator('.log__entry--step')).toHaveCount(2)
    await expect(log.locator('.log__entry--step').last()).toContainText('Turn 4, Upkeep')
    await expect(log.locator('.log__entry--life')).toContainText('Alice (p0) takes 2 damage')
  })

  test('shows a seat that has dropped without hiding anything about it', async ({ page }) => {
    await serveFrames(page, [turn()])
    await page.goto('/')

    const opponent = page.getByRole('region', { name: /Bob .* seat/ })
    await expect(opponent).toContainText('disconnected')
    await expect(opponent).toContainText('20 life')
  })
})

test.describe('ending a match', () => {
  test('asks twice before conceding, and sends nothing until the second click', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [turn()])
    await page.goto('/')

    await page.getByRole('list', { name: 'Global actions' }).getByText('Concede').click()
    const confirm = page.getByRole('group', { name: 'Confirm' })
    await expect(confirm).toContainText('nothing undoes it')
    expect(submissions(sent)).toHaveLength(0)

    // Any other move is a no.
    await confirm.getByRole('button', { name: 'Keep playing' }).click()
    await expect(confirm).toBeHidden()
    expect(submissions(sent)).toHaveLength(0)

    await page.getByRole('list', { name: 'Global actions' }).getByText('Concede').click()
    await page.getByRole('button', { name: 'Yes, concede' }).click()
    await expect.poll(() => submissions(sent).length).toBe(1)
    expect(submissions(sent)[0]).toMatchObject({ action_id: 'a2' })
  })

  test('presents the result over the board, and can be pushed aside to read it', async ({
    page,
  }) => {
    await page.setViewportSize(DESKTOP)
    const { push } = await serveFrames(page, [turn()])
    await page.goto('/')
    // The socket has to exist before anything can be pushed down it.
    await expect(page.getByRole('heading', { name: /Turn 4 — Upkeep/ })).toBeVisible()

    push({
      ...turn(),
      valid_actions: [],
      result: { winner: 'p0', losers: ['p1'], reason: 'concede' },
    })

    const result = page.getByRole('region', { name: 'Game over' })
    await expect(result).toContainText('You win.')
    await expect(result).toContainText('By a concession.')

    await result.getByRole('button', { name: 'Look at the board' }).click()
    await expect(result).toBeHidden()
    // The header keeps saying so, so a dismissed panel is not a game that looks live.
    await expect(page.locator('.match')).toContainText('Game over')
    await expect(page.getByRole('region', { name: 'Actions' })).toContainText('the game is over')
  })

  test('leaves the game by starting a new session, not by pretending to send one', async ({
    page,
  }) => {
    // Round one is the finished game; round two is what a fresh, token-less session gets.
    const { sent, sockets } = await serveSockets(page, [
      [{ ...turn(), valid_actions: [], result: { winner: 'p1', reason: 'life_zero' } }],
      [fixture('lobbyview.json')],
    ])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Game over' })
      .getByRole('button', { name: 'Back to the lobby' })
      .click()

    // The board goes immediately: the session it belonged to has been given up.
    await expect(page.getByRole('heading', { name: 'SAGE' })).toBeVisible()
    await expect.poll(() => sockets.length, { timeout: 15_000 }).toBe(2)
    await expect(page.getByRole('heading', { name: 'Kitchen table' })).toBeVisible()

    // The second socket says hello with **no token**: the seat is given up deliberately, which
    // is the difference between leaving and reconnecting.
    expect(messages(sent, 'hello').at(-1)).toEqual({ type: 'hello' })
  })
})

test.describe('losing the connection', () => {
  test('keeps the board, says so, and reclaims the seat with the stored token', async ({
    page,
  }) => {
    const resumed = { ...turn(), turn: 5, phase: 'draw' }
    const { sent, sockets, drop } = await serveSockets(page, [
      [fixture('lobbyview.json'), turn()],
      [resumed],
    ])
    await page.goto('/')
    await expect(page.getByRole('heading', { name: /Turn 4 — Upkeep/ })).toBeVisible()

    drop()

    // The last view the server sent stays on screen — it is the only accurate picture the
    // player has — and the header says why nothing is moving.
    await expect(page.locator('.match')).toContainText('Connection lost')
    await expect(page.getByRole('heading', { name: /Turn 4 — Upkeep/ })).toBeVisible()

    // A second socket opens on its own and proves ownership of the held seat with the token
    // the lobby issued, and the server's view replaces what was on screen.
    await expect.poll(() => sockets.length, { timeout: 15_000 }).toBe(2)
    await expect(page.getByRole('heading', { name: /Turn 5 — Draw/ })).toBeVisible()
    await expect(page.locator('.match')).not.toContainText('Connection lost')
    expect(messages(sent, 'hello').at(-1)).toEqual({ type: 'hello', token: 's_7f3a9c21' })
  })

  test('stops waiting on a click the reconnect can never answer', async ({ page }) => {
    // The server drops a seat's `action_ack` when it reconnects, so an ack answering a click
    // that was in flight is never coming. A dock still saying "waiting" would be blocked on a
    // reply that no longer exists.
    const { sockets, drop } = await serveSockets(page, [[turn()], [turn()]])
    await page.goto('/')

    const dock = page.getByRole('region', { name: 'Actions' })
    await page.getByRole('list', { name: 'Global actions' }).getByText('Pass').click()
    await expect(dock).toContainText('waiting for the server')

    drop()
    await expect.poll(() => sockets.length, { timeout: 15_000 }).toBe(2)
    await expect(dock).not.toContainText('waiting for the server')
  })
})
