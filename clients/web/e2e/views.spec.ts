/**
 * Rendering, driven by committed fixtures over an intercepted WebSocket.
 *
 * The board as one view: what the server sent, what a click on it means, the shapes a prompt
 * comes in, and the composition the whole table is laid out in. Relationships between objects —
 * combat, targets, attachments — and the zone browsers are the other half of this tier and live
 * in `relationships.views.spec.ts`.
 *
 * This is the **non-blocking** tier (ADR 0011). Breadth lives here so breadth never gates a
 * merge on browser flake; the one blocking path is `smoke.spec.ts`.
 */
import { expect, test } from '@playwright/test'

import { DESKTOP, fixture, pageFits, serveFrames, submissions } from './frames'

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

    // Bottom-first on the wire, top-first on screen: what a player needs from this column is
    // what resolves next, and the object that does says so in words rather than leaving it to
    // be inferred from which end of the column it sits on.
    const stack = page.getByRole('region', { name: 'Stack' })
    const objects = stack.getByRole('listitem')
    await expect(objects.first()).toContainText('Resolves next')
    await expect(objects.first()).toContainText('Tap target creature')
    await expect(objects.last()).toContainText('Lightning Bolt')
    await expect(objects.last()).toContainText('6 of 6')

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

    // The Elves own no action in this view, so there is nothing to act on and one click reads
    // the card. An object the server did not name never costs a player a second click.
    await page.getByRole('region', { name: 'Your hand' }).getByRole('button').first().click()

    // Everything the hand tile clamps or drops is here, in full.
    const inspector = page.getByRole('dialog')
    await expect(inspector).toContainText('Llanowar Elves')
    await expect(inspector).toContainText('{T}: Add {G}.')

    // Inspecting is not a game action. Asserted against what was *sent* rather than a message
    // count, because the client's own `hello` races the click and would count as traffic.
    expect(submissions(sent)).toEqual([])

    await page.keyboard.press('Escape')
    await expect(inspector).toHaveCount(0)
  })

  test('reads a card that does have an action, on the second click', async ({ page }) => {
    // Selecting is the first click and inspecting the second, so an object with something to do
    // never becomes an object that cannot be read. Nothing about either is sent.
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bolt = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })
    await bolt.click()
    await expect(page.getByRole('region', { name: 'Actions' })).toContainText('Cast Lightning Bolt')

    await bolt.click()
    await expect(page.getByRole('dialog')).toContainText('Lightning Bolt')
    expect(submissions(sent)).toEqual([])
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
    // Passing owns no object, so it lives in the dock where it can always be found.
    await expect(actions.getByRole('button', { name: 'Pass' })).toBeVisible()
    // Nothing invented: a button the server did not list must not exist, in either list.
    await expect(actions.getByRole('button', { name: 'Concede' })).toHaveCount(0)

    // Every action stays reachable without a card, because a subject is not guaranteed to be
    // drawn anywhere — it may sit inside a collapsed pile or in no rendered zone at all.
    await actions.getByText('Every action (4)').click()
    const every = actions.getByRole('list', { name: 'Every action' })
    await expect(every.getByRole('button', { name: 'Play Forest' })).toBeVisible()
    await expect(every.getByRole('button', { name: 'Tap for mana' })).toBeVisible()
  })

  test('sends the action id and token the server issued', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Pass' }).click()
    await expect.poll(() => submissions(sent)).toHaveLength(1)

    expect(submissions(sent)[0]).toMatchObject({ type: 'choose_action', action_id: 'a1' })
  })
})

test.describe('acting by clicking the table', () => {
  test('offers only the actions the server attached to the card that was clicked', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    const actions = page.getByRole('region', { name: 'Actions' })
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Forest/ })
      .click()

    const owned = actions.getByRole('list', { name: 'Actions for the selected object' })
    await expect(owned.getByRole('button', { name: 'Play Forest' })).toBeVisible()
    // The other card's action is the other card's business; a subject list is not a menu of
    // everything that is legal right now.
    await expect(owned.getByRole('button', { name: /Emberfall/ })).toHaveCount(0)

    await owned.getByRole('button', { name: 'Play Forest' }).click()
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'a1',
      token: 't0000000000000a1',
    })
  })

  test('takes a mana ability in one click, because the server said it is one', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Llanowar Elves/ })
      .click()
    await page.getByRole('button', { name: '{T}: Add {G}. ⟨mana⟩' }).click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({ action_id: 'a3' })
  })

  test('highlights the candidates for the open slot, and nothing else', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    const hand = page.getByRole('region', { name: 'Your hand' })
    await hand.getByRole('button', { name: /^Emberfall Surge/ }).click()
    await page.getByRole('button', { name: /^Cast Emberfall Surge/ }).click()

    // `t0` lists the elves and the opponent. Both light up — a seat is a target like anything
    // else — and the Forest, which owns an action of its own, does not.
    await expect(
      page
        .getByRole('region', { name: 'Your battlefield' })
        .getByRole('button', { name: /^Llanowar Elves/ }),
    ).toHaveClass(/card--candidate/)
    await expect(
      page.getByRole('region', { name: 'p1 seat' }).getByRole('button', { name: 'p1' }),
    ).toHaveClass(/card--candidate/)
    await expect(hand.getByRole('button', { name: /^Forest/ })).not.toHaveClass(/card--candidate/)
  })

  test('builds a targeted spell from the table and the dock together', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Emberfall Surge/ })
      .click()
    await page.getByRole('button', { name: /^Cast Emberfall Surge/ }).click()

    const confirm = page.getByRole('button', { name: 'Confirm' })
    // X has a range and no answer yet, so the draft is incomplete and cannot be sent.
    await expect(confirm).toBeDisabled()

    // The target comes from the board; the number comes from the control the range describes.
    await page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Llanowar Elves/ })
      .click()
    await page.getByRole('spinbutton').fill('2')

    await expect(confirm).toBeEnabled()
    await confirm.click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'a2',
      token: 't0000000000000a2',
      targets: [
        { slot: 't0', chosen: ['perm_elves'] },
        { slot: 'x', chosen: ['2'] },
      ],
    })
  })

  test('cancels a draft without telling the server anything', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Emberfall Surge/ })
      .click()
    await page.getByRole('button', { name: /^Cast Emberfall Surge/ }).click()
    await page.getByRole('button', { name: 'Cancel' }).click()

    // Back, not away: the card is still selected and its actions are offered again, so a
    // mis-armed action costs one click rather than restarting from the table.
    await expect(page.getByRole('button', { name: 'Confirm' })).toHaveCount(0)
    await expect(
      page
        .getByRole('list', { name: 'Actions for the selected object' })
        .getByRole('button', { name: /^Cast Emberfall Surge/ }),
    ).toBeVisible()
    expect(submissions(sent)).toEqual([])
  })
})

test.describe('the shapes a prompt comes in', () => {
  test('asks the follow-up question only once the option that needs it is chosen', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-prompts.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Keep or mulligan' }).click()
    const choices = page.getByRole('region', { name: 'Choices' })

    // Taking another hand bottoms nothing, so the bottoming slot is not a question yet.
    await expect(choices).not.toContainText('bottom of your library')
    await expect(page.getByRole('button', { name: 'Confirm' })).toBeDisabled()

    await choices.getByRole('button', { name: 'Keep this hand' }).click()
    await expect(choices).toContainText('Put 1 card(s) on the bottom of your library')
    await expect(choices).toContainText('0 of 1')

    // The candidates are hand cards, so the hand answers it.
    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Grizzly Bears/ })
      .click()
    await expect(choices).toContainText('1 of 1')

    await page.getByRole('button', { name: 'Confirm' }).click()
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'a0',
      targets: [
        { slot: 'decision', chosen: ['keep'] },
        { slot: 'bottom', chosen: ['card_11'] },
      ],
    })
  })

  test('says when picking fewer — or none — is a finished answer', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-choice.json')])
    await page.goto('/')

    await page.getByRole('button', { name: /^Choose up to 2 cards/ }).click()
    const choices = page.getByRole('region', { name: 'Choices' })
    await expect(choices).toContainText('0 of 0–2')

    // "Up to" means an empty answer is complete. A player who cannot tell sits waiting for a
    // button that is already lit.
    await expect(page.getByRole('button', { name: 'Confirm' })).toBeEnabled()

    // The cards are the ones the server showed this seat, so the panel beside the dock is where
    // they are clicked.
    await page
      .getByRole('region', { name: 'Shown to you' })
      .getByRole('button', { name: /^Swamp/ })
      .click()
    await expect(choices).toContainText('1 of 0–2')

    await page.getByRole('button', { name: 'Confirm' }).click()
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      targets: [{ slot: 'choice', chosen: ['card_21'] }],
    })
  })

  test('offers an optional effect as the two answers the server gave it', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-optional.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Pay {1} to draw a card?' }).click()
    const choices = page.getByRole('region', { name: 'Choices' })
    await expect(choices.getByRole('button', { name: 'Pay {1}' })).toBeVisible()
    await choices.getByRole('button', { name: 'Decline' }).click()
    await page.getByRole('button', { name: 'Confirm' }).click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'a0',
      targets: [{ slot: 'choice', chosen: ['decline'] }],
    })
  })
})

test.describe('declaring combat', () => {
  /**
   * The shape the server sends once an opponent has a planeswalker (issue #608): one
   * multi-select naming who may attack, then one slot per attacker naming what it may attack.
   * There is no committed fixture for it, so the view is written out here from `docs/protocol.md`
   * — the same way the empty-board and stress cases below are.
   */
  const creature = (id: string, name: string, controller: string) => ({
    id,
    controller,
    owner: controller,
    card: { id, name, type_line: 'Creature — Bear', power: '2', toughness: '2' },
  })

  const COMBAT = {
    you: 'p0',
    phase: 'declare_attackers',
    turn: 4,
    me: { life: 20, library_size: 40 },
    opponents: [{ player_id: 'p1', life: 20, hand_size: 5, library_size: 40, graveyard_size: 0 }],
    seat_order: ['p0', 'p1'],
    active_player: 'p0',
    priority_player: 'p0',
    battlefield: [
      creature('perm_bear', 'Grizzly Bears', 'p0'),
      {
        id: 'perm_ajani',
        controller: 'p1',
        owner: 'p1',
        card: {
          id: 'perm_ajani',
          name: 'Ajani, Adversary of Tyrants',
          type_line: 'Legendary Planeswalker — Ajani',
          loyalty: '5',
        },
        counters: [{ kind: 'loyalty', count: 4 }],
      },
    ],
    valid_actions: [
      {
        id: 'atk',
        type: 'declare_attackers',
        label: 'Declare attackers',
        token: 'tcombat',
        requirements: [
          {
            slot: 'attackers',
            prompt: 'Choose which creatures attack',
            candidates: ['perm_bear'],
          },
          {
            slot: 'defend_perm_bear',
            prompt: 'Choose what Grizzly Bears attacks',
            candidates: ['p1', 'perm_ajani'],
          },
        ],
      },
    ],
  }

  test('answers two slots from the board, each click going where the server put it', async ({
    page,
  }) => {
    const { sent } = await serveFrames(page, [COMBAT])
    await page.goto('/')

    // Exact: the turn strip also carries a step by this name, and it is a different control.
    await page.getByRole('button', { name: 'Declare attackers', exact: true }).click()
    const choices = page.getByRole('region', { name: 'Choices' })
    const yourBoard = page.getByRole('region', { name: 'Your battlefield' })
    const theirBoard = page.getByRole('region', { name: 'p1 battlefield' })

    // Both questions are open at once, so both slots' candidates light up. There is no cursor
    // to advance and no order to guess: the bear is in `attackers`, Ajani is in the bear's
    // defender slot, and clicking either goes where the server put it.
    await expect(yourBoard.getByRole('button', { name: /^Grizzly Bears/ })).toHaveClass(
      /card--candidate/,
    )
    await expect(theirBoard.getByRole('button', { name: /^Ajani/ })).toHaveClass(/card--candidate/)

    await theirBoard.getByRole('button', { name: /^Ajani/ }).click()
    await expect(choices).toContainText('Choose what Grizzly Bears attacks — 1 chosen')
    await yourBoard.getByRole('button', { name: /^Grizzly Bears/ }).click()
    await expect(choices).toContainText('Choose which creatures attack — 1 chosen')

    await page.getByRole('button', { name: 'Confirm' }).click()
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'atk',
      token: 'tcombat',
      // In the order they were answered, which is the player's and not the list's.
      targets: [
        { slot: 'defend_perm_bear', chosen: ['perm_ajani'] },
        { slot: 'attackers', chosen: ['perm_bear'] },
      ],
    })
  })

  test('lets an attack be declared with nobody attacking', async ({ page }) => {
    // A requirement publishes no count and declaring no attackers is a legal declaration, so
    // an empty answer must be sendable. A client that required a target here would make the
    // most common combat decision in the game impossible.
    const { sent } = await serveFrames(page, [COMBAT])
    await page.goto('/')

    // Exact: the turn strip also carries a step by this name, and it is a different control.
    await page.getByRole('button', { name: 'Declare attackers', exact: true }).click()
    await page.getByRole('button', { name: 'Confirm' }).click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toEqual({
      type: 'choose_action',
      action_id: 'atk',
      token: 'tcombat',
      submission: 's:1',
    })
  })

  test('builds an ordering by clicking in the order wanted', async ({ page }) => {
    const { sent } = await serveFrames(page, [
      {
        ...COMBAT,
        phase: 'combat_damage',
        battlefield: [
          creature('perm_bear', 'Grizzly Bears', 'p0'),
          creature('perm_wall', 'Wall of Vines', 'p1'),
          creature('perm_ogre', 'Onakke Ogre', 'p1'),
        ],
        valid_actions: [
          {
            id: 'ord',
            type: 'order_combat_damage',
            label: 'Order blockers',
            token: 'torder',
            prompts: [
              {
                kind: 'order',
                slot: 'order',
                prompt: 'Order the blockers of Grizzly Bears',
                items: ['perm_wall', 'perm_ogre'],
              },
            ],
          },
        ],
      },
    ])
    await page.goto('/')

    await page.getByRole('button', { name: 'Order blockers' }).click()
    const choices = page.getByRole('region', { name: 'Choices' })
    await expect(choices).toContainText('0 of 2, in order')

    // A permutation is only complete when every item has a place, and where each one sits has
    // to be readable from the control itself.
    const opponentBoard = page.getByRole('region', { name: 'p1 battlefield' })
    await opponentBoard.getByRole('button', { name: /^Onakke Ogre/ }).click()
    await expect(page.getByRole('button', { name: 'Confirm' })).toBeDisabled()
    await opponentBoard.getByRole('button', { name: /^Wall of Vines/ }).click()
    await expect(choices.getByRole('button', { name: '1. Onakke Ogre' })).toBeVisible()
    await expect(choices.getByRole('button', { name: '2. Wall of Vines' })).toBeVisible()

    await page.getByRole('button', { name: 'Confirm' }).click()
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      targets: [{ slot: 'order', chosen: ['perm_ogre', 'perm_wall'] }],
    })
  })
})

test.describe('a submission the server has not answered', () => {
  test('says what is in flight and refuses to send it twice', async ({ page }) => {
    const { sent, push } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Pass' }).click()
    const actions = page.getByRole('region', { name: 'Actions' })
    await expect(actions).toContainText('Sent “Pass” — waiting for the server')
    await expect(actions.getByRole('button', { name: 'Pass' })).toBeDisabled()

    // A second click on a disabled control is a no-op; the count proves it never reached the
    // socket, which is what "prevents accidental duplicate submission" has to mean.
    await actions.getByRole('button', { name: 'Pass' }).click({ force: true })
    expect(submissions(sent)).toHaveLength(1)

    // The server's own echo releases it, and only its own: an id that is not ours changes
    // nothing, because an ack-less broadcast says nothing about a submission in flight.
    push({ ...fixture('gameview.json'), action_ack: { submission: 's:9', accepted: true } })
    await expect(actions).toContainText('waiting for the server')

    push({ ...fixture('gameview.json'), action_ack: { submission: 's:1', accepted: true } })
    await expect(actions).not.toContainText('waiting for the server')
    await expect(actions.getByRole('button', { name: 'Pass' })).toBeEnabled()
  })

  test('names what was refused, and leaves the player able to carry on', async ({ page }) => {
    const { push } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Pass' }).click()
    push({
      ...fixture('gameview.json'),
      action_rejected: true,
      action_ack: { submission: 's:1', accepted: false },
    })

    // Two statements, and both are needed: the header says the state did not move, the dock
    // says which of the player's clicks it was that did not move it.
    await expect(page.getByText('That action could not be taken')).toBeVisible()
    const actions = page.getByRole('region', { name: 'Actions' })
    await expect(actions).toContainText('“Pass” was refused')
    await expect(actions.getByRole('button', { name: 'Pass' })).toBeEnabled()
  })

  test('can be given up on when no answer is coming', async ({ page }) => {
    // An older server sends no ack at all, so waiting is correct and there has to be a way out
    // of it that does not involve reloading the page.
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page.getByRole('button', { name: 'Pass' }).click()
    await page.getByRole('button', { name: 'Stop waiting' }).click()

    await expect(page.getByRole('button', { name: 'Pass' })).toBeEnabled()
  })
})

test.describe('the table as a composition', () => {
  // Desktop landscape is the one layout (`docs/brief.md`), so it is the one these are sized to.
  test('seats both players with their own half of the board', async ({ page }) => {
    // The point of a table over a state dump: a permanent's controller is answered by where
    // the card is. `gameview-commander.json` has one permanent per opponent and none of yours,
    // so a client that pooled them into one list would put Bob's commander on your side.
    await page.setViewportSize(DESKTOP)
    await serveFrames(page, [fixture('gameview-commander.json')])
    await page.goto('/')

    await expect(page.getByRole('region', { name: /^Bob .* battlefield/ })).toContainText(
      'Lathliss, Dragon Queen',
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

    // Exile is yours in this fixture and the graveyard is the opponent's; each is counted in
    // front of the seat that owns it rather than listed somewhere central where ownership
    // would be a label. Opening one is a click on that seat's own control.
    const mine = page.getByRole('region', { name: 'Your seat' })
    await expect(mine.getByRole('button', { name: 'Exile (1)' })).toBeVisible()
    await mine.getByRole('button', { name: 'Exile (1)' }).click()
    await expect(page.getByRole('region', { name: 'Exile' })).toContainText('Path to Exile')

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
    // One entry per turn the path crossed, and inside it the steps in the order the settle
    // acted — the repeat preserved rather than collapsed.
    const runs = settle.getByRole('listitem')
    await expect(runs).toHaveCount(2)
    await expect(runs.nth(0)).toHaveText('Turn 3 — Begin combat → Declare attackers → Begin combat')
    await expect(runs.nth(1)).toHaveText('Turn 4 — Upkeep')
  })
})

test.describe('the pre-game screen', () => {
  test('renders a lobby and offers only the commands the server allows', async ({ page }) => {
    await serveFrames(page, [
      { session: 's1', you: 'p0', valid_commands: ['create_room', 'join_room'] },
    ])
    await page.goto('/')

    await expect(page.getByRole('button', { name: 'Create a table' })).toBeVisible()
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
    await expect(page.getByRole('button', { name: 'Create a table' })).toBeVisible()
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
    await expect(page.getByRole('button', { name: 'Create a table' })).toBeVisible()
  })
})
