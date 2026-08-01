/**
 * Relationships and zones, driven by committed fixtures over an intercepted WebSocket.
 *
 * The other half of the non-blocking view tier (ADR 0011); the harness and the rest of it are
 * in `frames.ts` and `views.spec.ts`. What is asserted here is everything that is *between*
 * objects rather than about one — combat, targets, attachments, sources — and the surfaces that
 * browse a zone. Every relationship checked is one the **server stated**: none of it is
 * available to a client that reads rules text or log lines instead.
 */
import { expect, test, type Page } from '@playwright/test'

import { DESKTOP, fixture, pageFits, serveFrames, submissions } from './frames'

/**
 * The dense board: `gameview-board.json` in a real browser.
 *
 * One frame carrying everything a client has to be able to draw at once — a seven-deep stack,
 * all four target tags, two creatures blocking one attacker, an attack on a planeswalker, an
 * Aura across the table and an Equipment at home, counters, marked damage, tokens, an emblem,
 * and a ten-card graveyard. Every relationship asserted here is one the *server stated*; none of
 * it is available to a client that reads rules text or log lines instead.
 */
test.describe('a board with relationships to trace', () => {
  const MINE = 'Your battlefield'
  const THEIRS = 'Bo (p2) battlefield'

  /**
   * One object's tile, addressed by the card frame inside it rather than by its text.
   *
   * Text is ambiguous here on purpose: a trail names other objects, so the Equipment's tile and
   * the creature it is attached to both contain the words "Marauder's Axe". The card frame's own
   * accessible name begins with the card's name, and a trail control's begins with the phrase
   * that put it there, so the frame is what identifies the tile.
   */
  const tile = (page: Page, region: string, name: string) =>
    page
      .getByRole('region', { name: region })
      .getByRole('listitem')
      .filter({ has: page.getByRole('button', { name: new RegExp(`^${name}`) }) })

  const board = async (page: Page) => {
    await page.setViewportSize(DESKTOP)
    const served = await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')
    return served
  }

  test('gives an attacker the blockers that no field on it states', async ({ page }) => {
    await board(page)

    // `blocking` rides on each blocker and the attacker carries nothing at all about being
    // blocked, so this line only exists because the client indexed the relationship from both
    // ends. Two blockers on one attacker is the case that makes it worth doing.
    const ogre = tile(page, MINE, 'Onakke Ogre')
    await expect(ogre).toContainText('blocked by')
    await expect(ogre.getByRole('button', { name: 'blocked by Colossal Dreadmaw' })).toBeVisible()
    await expect(ogre.getByRole('button', { name: 'blocked by Zombie' })).toBeVisible()

    // And the direction the wire actually stated, on the blocker itself.
    await expect(
      tile(page, THEIRS, 'Colossal Dreadmaw').getByRole('button', {
        name: 'blocking Onakke Ogre',
      }),
    ).toBeVisible()
  })

  test('points an attack at the planeswalker rather than at the seat behind it', async ({
    page,
  }) => {
    await board(page)

    // The view states both for an attack on a planeswalker: the seat that answers for it, and
    // the planeswalker the damage is going to. Drawing the seat would put the arrow on the
    // wrong object, and it is the distinction that makes a planeswalker worth attacking.
    await expect(
      tile(page, THEIRS, 'Vivien Reid').getByRole('button', { name: 'attacked by Onakke Ogre' }),
    ).toBeVisible()

    const seat = page.getByRole('region', { name: 'Bo (p2) seat' })
    await expect(seat.getByRole('button', { name: 'attacked by Serra Angel' })).toBeVisible()
    await expect(seat.getByRole('button', { name: 'attacked by Thopter' })).toBeVisible()
    await expect(seat.getByRole('button', { name: 'attacked by Onakke Ogre' })).toHaveCount(0)

    // A seat is on the other end of a target the same way a permanent is.
    await expect(seat).toContainText('targeted by')

    // The planeswalker shows the loyalty it has, not the loyalty it was printed with.
    await expect(tile(page, THEIRS, 'Vivien Reid')).toContainText('3')
    await expect(tile(page, THEIRS, 'Vivien Reid')).not.toContainText('5')
  })

  test('traces an attachment from both ends, across the table', async ({ page }) => {
    await board(page)

    // The Aura is controlled by one player and attached to the other's creature, which is
    // exactly when "where the card sits" stops answering the question and the trail has to.
    await expect(
      tile(page, MINE, 'Pacifism').getByRole('button', { name: 'attached to Gearsmith Guardian' }),
    ).toBeVisible()
    await expect(
      tile(page, THEIRS, 'Gearsmith Guardian').getByRole('button', { name: 'attached Pacifism' }),
    ).toBeVisible()

    // Equipment reads identically. Which of the two it is is a question about the card, and
    // the client does not read cards.
    await expect(
      tile(page, MINE, "Marauder's Axe").getByRole('button', { name: 'attached to Serra Angel' }),
    ).toBeVisible()
    await expect(
      tile(page, MINE, 'Serra Angel').getByRole('button', { name: "attached Marauder's Axe" }),
    ).toBeVisible()
  })

  test('reads the stack top down, with each object’s source and targets', async ({ page }) => {
    await board(page)

    const objects = page.getByRole('region', { name: 'Stack' }).getByRole('listitem')
    await expect(objects).toHaveCount(7)

    // The wire lists the stack bottom first. What resolves next is what a player needs, and it
    // says so in words rather than leaving it to be inferred from which end of the column it
    // is on — getting that backwards is the difference between holding priority and losing.
    const top = objects.first()
    await expect(top).toContainText('Resolves next')
    await expect(top).toContainText('Equipped creature deals 1 damage to each of two targets.')
    // An ability has no card of its own, so the source is the only link back to what made it.
    await expect(top.getByRole('button', { name: "from Marauder's Axe" })).toBeVisible()
    await expect(top.getByRole('button', { name: 'targeting Air Elemental' })).toBeVisible()
    await expect(top.getByRole('button', { name: 'targeting Bo (p2)' })).toBeVisible()

    // The bottom knows its position, and knows what named it — a spell and the counterspell
    // aimed at it trace to each other without either reading the other's rules text.
    const bottom = objects.last()
    await expect(bottom).toContainText('7 of 7')
    await expect(bottom.getByRole('button', { name: 'targeting Colossal Dreadmaw' })).toBeVisible()
    await expect(
      bottom.getByRole('button', { name: 'targeted by Cancel targeting Divine Verdict' }),
    ).toBeVisible()

    // A card in a graveyard is a target like anything else, and is named as the card it is.
    await expect(
      objects.nth(4).getByRole('button', { name: 'targeting Llanowar Elves' }),
    ).toBeVisible()
  })

  test('reaches an object through the relationship that names it', async ({ page }) => {
    const { sent } = await board(page)

    // The traversal, and the reason the trail is controls rather than text: the other end of a
    // relationship is often across the table or inside a pile, and this is the way to it.
    // Reading it is not a game action.
    await page
      .getByRole('region', { name: 'Stack' })
      .getByRole('listitem')
      .last()
      .getByRole('button', { name: 'targeting Colossal Dreadmaw' })
      .click()

    await expect(page.getByRole('dialog')).toContainText('Colossal Dreadmaw')
    expect(submissions(sent)).toEqual([])
  })

  test('emphasises what a looked-at object relates to, and nothing else', async ({ page }) => {
    await board(page)

    // Tracing follows the look rather than the click, because the objects most worth tracing
    // own no action: clicking a blocker opens the inspector over the very board the
    // relationship crosses.
    await tile(page, MINE, 'Onakke Ogre')
      .getByRole('button', { name: /^Onakke Ogre/ })
      .hover()

    const frame = (region: string, name: string) =>
      tile(page, region, name).getByRole('button', { name: new RegExp(`^${name}`) })

    await expect(frame(THEIRS, 'Colossal Dreadmaw')).toHaveClass(/card--linked/)
    await expect(frame(THEIRS, 'Zombie')).toHaveClass(/card--linked/)
    await expect(frame(THEIRS, 'Vivien Reid')).toHaveClass(/card--linked/)
    // Blocking a different attacker is not a relationship with this one.
    await expect(frame(THEIRS, 'Air Elemental')).not.toHaveClass(/card--linked/)
    await expect(frame(MINE, 'Serra Angel')).not.toHaveClass(/card--linked/)
  })

  /** One drawn relationship, addressed by the two ids the server stated for its ends. */
  const line = (page: Page, from: string, to: string) =>
    page.locator(`.overlay line[data-from="${from}"][data-to="${to}"]`)

  /**
   * The same fixture, on a screen with room for all of it.
   *
   * The drawn board deliberately points at nothing a region has scrolled away — an arrow to a
   * card the player cannot see is worse than the sentence under it, which is still there. This
   * fixture is the maximal board and does not fit in `DESKTOP`, so these cases give it a screen
   * where every object is showing and the geometry is what is being tested.
   */
  const wholeBoard = async (page: Page) => {
    await page.setViewportSize({ width: 1600, height: 1200 })
    const served = await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')
    return served
  }

  // Counted rather than asserted visible: a line straight up the board has a zero-width box,
  // which a visibility heuristic reads as hidden and a player reads as a line.
  test('joins combat with drawn lines, between the ids the view stated', async ({ page }) => {
    await wholeBoard(page)

    // The picture of combat, and the whole of what it is allowed to be: a segment between two
    // objects the *server* related. Both directions of the same exchange are here — an attack
    // the attacker states, and the blocks that only its blockers state.
    await expect(line(page, 'perm_serra', 'p2')).toHaveCount(1)
    await expect(line(page, 'perm_dreadmaw', 'perm_serra')).toHaveCount(0)
    await expect(line(page, 'perm_elemental', 'perm_serra')).toHaveCount(1)
    await expect(line(page, 'perm_dreadmaw', 'perm_ogre')).toHaveCount(1)
    await expect(line(page, 'perm_zombie', 'perm_ogre')).toHaveCount(1)

    // An attack on a planeswalker points at the planeswalker rather than at the seat that
    // answers for it, exactly as the trail says it in words.
    await expect(line(page, 'perm_ogre', 'perm_vivien')).toHaveCount(1)
    await expect(line(page, 'perm_ogre', 'p2')).toHaveCount(0)

    // The same overlay draws the stack: a spell to what it named, an ability to the permanent
    // it came from, an Aura to what it is attached to. The object that resolves next is at the
    // top of the rail, so it is the one whose position no amount of stack is allowed to move.
    await expect(line(page, 's7', 'perm_elemental')).toHaveCount(1)
    await expect(line(page, 's7', 'p2')).toHaveCount(1)
    await expect(line(page, 's7', 'perm_axe')).toHaveCount(1)
    await expect(line(page, 'perm_pacifism', 'perm_gearsmith')).toHaveCount(1)
    await expect(line(page, 'perm_axe', 'perm_serra')).toHaveCount(1)

    // A real segment between two boxes, not a point: the geometry ran against a laid-out board.
    const drawn = line(page, 'perm_dreadmaw', 'perm_ogre')
    const [x1, y1, x2, y2] = await Promise.all([
      drawn.getAttribute('x1'),
      drawn.getAttribute('y1'),
      drawn.getAttribute('x2'),
      drawn.getAttribute('y2'),
    ])
    expect(`${x1},${y1}`).not.toEqual(`${x2},${y2}`)
    expect(Number(y1)).toBeGreaterThan(0)
  })

  test('draws nothing an id could not be resolved for, and nothing extra', async ({ page }) => {
    await wholeBoard(page)

    // The Gravedigger ability names a card in a graveyard nobody has opened. There is no box to
    // point at, and the trail under it still names the card in words — which is the copy of the
    // fact that never depends on where anything is drawn.
    await expect(line(page, 's3', 'g2')).toHaveCount(0)

    // And every line that *is* drawn is one the view stated. Which of them are on screen is a
    // question about layout — a seven-deep stack scrolls inside its own rail even here — but
    // nothing outside this set can ever appear, whatever the layout does, because an edge with
    // no stated identifier behind it has nowhere to come from.
    const stated = [
      'perm_serra->p2',
      'perm_ogre->perm_vivien',
      'perm_thopter->p2',
      'perm_axe->perm_serra',
      'perm_pacifism->perm_gearsmith',
      'perm_dreadmaw->perm_ogre',
      'perm_elemental->perm_serra',
      'perm_zombie->perm_ogre',
      's1->perm_dreadmaw',
      's2->s1',
      's3->perm_gravedigger',
      's4->perm_serra',
      's5->perm_vivien',
      's6->p2',
      's7->perm_axe',
      's7->perm_elemental',
      's7->p2',
    ]
    const drawn = await page
      .locator('.overlay line')
      .evaluateAll((lines) =>
        lines.map(
          (element) => `${element.getAttribute('data-from')}->${element.getAttribute('data-to')}`,
        ),
      )
    expect(drawn.filter((edge) => !stated.includes(edge))).toEqual([])
  })

  test('keeps the drawn board out of the way of reading it and clicking it', async ({ page }) => {
    await wholeBoard(page)

    // A drawn arrow is not readable, so the overlay is hidden from assistive technology and the
    // trail under each card stays the accessible equivalent rather than being replaced by it.
    const overlay = page.locator('.overlay')
    await expect(overlay).toHaveAttribute('aria-hidden', 'true')
    await expect(
      tile(page, MINE, 'Onakke Ogre').getByRole('button', { name: 'attacking Vivien Reid' }),
    ).toBeVisible()

    // And it takes no clicks: it lies over the whole table, and everything under it has to stay
    // exactly as reachable as it was before anything was drawn.
    await expect(overlay).toHaveCSS('pointer-events', 'none')
    await tile(page, THEIRS, 'Colossal Dreadmaw')
      .getByRole('button', { name: /^Colossal Dreadmaw/ })
      .click()
    await expect(page.getByRole('dialog')).toContainText('Colossal Dreadmaw')
  })

  test('raises the lines of what is being looked at, and steps the rest back', async ({ page }) => {
    await wholeBoard(page)

    await tile(page, MINE, 'Onakke Ogre')
      .getByRole('button', { name: /^Onakke Ogre/ })
      .hover()

    // The same look that emphasises the related cards emphasises the lines to them, because
    // both read from the one join. Either end counts: the Ogre states its attack and states
    // nothing at all about having been blocked.
    await expect(line(page, 'perm_ogre', 'perm_vivien')).toHaveClass(/overlay__edge--traced/)
    await expect(line(page, 'perm_dreadmaw', 'perm_ogre')).toHaveClass(/overlay__edge--traced/)
    await expect(line(page, 'perm_zombie', 'perm_ogre')).toHaveClass(/overlay__edge--traced/)
    await expect(line(page, 'perm_elemental', 'perm_serra')).toHaveClass(/overlay__edge--dimmed/)
  })

  test('draws counters, damage, tokens, and emblems on one dense frame', async ({ page }) => {
    await board(page)

    // Each is a separate fact about a permanent and each has to survive a board this full.
    await expect(tile(page, THEIRS, 'Gearsmith Guardian')).toContainText('2 damage')
    await expect(tile(page, THEIRS, 'Gearsmith Guardian')).toContainText('1× -1/-1')
    await expect(tile(page, THEIRS, 'Zombie')).toContainText('Token')
    await expect(tile(page, MINE, 'Onakke Ogre')).toContainText('Tapped')
    await expect(tile(page, MINE, 'Serra Angel')).toContainText('1× +1/+1')

    await expect(page.getByRole('region', { name: 'Emblems' })).toContainText(
      'Creatures you control get +1/+1.',
    )

    // Restricted mana (CR 106.6) rides the pool as a pip suffixed `*`. The suffix is not a mana
    // symbol, so it reaches the screen as a *marked* pip with the sentence that explains it —
    // an asterisk on its own is a fact a player has no way to read.
    const pool = page.getByRole('region', { name: 'Your seat' }).getByText(/^Pool:/)
    // Floating mana is drawn with the same discs a card's cost is, and reaches assistive
    // technology as the same words, because a pool and the cost it is about to pay are the one
    // comparison a player makes constantly.
    await expect(pool.getByRole('img', { name: 'red mana' })).toBeVisible()
    await expect(pool.getByRole('img', { name: 'green mana' })).toBeVisible()
    await expect(pool).toContainText('restricted')
    await expect(pool).toContainText('spendable only on what made it')

    // None of it grew the page: every region still scrolls inside its own area.
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })
})

/**
 * The same board, for a player who has asked their system for less motion.
 *
 * The request is honoured by arriving at the same state instantly rather than by arriving
 * somewhere else — so what is asserted here is that the drawn board is *identical*, and only the
 * time it takes to get there is not.
 */
test.describe('the board a reduced-motion request reaches', () => {
  test('reaches the same lines, without the fade that raises them', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.setViewportSize({ width: 1600, height: 1200 })
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')

    const edge = page.locator('.overlay line[data-from="perm_dreadmaw"][data-to="perm_ogre"]')
    await expect(edge).toHaveCount(1)
    // 1ms rather than none: the state is reached, and it is reached at once.
    await expect(edge).toHaveCSS('transition-duration', '0.001s, 0.001s')

    // And the emphasis a look produces still happens — it is a state, not an animation.
    await page
      .getByRole('region', { name: 'Your battlefield' })
      .getByRole('button', { name: /^Onakke Ogre/ })
      .hover()
    await expect(edge).toHaveClass(/overlay__edge--traced/)
  })
})

test.describe('browsing what a player is allowed to see', () => {
  test('opens a public pile beside the table, never over it', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 })
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')

    const seat = page.getByRole('region', { name: 'Your seat' })
    await seat.getByRole('button', { name: 'Graveyard (10)' }).click()

    const pile = page.getByRole('region', { name: 'Graveyard' })
    await expect(pile.getByRole('listitem')).toHaveCount(10)
    await expect(pile).toContainText('Ada (p1)')
    await expect(pile).toContainText('Llanowar Elves')

    // Beside, not over: a pile is usually what a player is choosing *from* while the dock asks
    // the question, so covering the board or the controls would make the two halves of one
    // decision take turns.
    await expect(page.getByRole('region', { name: 'Your battlefield' })).toBeVisible()
    await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()

    await pile.getByRole('button', { name: 'Close' }).click()
    await expect(page.getByRole('region', { name: 'Graveyard' })).toHaveCount(0)
  })

  test('states a hidden zone as a count with nothing to open', async ({ page }) => {
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')

    // The rule the zone row carries. A library and an opponent's hand are hidden zones: the
    // view projects a count and no cards, so a count is all there is to draw and there is
    // nothing to open. A player must be able to tell that apart from a pile they may read
    // without clicking to find out.
    const mine = page.getByRole('region', { name: 'Your seat' })
    await expect(mine).toContainText('Library (31)')
    await expect(mine.getByRole('button', { name: /^Library/ })).toHaveCount(0)

    const theirs = page.getByRole('region', { name: 'Bo (p2) seat' })
    await expect(theirs).toContainText('Hand (4)')
    await expect(theirs.getByRole('button', { name: /^Hand/ })).toHaveCount(0)
    await expect(theirs).toContainText('Library (28)')

    // Their exile *was* itemized, so it opens; their graveyard was only counted, so it does
    // not — the client browses what it was sent cards for and never claims more.
    await expect(theirs.getByRole('button', { name: 'Exile (2)' })).toBeVisible()
    await expect(theirs).toContainText('Graveyard (3)')
    await expect(theirs.getByRole('button', { name: /^Graveyard/ })).toHaveCount(0)
  })

  test('opens a command zone through the same surface', async ({ page }) => {
    // The command zone is the same shape as any other public pile, and gets the same control.
    const base = fixture('gameview-commander.json')
    await serveFrames(page, [
      {
        ...base,
        command: [
          {
            player_id: 'p0',
            cards: [
              {
                id: 'cmd1',
                name: 'Kalamax, the Stormsire',
                type_line: 'Legendary Creature — Elemental Dinosaur',
                mana_cost: '{1}{G}{U}{R}',
              },
            ],
          },
        ],
      },
    ])
    await page.goto('/')

    await page
      .getByRole('region', { name: 'Your seat' })
      .getByRole('button', { name: 'Command (1)' })
      .click()
    await expect(page.getByRole('region', { name: 'Command' })).toContainText('Kalamax')
  })

  test('shows a choice’s revealed cards on the same browsing surface', async ({ page }) => {
    // Cards the server is showing this seat alone, while a choice asks about them. Same
    // surface as a pile, because it is the same thing to a player — an ordered set of faces to
    // read and choose from — and it is not something they opened, so there is nothing to close.
    await serveFrames(page, [fixture('gameview-choice.json')])
    await page.goto('/')

    const shown = page.getByRole('region', { name: 'Shown to you' })
    await expect(shown.getByRole('listitem').first()).toBeVisible()
    await expect(shown.getByRole('button', { name: 'Close' })).toHaveCount(0)
  })
})
