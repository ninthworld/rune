/**
 * The live scenario path (issue #777), through the shipping UI.
 *
 * `sage-scenario` builds an exact position from a checked-in file and serves it on a loopback
 * socket; this build is pointed at that socket with `?server=` and nothing else changes. So what
 * is under test is the claim the whole tool rests on: that a hand-authored position is a *real*
 * game — the engine offering the actions, the server projecting the view, the client rendering it
 * and sending back an advertised id.
 *
 * It is deliberately **not** the blocking gate (ADR 0011 keeps that to one path). It is its own
 * tier because it needs a Rust toolchain the `views` tier does not, and because a contributor tool
 * breaking must not stop a merge — it must only be noticed.
 *
 * What no other tier can stand in for: the fixture tier replays a `GameView` over an intercepted
 * socket, so a click there proves rendering and nothing about legality. Here a click has to be an
 * action the engine actually generated for this position, or the server rejects it.
 */
import { expect, test } from '@playwright/test'

/** Where `make e2e-scenario` serves `scenarios/murder-the-dreadmaw.toml`. */
const SCENARIO_SERVER = 'ws://127.0.0.1:9010'

test('an authored position opens straight into a live game and plays', async ({ page }) => {
  const failures: string[] = []
  page.on('pageerror', (error) => failures.push(String(error)))

  await page.goto(`/?server=${SCENARIO_SERVER}`)

  // 1. The board, with no lobby, no deck, and no mulligan in front of it. The server's first
  //    frame on this socket is a `GameView`, and a game view arriving is what the client treats
  //    as "you are seated" — so there is nothing to click through to get here.
  const heading = page.getByRole('heading', { name: /^Turn 6 — / })
  await expect(heading).toBeVisible({ timeout: 30_000 })
  await expect(page.getByRole('heading', { name: 'Open tables' })).toHaveCount(0)
  await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()

  // 2. And it is the position the *file* describes, not a game that happened to start. Every one
  //    of these came from a line in `scenarios/murder-the-dreadmaw.toml`.
  await expect(page.getByRole('button', { name: /^You, 14 life/ })).toBeVisible()
  await expect(page.getByRole('button', { name: /^Sparring partner, 17 life/ })).toBeVisible()
  await expect(page.getByRole('region', { name: /battlefield/ }).first()).toBeVisible()
  await expect(page.getByText('Colossal Dreadmaw').first()).toBeVisible()

  // 3. Play the land in hand. One click, because the server offered exactly one thing to do with
  //    it — and it offered that only because the engine generated it for *this* position, at this
  //    step, with this seat holding priority. No fixture can produce that.
  const hand = page.getByRole('region', { name: 'Your hand' })
  const field = page.getByRole('region', { name: 'Your battlefield' })
  await expect(field.locator('[aria-label^="Swamp"]')).toHaveCount(3)
  await hand.locator('[aria-label^="Swamp"]').first().click()

  // The authoritative board moved: the land left the hand and is on the battlefield. Both halves
  // came back from the server in one view; neither was drawn by the click.
  await expect(field.locator('[aria-label^="Swamp"]')).toHaveCount(4, { timeout: 20_000 })
  await expect(hand.locator('[aria-label^="Swamp"]')).toHaveCount(0)
  // A rejected submission means the client built a message the server would not take — which on
  // this tier would most likely mean the *position* is the thing that is wrong.
  await expect(page.getByText('could not be taken')).toHaveCount(0)

  // 4. Reload. The scenario room holds the seat open exactly as a lobby room does, so the whole
  //    live game comes back from one view — the game that is *running*, not the file replayed
  //    from the top: the fourth Swamp is still there.
  await page.reload()
  await expect(page.getByRole('heading', { name: /^Turn 6 — / })).toBeVisible({ timeout: 30_000 })
  await expect(
    page.getByRole('region', { name: 'Your battlefield' }).locator('[aria-label^="Swamp"]'),
  ).toHaveCount(4, { timeout: 20_000 })

  expect(failures, 'the page threw while playing the scenario').toEqual([])
})
