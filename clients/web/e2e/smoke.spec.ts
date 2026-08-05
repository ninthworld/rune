/**
 * The blocking gate: one thin path against the real server.
 *
 * Build → browser → socket → `sage-server` → engine, and back to painted DOM. Nothing is
 * mocked. This is the only e2e test a merge waits on, and it is kept to one path deliberately
 * — a gate whose runtime is an argument for deleting it does not survive (ADR 0011).
 *
 * What it proves that nothing else can: that the real wire contract, the real view projection,
 * and this client's reconstruction of them actually agree. Unit tests check the mirror against
 * fixtures; fixtures are only ever what we believed the server sends.
 *
 * It runs the match to its end rather than stopping at the first action, because the seams at
 * the end are the same kind of seam as the one at the start and no fixture can stand in for
 * them: a **reload** must land back in the game the server is still running (which is a
 * server-side hand-off, not a client trick), and a **concession** must actually end it. Both are
 * a handful of clicks on a game that is already up, so the gate stays one path.
 */
import { expect, test } from '@playwright/test'

test('a fresh page to a played action, a reload, and a conceded game', async ({ page }) => {
  const failures: string[] = []
  page.on('pageerror', (error) => failures.push(String(error)))

  await page.goto('/')

  // 1. Connect: a name and a server, before anything else (`docs/client-design.md` §9.3). The
  //    socket is already open to the address `socket.ts` resolved, so this is the client saying
  //    who it is on a connection the server has already answered.
  await expect(page.getByRole('heading', { name: 'SAGE' })).toBeVisible()
  await page.getByLabel('Name').fill('Smoke')
  await page.getByRole('button', { name: 'Connect' }).click()

  // The lobby, and the name the server took: `set_name` is a real round trip on the lobby
  // contract, and the topbar shows what came back rather than what was typed.
  await expect(page.getByRole('heading', { name: 'Open tables' })).toBeVisible()
  await expect(page.locator('.lobby-who')).toContainText('Smoke')

  // 2. A table, created from the server's own catalog. The format list is not in this build: it
  //    arrives in the `CatalogView` answering the `request_catalog` this screen sends, so the
  //    control being populated at all is another real round trip on the lobby contract.
  await page.getByRole('button', { name: '+ Create table' }).click()
  const form = page.getByRole('dialog', { name: 'New table' })
  await expect(
    form.getByRole('radiogroup', { name: 'Format' }).getByRole('radio').first(),
  ).toBeVisible()
  await form.getByRole('button', { name: 'Create the table' }).click()
  await expect(page.getByRole('region', { name: 'Table' })).toBeVisible()

  // 3. A deck, an opponent, and a ready signal — each gated on `valid_commands`, so each control
  //    appearing at all is the server saying the step is available now. Choosing a deck submits
  //    it: the server is what says a deck is legal.
  await page.getByRole('button', { name: 'Change' }).click()
  await page
    .getByRole('dialog', { name: 'Load a deck' })
    .getByRole('button', { name: 'Load' })
    .first()
    .click()

  // The AI kinds are the catalog's too, so seating one is only possible because the catalog
  // arrived — and the deck it seats the bot with is validated exactly like a human's.
  await page.getByRole('button', { name: 'Seat an AI opponent' }).first().click()
  await page
    .getByRole('dialog', { name: 'Seat an AI opponent' })
    .getByRole('button', { name: 'Seat', exact: true })
    .click()
  await expect(page.getByRole('region', { name: 'Table' })).toContainText('AI')

  await page.getByRole('button', { name: 'Ready', exact: true }).click()

  // 4. The hand-off: the server switches this socket to the in-game contract by sending a game
  //    view, and the screen follows the frame with no client-held phase.
  await expect(page.getByRole('heading', { name: /^Turn \d+ — / })).toBeVisible({ timeout: 20_000 })
  await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Your battlefield' })).toBeVisible()

  // 5. Take the action the server offers, and see the game move.
  const actions = page.getByRole('region', { name: 'Actions' })
  const heading = page.getByRole('heading', { name: /^Turn \d+ — / })

  // What the screen says now, so the change after acting is a real difference and not a hopeful
  // timeout.
  const signature = async () =>
    `${await heading.textContent()}|${(await actions.getByRole('button').allTextContents()).join(',')}`
  const before = await signature()

  const first = actions.getByRole('button').first()
  await expect(first).toBeVisible()
  await first.click()

  // A mulligan decision opens a draft rather than submitting. Answer it by clicking the
  // server's own options until `Confirm` is enabled — which is exactly what "the counts it
  // published are satisfied" means, so this needs no knowledge of the prompt it was handed.
  const confirm = page.getByRole('button', { name: 'Confirm' })
  if (await confirm.isVisible().catch(() => false)) {
    const candidates = actions.locator('.action-slots button')
    for (let i = 0; i < (await candidates.count()); i += 1) {
      if (await confirm.isEnabled()) break
      await candidates.nth(i).click()
    }
    await confirm.click()
  }

  // The server answered: the step advanced, or a fresh action list arrived. Either way a full
  // round trip completed, which is what this gate exists to prove.
  await expect.poll(signature, { timeout: 20_000 }).not.toBe(before)

  // A rejected submission is a real failure here: the client built a message the server would
  // not take, which is exactly the class of bug this tier is for.
  await expect(page.getByText('could not be taken')).toHaveCount(0)

  // And the correlation closed. The client holds its click as pending until the server echoes
  // the id back in `action_ack`, so a bar still saying "waiting" after a completed round trip
  // means the two ends disagree about correlation — which only a real server can prove.
  await expect(actions).not.toContainText('waiting for the server')

  // 6. Reload. The seat is held open, the game keeps running, and the token in this tab's
  //    session storage is what proves the returning connection owns it. The server has to hand
  //    that connection back to its room — a lobby view would leave a player stranded outside
  //    their own match — and the whole match comes back from one view.
  await page.reload()
  await expect(page.getByRole('heading', { name: /^Turn \d+ — / })).toBeVisible({ timeout: 20_000 })
  await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()
  await expect(
    page.getByRole('list', { name: 'Turn steps' }).first().getByRole('button'),
  ).toHaveCount(12)

  // 7. Concede, which is the one action that ends a match on demand. It is asked twice, so the
  //    first click sends nothing; the second one ends the game.
  const concede = page.getByRole('button', { name: 'Concede' })
  await expect(concede).toBeVisible({ timeout: 20_000 })
  await concede.click()
  await page.getByRole('button', { name: /^Yes, concede/ }).click()

  // 8. The result, from the server's own `GameView.result`.
  const result = page.getByRole('dialog', { name: 'Game over' })
  await expect(result).toBeVisible({ timeout: 20_000 })
  await expect(result).toContainText('By a concession.')

  // 9. And the way out: leaving gives up the token that holds the seat, so the next connection
  //    is a new session and lands in the lobby.
  await result.getByRole('button', { name: 'Back to the lobby' }).click()
  // Back in the lobby, not back at the connect screen: giving up a seat gives up a token, not
  // the server this tab is talking to.
  await expect(page.getByRole('heading', { name: 'Open tables' })).toBeVisible({ timeout: 20_000 })
  await expect(page.getByRole('region', { name: 'Tables' })).toBeVisible({ timeout: 20_000 })

  expect(failures, 'the page threw while playing').toEqual([])
})
