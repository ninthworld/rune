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
 */
import { expect, test } from '@playwright/test'

test('two clicks from a fresh page to a played action', async ({ page }) => {
  const failures: string[] = []
  page.on('pageerror', (error) => failures.push(String(error)))

  await page.goto('/')

  // 1. The lobby, from the server's own first frame.
  await expect(page.getByRole('heading', { name: 'SAGE' })).toBeVisible()
  await expect(page.getByText(/^You are /)).toBeVisible()

  // 2. A table.
  await page.getByRole('button', { name: /Create a two-seat table/ }).click()
  await expect(page.getByRole('region', { name: /^r/ })).toBeVisible()

  // 3. A deck, an opponent, and a ready signal — each gated on `valid_commands`, so each
  //    button appearing at all is the server saying the step is available now.
  await page.getByRole('button', { name: /^Submit deck/ }).click()
  await expect(page.getByText(/· decked/).first()).toBeVisible()

  await page.getByRole('button', { name: /Seat an AI opponent/ }).click()
  await expect(page.getByText(/· AI \(/)).toBeVisible()

  await page.getByRole('button', { name: 'Ready' }).click()

  // 4. The hand-off: the server switches this socket to the in-game contract by sending a
  //    game view, and the screen follows the frame with no client-held phase.
  await expect(page.getByRole('heading', { name: /^Turn \d+ — / })).toBeVisible({ timeout: 20_000 })
  await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Your seat' })).toContainText('life')

  // 5. Take the action the server offers, and see the game move.
  const actions = page.getByRole('region', { name: 'Actions' })
  const heading = page.getByRole('heading', { name: /^Turn \d+ — / })

  // What the screen says now, so the change after acting is a real difference and not a
  // hopeful timeout.
  const signature = async () =>
    `${await heading.textContent()}|${(await actions.getByRole('button').allTextContents()).join(',')}`
  const before = await signature()

  const first = actions.getByRole('button').first()
  await expect(first).toBeVisible()
  await first.click()

  // A mulligan decision opens a draft rather than submitting. Answer it by clicking candidates
  // until the server's own counts are satisfied — which is exactly what `Confirm` becoming
  // enabled means, so this needs no knowledge of the prompt it happens to have been handed.
  const confirm = page.getByRole('button', { name: 'Confirm' })
  if (await confirm.isVisible().catch(() => false)) {
    // Scoped to the slot fieldsets, so `Confirm` and `Cancel` are never among the candidates.
    const candidates = page
      .getByRole('region', { name: 'Choices' })
      .getByRole('group')
      .getByRole('button')
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
  await expect(page.getByText('That action could not be taken')).toHaveCount(0)

  // And the correlation closed. The client holds its click as pending until the server echoes
  // the id back in `action_ack`, so a dock still saying "waiting" after a completed round trip
  // means the two ends disagree about correlation — which only a real server can prove.
  await expect(page.getByText('waiting for the server')).toHaveCount(0)
  expect(failures, 'the page threw while playing').toEqual([])
})
