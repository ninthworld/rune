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

import { DESKTOP, fixture, messages, pageFits, serveFrames, submissions } from './frames'

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

    // The fourth is drawn rather than written (§6): tapped is the mark across the face, and the
    // word is what a screen reader is told in its place.
    await expect(bear.getByRole('button', { name: /^Grizzly Bears/ })).toHaveAccessibleName(
      /Tapped/,
    )
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

    // Everything the hand tile clamps or drops is here, in full — with the wire's `{T}` and
    // `{G}` drawn as the pips a player reads, so the sentence between them is what remains.
    const inspector = page.getByRole('dialog')
    await expect(inspector).toContainText('Llanowar Elves')
    await expect(inspector).toContainText(': Add')
    await expect(inspector.getByRole('img', { name: /tap/ })).toBeVisible()

    // Inspecting is not a game action. Asserted against what was *sent* rather than a message
    // count, because the client's own `hello` races the click and would count as traffic.
    expect(submissions(sent)).toEqual([])

    await page.keyboard.press('Escape')
    await expect(inspector).toHaveCount(0)
  })

  test('casts a card the server offered one action for, on one click', async ({ page }) => {
    // One action is one meaning, so the click is that meaning. What is sent is the id the
    // server issued for it and nothing the client composed.
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })
      .click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({ type: 'choose_action', action_id: 'a3' })
  })

  test('reads a card without spending a click on it', async ({ page }) => {
    // Which is what makes the click above safe: a card whose one action now fires immediately
    // has to stay readable, so reading moved to gestures that cost nothing — the pointer, and
    // the right-click that works on any object whatever else is in progress.
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bolt = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })

    await bolt.hover()
    await expect(page.getByRole('complementary')).toContainText(
      'Lightning Bolt deals 3 damage to any target.',
    )

    await bolt.click({ button: 'right' })
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
  test('plays the land the click was on, and only that one', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Forest/ })
      .click()

    // The action the *clicked object* owns, with the token the server issued for it. The other
    // card's action is the other card's business: a click is never a menu of everything legal.
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

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({ action_id: 'a3' })
  })

  test('highlights the candidates for the open slot, and nothing else', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    const hand = page.getByRole('region', { name: 'Your hand' })
    // One click arms it, because the server offered exactly one action for this card and that
    // action asks questions before it can be sent.
    await hand.getByRole('button', { name: /^Emberfall Surge/ }).click()

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

    // The braces are pips now, in an action's label as much as in a card's text, so the name
    // this control answers to is the sentence with the symbol read aloud in it.
    await page.getByRole('button', { name: 'Pay 1 to draw a card?' }).click()
    const choices = page.getByRole('region', { name: 'Choices' })
    await expect(choices.getByRole('button', { name: 'Pay 1' })).toBeVisible()
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

test.describe('a choice the game will not proceed past', () => {
  /**
   * A trigger on the stack waiting to be aimed (`docs/protocol.md`, `choose_targets`): the seat
   * is offered this and a concede, and nothing else — no pass, because play does not continue
   * around it. There is no committed fixture for it, so the view is written out here.
   */
  const AIMING = {
    you: 'p0',
    phase: 'precombat_main',
    turn: 4,
    me: { life: 20, library_size: 40 },
    opponents: [{ player_id: 'p1', life: 20, hand_size: 5, library_size: 40, graveyard_size: 0 }],
    seat_order: ['p0', 'p1'],
    active_player: 'p0',
    priority_player: 'p0',
    battlefield: [
      {
        id: 'perm_vamp',
        controller: 'p0',
        owner: 'p0',
        card: {
          id: 'perm_vamp',
          name: 'Skymarch Bloodletter',
          type_line: 'Creature — Vampire Soldier',
          power: '2',
          toughness: '2',
        },
      },
    ],
    stack: [
      {
        id: 'stack_9',
        controller: 'p0',
        kind: 'triggered',
        source: 'perm_vamp',
        description: 'Target opponent loses 1 life and you gain 1 life.',
      },
    ],
    valid_actions: [
      {
        id: 'aim',
        type: 'choose_targets',
        label: 'Target opponent loses 1 life and you gain 1 life.',
        subject: ['stack_9', 'perm_vamp'],
        token: 'taim',
        requirements: [{ slot: 't0', prompt: 'Target opponent', candidates: ['p1'] }],
      },
      { id: 'give', type: 'concede', label: 'Concede', token: 'tgive' },
    ],
  }

  test('is held out by the dock instead of left to be found on the board', async ({ page }) => {
    // The failure this exists for: the only thing the player could do was bound to a card, so
    // the dock offered nothing but a concede and the game read as stuck.
    await serveFrames(page, [AIMING])
    await page.goto('/')

    const owed = page.getByRole('list', { name: 'Actions you owe' })
    await expect(owed.getByRole('button', { name: /Target opponent loses 1 life/ })).toBeVisible()
    // And the hint that sends a player looking is gone, because there is nothing to look for.
    await expect(page.getByText('Click a highlighted card or player to act on it.')).toHaveCount(0)
  })

  test('is reachable from the trigger where it sits on the stack', async ({ page }) => {
    // Where a player looks first: the stack says something is waiting, so clicking that is the
    // gesture. Binding the action to its source permanent alone did not answer this click.
    const { sent } = await serveFrames(page, [AIMING])
    await page.goto('/')

    const stack = page.getByRole('region', { name: 'Stack' })
    await expect(stack.getByRole('button').first()).toHaveClass(/card--candidate/)

    // One action, so the click on the stack object *is* arming it — and because that action
    // asks a question, what the click reaches is the question rather than a submission.
    await stack.getByRole('button').first().click()
    await expect(page.getByRole('region', { name: 'Choices' })).toContainText(
      'Target opponent loses 1 life',
    )

    await page.getByRole('region', { name: 'p1 seat' }).getByRole('button', { name: 'p1' }).click()
    await page.getByRole('button', { name: 'Confirm' }).click()

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({
      action_id: 'aim',
      token: 'taim',
      targets: [{ slot: 't0', chosen: ['p1'] }],
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

    const settle = page.getByRole('region', { name: 'While you were passed' })
    await expect(settle).toBeVisible()
    // One entry per turn the path crossed, and inside it the steps in the order the settle
    // acted — the repeat preserved rather than collapsed. Scoped to the path, since the panel
    // now leads with the *events* of the stretch and the steps are the context under them.
    const runs = settle.locator('.side__path').getByRole('listitem')
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

test.describe('the keyboard, and what the frame draws', () => {
  test('passes priority on space', async ({ page }) => {
    // The most-pressed control in a game of Magic, under a thumb. What it sends is the action
    // the server listed as the pass, by the id the server issued for it.
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('heading', { name: /^Turn \d+ — / })).toBeVisible()

    await page.keyboard.press(' ')

    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({ type: 'choose_action', action_id: 'a1' })
  })

  test('a skip key sets the stop preference and hands the game back', async ({ page }) => {
    // Two messages and no loop: the preference the server will honour, and one pass. Everything
    // after that is the server's settle acting on a preference it stores (ADR 0010) — nothing
    // in this client decides that a step was uninteresting.
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('heading', { name: /^Turn \d+ — / })).toBeVisible()

    await page.keyboard.press('F5')

    // A bare `set_stops` is the protocol's own spelling of "stop nowhere".
    await expect.poll(() => messages(sent, 'set_stops')).toEqual([{ type: 'set_stops' }])
    await expect.poll(() => submissions(sent)).toHaveLength(1)
    expect(submissions(sent)[0]).toMatchObject({ action_id: 'a1' })
  })

  test('asking to stop everywhere does not also skip', async ({ page }) => {
    // The opposite request: passing on top of it would skip the very step just asked for.
    const { sent } = await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')
    await expect(page.getByRole('heading', { name: /^Turn \d+ — / })).toBeVisible()

    await page.keyboard.press('F3')

    await expect.poll(() => messages(sent, 'set_stops')).toHaveLength(1)
    expect(submissions(sent)).toEqual([])
  })

  test('escape backs out of an armed action without sending anything', async ({ page }) => {
    const { sent } = await serveFrames(page, [fixture('gameview-actions.json')])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Emberfall Surge/ })
      .click()
    await expect(page.getByRole('button', { name: 'Confirm' })).toBeVisible()

    await page.keyboard.press('Escape')

    await expect(page.getByRole('button', { name: 'Confirm' })).toHaveCount(0)
    expect(submissions(sent)).toEqual([])
  })

  test('draws a cost as pips without losing it from the name', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const bolt = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Lightning Bolt/ })

    // The pips are a labelled image: a disc with an R in it says nothing when read out, so the
    // cost reaches assistive technology as words instead.
    await expect(bolt.getByRole('img', { name: 'red mana' })).toBeVisible()
  })

  test('marks a tapped permanent across the face, and never turns it', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    const field = page.getByRole('region', { name: 'Your battlefield' })
    const bear = field.getByRole('button', { name: /^Grizzly Bears/ })
    const thopter = field.getByRole('button', { name: /^Thopter/ })

    // Nothing is turned. Asserted as the matrix the browser resolved rather than as a
    // declaration, so a rotation reintroduced anywhere above this element fails here.
    await expect.poll(() => bear.evaluate((card) => getComputedStyle(card).transform)).toBe('none')

    // The mark itself, read off the frame's own layer. A *pattern*, not a tint: every fact on
    // this board has to survive being seen without colour, and a hatch does where a wash of
    // Canvas would not.
    const mark = (card: typeof bear) =>
      card.evaluate((node) => {
        const style = getComputedStyle(node, '::after')
        return { opacity: style.opacity, image: style.backgroundImage }
      })
    await expect
      .poll(() => mark(bear))
      .toMatchObject({
        opacity: '1',
        image: expect.stringContaining('repeating-linear-gradient'),
      })
    // And it is the tap that draws it, not the frame: the untapped permanent beside it carries
    // the same layer, unpainted.
    await expect.poll(() => mark(thopter)).toMatchObject({ opacity: '0' })

    // One shape per tile whatever the server said about it. A tapped permanent that cost more
    // room than an untapped one is the landscape footprint coming back by another route.
    const [tapped, upright] = [await bear.boundingBox(), await thopter.boundingBox()]
    expect(tapped?.width).toBeCloseTo(upright?.width ?? 0, 0)
    expect(tapped?.height).toBeCloseTo(upright?.height ?? 0, 0)

    // The name is horizontal and whole, which is the entire reason the turn went away.
    await expect(bear.locator('.card__name')).toHaveText('Grizzly Bears')

    // The badge is gone with it: the mark is the statement, and a pill repeating it spent the
    // frame's scarcest room on a fact the card already shows.
    await expect(page.locator('.badge--tapped')).toHaveCount(0)
    await expect(bear).toHaveAccessibleName(/Tapped/)
  })
})

test.describe('a battlefield with rows', () => {
  /** The board fixture, with a land added — no committed board has one to group. */
  const withLand = () => {
    const base = fixture('gameview.json')
    const battlefield = [
      ...(base.battlefield as Record<string, unknown>[]),
      {
        id: 'perm_forest',
        controller: 'p1',
        owner: 'p1',
        card: {
          id: 'c_forest_perm',
          name: 'Forest',
          type_line: 'Basic Land — Forest',
          card_types: ['land'],
          functional_id: 'forest',
        },
      },
    ]
    return { ...base, battlefield }
  }

  test('separates lands from creatures, from the types the server stated', async ({ page }) => {
    await serveFrames(page, [withLand()])
    await page.goto('/')

    // Named rows, not one wrapping list. The client parsed no type line to get here: the row
    // comes from `card_types`, which the server projects beside the sentence it renders.
    const field = page.getByRole('region', { name: 'Your battlefield' })
    await expect(field.getByRole('list', { name: 'Creatures' })).toContainText('Grizzly Bears')
    await expect(field.getByRole('list', { name: 'Lands' })).toContainText('Forest')

    // And each permanent is in exactly one of them.
    await expect(field.getByRole('list', { name: 'Lands' })).not.toContainText('Grizzly Bears')
  })

  test('draws creatures nearest the middle of the table, on both halves', async ({ page }) => {
    // So the two sets of creatures face each other across the dividing line and combat reads as
    // one band rather than two lists that happen to be stacked.
    await serveFrames(page, [withLand()])
    await page.goto('/')

    const field = page.getByRole('region', { name: 'Your battlefield' })
    const creatures = await field.getByRole('list', { name: 'Creatures' }).boundingBox()
    const lands = await field.getByRole('list', { name: 'Lands' }).boundingBox()

    // Your half is below the divider, so your creatures are the row nearer the top of it.
    expect(creatures && lands ? creatures.y < lands.y : false).toBe(true)
  })
})

test.describe('the band that says what the game wants', () => {
  test('names the state in words as well as in colour, with the step beside it', async ({
    page,
  }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // Colour is the shortcut; the words are what make it answerable at all, because a colour
    // nobody has learnt yet and a colour a screen reader cannot see both say nothing.
    const dock = page.getByRole('region', { name: 'Actions' })
    await expect(dock).toContainText('Your move')
    // The step, restated where the controls are — the header is a screen away from them.
    await expect(dock).toContainText('Turn 3 · Precombat main')
    await expect(page.locator('.dock')).toHaveClass(/dock--yours/)
  })

  test('turns to the confirming state before a match-ending action is sent', async ({ page }) => {
    const base = fixture('gameview.json')
    const conceding = {
      ...base,
      valid_actions: [
        ...(base.valid_actions as Record<string, unknown>[]),
        { id: 'a9', type: 'concede', label: 'Concede', subject: [] },
      ],
    }
    const { sent } = await serveFrames(page, [conceding])
    await page.goto('/')

    await page.getByRole('list', { name: 'Global actions' }).getByText('Concede').click()

    await expect(page.locator('.dock')).toHaveClass(/dock--confirm/)
    await expect(page.getByRole('region', { name: 'Actions' })).toContainText('cannot be undone')
    expect(submissions(sent)).toEqual([])
  })
})

test.describe('symbols where the wire writes braces', () => {
  test('draws a rules-text symbol as a pip a player can read', async ({ page }) => {
    await serveFrames(page, [fixture('gameview.json')])
    await page.goto('/')

    // `{T}: Add {G}.` is how the wire writes it; a card is not a debug dump.
    const elves = page
      .getByRole('region', { name: 'Your hand' })
      .getByRole('button', { name: /^Llanowar Elves/ })

    const rules = elves.locator('.card__rules')
    await expect(rules.getByRole('img', { name: /tap/ })).toBeVisible()
    await expect(rules.getByRole('img', { name: /green/ })).toBeVisible()
    // And the cost is still its own labelled row of pips, separately from the sentence.
    await expect(elves.locator('.card__cost')).toHaveAttribute('aria-label', 'green mana')
    // A pip in a sentence is a word in that sentence, so the sentence still reads as one.
    await expect(elves).toContainText(': Add')
  })
})

test.describe('the table at any size', () => {
  /**
   * A browser zoom shrinks the CSS viewport without shrinking a `rem`, so these are the same
   * screen at 100%, 120%, and 150%.
   *
   * The regression this pins: the hand was capped in `rem` and its cards were sized in `rem`,
   * so zooming in kept the hand claiming the same height out of a viewport that had less of it
   * to give — and the board lost the difference, with a land row disappearing under the hand.
   * Card sizes are now the smaller of a reading size and a share of the viewport, which is what
   * makes a card give ground when there is less screen to have.
   */
  for (const [zoom, size] of [
    ['100%', { width: 1440, height: 900 }],
    ['120%', { width: 1200, height: 750 }],
    ['150%', { width: 960, height: 600 }],
  ] as const) {
    test(`stacks the board, the controls, and the hand without overlap at ${zoom}`, async ({
      page,
    }) => {
      await page.setViewportSize(size)
      await serveFrames(page, [fixture('gameview.json')])
      await page.goto('/')
      await expect(page.getByRole('region', { name: 'Your hand' })).toBeVisible()

      const edges = async (selector: string) => {
        const box = await page.locator(selector).first().boundingBox()
        if (!box) throw new Error(`${selector} is not on the screen`)
        return { top: box.y, bottom: box.y + box.height }
      }
      const field = await edges('.field--you')
      const dock = await edges('.dock')
      const hand = await edges('.hand')

      // Each band ends where the next begins, and the last one ends at the bottom of the screen.
      expect(field.bottom).toBeLessThanOrEqual(dock.top + 1)
      expect(dock.bottom).toBeLessThanOrEqual(hand.top + 1)
      expect(hand.bottom).toBeLessThanOrEqual(size.height + 1)

      // And the board still has room to be a board rather than a sliver above the controls.
      expect(field.bottom - field.top).toBeGreaterThan(size.height * 0.1)
      expect(await pageFits(page)).toEqual({ x: true, y: true })
    })
  }
})
