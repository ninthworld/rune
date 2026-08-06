import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { GameView, type Permanent, type StackItem } from './protocol'
import { entityNames, relationLines, relationNote, relations } from './relations'
import { list } from './normalize'
import { seats } from './table'

const card = (id: string, name: string) => ({ id, name, type_line: 'Creature — Bear' })

const permanent = (id: string, name: string, extra: Partial<Permanent> = {}): Permanent => ({
  id,
  controller: 'p1',
  owner: 'p1',
  card: card(id, name),
  ...extra,
})

const view = (parts: Partial<GameView>): GameView => ({
  phase: 'declare_blockers',
  player_names: { p1: 'Ada', p2: 'Bo' },
  ...parts,
})

/** The trail one object draws, assembled exactly the way `Game.tsx` assembles it. */
const trail = (from: GameView, id: string) => relationLines(relations(from), id, entityNames(from))

describe('reading the board both ways', () => {
  it('gives an attacker the blockers that stopped it, which no field on it states', () => {
    // The whole reason this module exists. `blocking` rides on the blocker; the attacker
    // carries nothing at all about being blocked, and "how many blocked this one" is the
    // question combat is actually about.
    const board = view({
      battlefield: [
        permanent('attacker', 'Onakke Ogre', { attacking: true, attacking_player: 'p2' }),
        permanent('wall', 'Wall of Vines', { blocking: ['attacker'] }),
        permanent('bear', 'Grizzly Bears', { blocking: ['attacker'] }),
      ],
    })

    expect(
      relations(board)
        .to('attacker')
        .map((relation) => relation.from),
    ).toEqual(['wall', 'bear'])
    expect(trail(board, 'attacker')).toEqual([
      {
        kind: 'attacking',
        direction: 'from',
        label: 'attacking',
        ends: [{ id: 'p2', name: 'Bo' }],
      },
      {
        kind: 'blocking',
        direction: 'to',
        label: 'blocked by',
        ends: [
          { id: 'wall', name: 'Wall of Vines' },
          { id: 'bear', name: 'Grizzly Bears' },
        ],
      },
    ])
  })

  it('keeps two blockers as one line, in the order the server listed them', () => {
    // Multiple blockers are one relationship with two ends, not two relationships that happen
    // to share a phrase. The order is the view's — the client has no basis for another.
    const board = view({
      battlefield: [
        permanent('bear', 'Grizzly Bears', { blocking: ['attacker'] }),
        permanent('wall', 'Wall of Vines', { blocking: ['attacker'] }),
        permanent('attacker', 'Onakke Ogre', { attacking: true }),
      ],
    })

    const blocked = trail(board, 'attacker').filter((line) => line.kind === 'blocking')
    expect(blocked).toHaveLength(1)
    expect(blocked[0]?.ends.map((end) => end.name)).toEqual(['Grizzly Bears', 'Wall of Vines'])
  })

  it('gives a blocker on two attackers one line per attacker it blocked', () => {
    // A blocker blocks one attacker unless an effect lets it block additional creatures
    // (CR 509.1a), so `blocking` is a list and each entry is its own edge. The order is the
    // server's — it is the order the blocker assigns its combat damage in.
    const board = view({
      battlefield: [
        permanent('twins', 'Ghastbark Twins', { blocking: ['second', 'first'] }),
        permanent('first', 'Onakke Ogre', { attacking: true }),
        permanent('second', 'Colossal Dreadmaw', { attacking: true }),
      ],
    })

    expect(
      relations(board)
        .from('twins')
        .map((relation) => relation.to),
    ).toEqual(['second', 'first'])
    // And each attacker reads its own side of it, from the one field the blocker carries.
    expect(relationNote(trail(board, 'first'))).toContain('blocked by Ghastbark Twins')
    expect(relationNote(trail(board, 'second'))).toContain('blocked by Ghastbark Twins')
  })

  it('names the planeswalker being attacked, not the seat that answers for it', () => {
    // The server sends both when a planeswalker is attacked: `attacking_player` is the seat
    // the attack is answered by, `attacking_planeswalker` is what the damage is going to.
    // Drawing the seat would point the arrow at the wrong object; drawing both would show one
    // attack as two.
    const all = relations(
      view({
        battlefield: [
          permanent('attacker', 'Onakke Ogre', {
            attacking: true,
            attacking_player: 'p2',
            attacking_planeswalker: 'perm_walker',
          }),
        ],
      }),
    )

    expect(all.from('attacker')).toEqual([
      { kind: 'attacking', from: 'attacker', to: 'perm_walker' },
    ])
    expect(all.to('p2')).toEqual([])
  })

  it('states an attack whose defender the server did not name, without inventing one', () => {
    // `attacking: true` and nothing else is a fact about the game, and the honest rendering is
    // "attacking" with no name. Filling in the only opponent would be the client deciding
    // where an attack is going.
    const board = view({ battlefield: [permanent('attacker', 'Onakke Ogre', { attacking: true })] })

    expect(relations(board).from('attacker')).toEqual([
      { kind: 'attacking', from: 'attacker', to: undefined },
    ])
    expect(trail(board, 'attacker')).toEqual([
      { kind: 'attacking', direction: 'from', label: 'attacking', ends: [] },
    ])
    expect(relations(board).linked('attacker').size).toBe(0)
  })

  it('reads an attachment from the thing attached and from the thing it is on', () => {
    const board = view({
      battlefield: [
        permanent('aura', 'Pacifism', { attached_to: 'bear' }),
        permanent('bear', 'Grizzly Bears'),
      ],
    })

    expect(trail(board, 'aura')).toEqual([
      {
        kind: 'attached',
        direction: 'from',
        label: 'attached to',
        ends: [{ id: 'bear', name: 'Grizzly Bears' }],
      },
    ])
    expect(trail(board, 'bear')).toEqual([
      {
        kind: 'attached',
        direction: 'to',
        label: 'attached',
        ends: [{ id: 'aura', name: 'Pacifism' }],
      },
    ])
  })
})

describe('the stack, as relationships', () => {
  const stack: StackItem[] = [
    { id: 's1', controller: 'p1', description: 'Add {G}.', source: 'perm_bear', kind: 'ability' },
    {
      id: 's2',
      controller: 'p1',
      description: 'Twin Bolt',
      kind: 'spell',
      targets: [
        { kind: 'permanent', id: 'perm_bear' },
        { kind: 'player', player: 'p2' },
      ],
    },
    {
      id: 's3',
      controller: 'p2',
      description: 'Counterspell',
      kind: 'spell',
      targets: [{ kind: 'stack', id: 's2' }],
    },
  ]

  it('traces a target of every kind by the same entity id', () => {
    // Permanent, player, card, and stack object are four tags on the wire and one kind of
    // thing on screen: an id a click already reaches. A seat is a target like anything else.
    const board = view({ stack, battlefield: [permanent('perm_bear', 'Grizzly Bears')] })

    expect(trail(board, 's2')).toEqual([
      {
        kind: 'targeting',
        direction: 'from',
        label: 'targeting',
        ends: [
          { id: 'perm_bear', name: 'Grizzly Bears' },
          { id: 'p2', name: 'Bo' },
        ],
      },
      {
        kind: 'targeting',
        direction: 'to',
        label: 'targeted by',
        ends: [{ id: 's3', name: 'Counterspell' }],
      },
    ])
    expect(trail(board, 'p2')).toEqual([
      {
        kind: 'targeting',
        direction: 'to',
        label: 'targeted by',
        ends: [{ id: 's2', name: 'Twin Bolt' }],
      },
    ])
  })

  it('links an ability on the stack back to the permanent it came from', () => {
    const board = view({ stack, battlefield: [permanent('perm_bear', 'Grizzly Bears')] })

    expect(trail(board, 's1')).toEqual([
      {
        kind: 'source',
        direction: 'from',
        label: 'from',
        ends: [{ id: 'perm_bear', name: 'Grizzly Bears' }],
      },
    ])
    expect(trail(board, 'perm_bear')).toEqual([
      {
        kind: 'targeting',
        direction: 'to',
        label: 'targeted by',
        ends: [{ id: 's2', name: 'Twin Bolt' }],
      },
      {
        kind: 'source',
        direction: 'to',
        label: 'on the stack',
        ends: [{ id: 's1', name: 'Add {G}.' }],
      },
    ])
  })

  it('gathers every end of every edge touching one object', () => {
    // What a focused view emphasises: one click on a spell should light up what it names and
    // what named it, in both directions and across zones.
    const all = relations(view({ stack }))

    expect([...all.linked('s2')].sort()).toEqual(['p2', 'perm_bear', 's3'])
  })
})

describe('a view with nothing to relate', () => {
  it('answers every question with nothing rather than failing', () => {
    const empty = view({})
    const all = relations(empty)

    expect(all.all).toEqual([])
    expect(all.from('anything')).toEqual([])
    expect(all.to('anything')).toEqual([])
    expect(all.linked('anything').size).toBe(0)
    expect(trail(empty, 'anything')).toEqual([])
  })
})

/**
 * The join from an id to a word, which is the whole of #674.
 *
 * A relationship arrives as an identifier and is read by a person. `perm_vivien` is for a log
 * (`docs/client-design.md` §9.2) and, worse, it looks like a card name to anyone who has not seen
 * the wire — so what is asserted here is that a name always comes from the view and never from
 * the id, in every place the view is allowed to state one.
 */
describe('naming the other end of a relationship', () => {
  const attacking = (to: string): Partial<GameView> => ({
    battlefield: [
      permanent('perm_ogre', 'Onakke Ogre', { attacking: true, attacking_planeswalker: to }),
    ],
  })

  const attacked = (from: GameView) =>
    trail(from, 'perm_ogre').find((line) => line.kind === 'attacking')

  it('names an end the board is drawing', () => {
    const board = view({
      battlefield: [
        permanent('perm_ogre', 'Onakke Ogre', {
          attacking: true,
          attacking_planeswalker: 'perm_vivien',
        }),
        permanent('perm_vivien', 'Vivien Reid'),
      ],
    })

    expect(attacked(board)?.ends).toEqual([{ id: 'perm_vivien', name: 'Vivien Reid' }])
  })

  it('names an end that is in a pile nobody has opened', () => {
    // The case the trail exists for: the other end has no box on the screen, so the drawn line
    // clips to nothing (`overlay.ts`) and the sentence is the only copy of the fact left. It
    // has to be a name.
    const board = view({
      stack: [
        {
          id: 's3',
          controller: 'p1',
          description: 'Return target creature card from your graveyard to your hand.',
          targets: [{ kind: 'card', id: 'g2' }],
        },
      ],
      graveyards: [{ player_id: 'p1', cards: [card('g2', 'Llanowar Elves')] }],
    })

    expect(trail(board, 's3')).toEqual([
      {
        kind: 'targeting',
        direction: 'from',
        label: 'targeting',
        ends: [{ id: 'g2', name: 'Llanowar Elves' }],
      },
    ])
  })

  it('says the phrase and nothing else for an end the view never described', () => {
    // #674 exactly: an opponent whose permanents are not in this view at all. There is no name
    // to state, and the only word available would be one worked out from the *relationship* —
    // "a creature", because something is being attacked — which is the client concluding a fact
    // about the game. So the line is the phrase, which is all the view actually said.
    expect(attacked(view(attacking('perm_vivien')))).toEqual({
      kind: 'attacking',
      direction: 'from',
      label: 'attacking',
      ends: [],
    })
  })

  it('treats a blank name on the wire as no name at all', () => {
    // A name that is empty, or whitespace, is stated absence rather than a label. Letting it
    // through would hang an empty control under the card, which is worse than the phrase alone.
    const blank = view({
      ...attacking('perm_vivien'),
      graveyards: [
        { player_id: 'p2', cards: [{ id: 'perm_vivien', name: '   ', type_line: 'Planeswalker' }] },
      ],
    })

    expect(attacked(blank)?.ends).toEqual([])
  })

  it('takes a name from every place the view is allowed to state one', () => {
    // Totality is the point: a join that covered the battlefield and not the graveyard would
    // move the defect rather than fix it. Every entry here is a name the *server* stated.
    const board = view({
      you: 'p1',
      my_hand: [card('h1', 'Shock')],
      revealed: [card('r1', 'Divine Verdict')],
      battlefield: [permanent('perm_axe', "Marauder's Axe")],
      emblems: [{ id: 'emblem_1', controller: 'p1' }],
      stack: [
        {
          id: 's7',
          controller: 'p1',
          description: 'Equipped creature deals 1 damage to each of two targets.',
          source: 'perm_axe',
          card: card('c9', 'Lightning Strike'),
        },
      ],
      graveyards: [{ player_id: 'p1', cards: [card('g2', 'Llanowar Elves')] }],
      exile: [{ player_id: 'p2', cards: [card('x1', 'Cancel')] }],
      command: [{ player_id: 'p1', cards: [card('cmd1', 'Kalamax, the Stormsire')] }],
      log: [
        {
          sequence: 44,
          event: { type: 'permanent_died', permanent: { id: 'd1', name: 'Thopter' } },
        },
      ],
    })

    expect(Object.fromEntries(entityNames(board))).toEqual({
      d1: 'Thopter',
      g2: 'Llanowar Elves',
      x1: 'Cancel',
      cmd1: 'Kalamax, the Stormsire',
      h1: 'Shock',
      r1: 'Divine Verdict',
      c9: 'Lightning Strike',
      perm_axe: "Marauder's Axe",
      s7: 'Equipped creature deals 1 damage to each of two targets.',
      emblem_1: 'Emblem',
      p1: 'Ada',
      p2: 'Bo',
    })
  })

  it('prefers what the view shows now over what its log remembers', () => {
    // The log is the lowest-precedence source, because it is a transcript: a name it stated is
    // a name that *was* true, and anything the view is currently projecting outranks it.
    const board = view({
      battlefield: [permanent('perm_zombie', 'Zombie')],
      log: [
        {
          sequence: 9,
          event: {
            type: 'attackers_declared',
            player: 'p1',
            attackers: [{ id: 'perm_zombie', name: 'Something Else' }],
          },
        },
      ],
    })

    expect(entityNames(board).get('perm_zombie')).toBe('Zombie')
  })

  it('takes a seat’s name from the same place the seat’s own panel takes it', () => {
    // A seat is an end like any other, and it must not be called two things by two surfaces.
    const board = view({
      you: 'p1',
      seat_order: ['p1', 'p2', 'p3'],
      opponents: [{ player_id: 'p3', hand_size: 4, life: 20, library_size: 28, graveyard_size: 0 }],
    })

    // Named by the server, and named by nobody: the second is what `playerLabel` calls a seat
    // nobody named, which is the string that seat's panel, its field and its life total carry —
    // and, since #692, is a phrase rather than the wire's key for it.
    expect(entityNames(board).get('p1')).toBe('Ada')
    expect(entityNames(board).get('p2')).toBe('Bo')
    expect(entityNames(board).get('p3')).toBe('Opponent 3')
  })
})

/**
 * The guard that keeps #674 and #692 from coming back in a shape nobody anticipated.
 *
 * Structural rather than a list of the strings that happened to leak: the forbidden set is read
 * *out of each fixture* — every id it addresses an object or a seat by — and everything a player
 * reads about a relationship or a seat is swept against it. Anything the client fails to name and
 * prints anyway is caught whatever it is called.
 *
 * Two surfaces, because there were two defects and one rule (`docs/client-design.md` §2.1, rule
 * 3): the **trail** under every object with a relationship, and the **name every seat is drawn
 * under** — the string its panel, its battlefield's region label and its life total all share.
 *
 * The reductions are what make the trail half bite. A fixture as sent describes every object its
 * own relationships name, so it can never leak; the defect appears exactly when a relationship
 * outlives the description of its other end, which is a board where the opponent controls no
 * permanents, a pile the server stopped itemizing, or a seat this client can see nothing of. The
 * relationships stay the fixture's own throughout — only what is available to name them shrinks.
 * The seat half needs no reduction: most committed fixtures carry no `player_names` at all, which
 * is already the case where the client has to find a seat a word of its own.
 */
describe('no identifier reaches a player, on any committed fixture', () => {
  const FIXTURES = join(
    dirname(fileURLToPath(import.meta.url)),
    '../../../crates/sage-protocol/fixtures',
  )
  const readFixture = (name: string): unknown =>
    JSON.parse(readFileSync(join(FIXTURES, name), 'utf8'))
  const GAMEVIEWS = readdirSync(FIXTURES).filter(
    (name) => name.startsWith('gameview') && name.endsWith('.json'),
  )

  /**
   * Fields that address an object rather than describing one.
   *
   * `id` wherever it appears, plus the four relationship fields, which is how an id that is only
   * ever *pointed at* — a permanent the view no longer carries — still lands in the forbidden set.
   */
  const ADDRESSES = new Set([
    'id',
    'attached_to',
    'blocking',
    'attacking_planeswalker',
    'source',
    'physical_card',
  ])

  /**
   * Fields that address a **seat**.
   *
   * These were deliberately left out when this guard was written for #674, on the argument that
   * `p2` was the string the client labelled that seat with everywhere and so functioned as a
   * name. #692 is the answer to that: it is the *wire's* key, a player has never seen the
   * protocol, and consistency is a reason it was tolerable rather than a reason it was right. A
   * seat id is now as forbidden as any other, which is what makes this one guard instead of two.
   */
  const SEATS = new Set([
    'you',
    'player_id',
    'player',
    'controller',
    'owner',
    'attacking_player',
    'active_player',
    'priority_player',
    'winner',
  ])

  /** Lists whose entries are seats, and the map whose *keys* are. */
  const SEAT_LISTS = new Set(['seat_order', 'losers'])

  const forbiddenIds = (value: unknown, into = new Set<string>()): Set<string> => {
    if (Array.isArray(value)) {
      for (const item of value) forbiddenIds(item, into)
      return into
    }
    if (value !== null && typeof value === 'object') {
      for (const [key, entry] of Object.entries(value)) {
        if (typeof entry === 'string' && (ADDRESSES.has(key) || SEATS.has(key))) into.add(entry)
        else if (SEAT_LISTS.has(key) && Array.isArray(entry)) {
          for (const seat of entry) if (typeof seat === 'string') into.add(seat)
        } else if (key === 'player_names' && entry !== null && typeof entry === 'object') {
          // The one place a seat id is a *key* rather than a value — and the values beside them
          // are the display names, which are the one thing here that is allowed on screen.
          for (const seat of Object.keys(entry)) into.add(seat)
        } else forbiddenIds(entry, into)
      }
    }
    return into
  }

  /** What is left to name things with, from everything down to nothing at all. */
  const REDUCTIONS: readonly (readonly [string, (from: GameView) => GameView])[] = [
    ['as it was sent', (from) => from],
    [
      'with no opponent permanents on the board',
      (from) => ({
        ...from,
        battlefield: list(from.battlefield).filter(
          (each) => from.you !== undefined && each.controller === from.you,
        ),
      }),
    ],
    [
      'with every pile no longer itemized',
      (from) => ({ ...from, graveyards: undefined, exile: undefined, command: undefined }),
    ],
    [
      'with nothing left that describes an object',
      (from) => ({
        phase: from.phase,
        you: from.you,
        seat_order: from.seat_order,
        opponents: from.opponents,
        player_names: from.player_names,
      }),
    ],
  ]

  /** Every object a relationship touches, which is every object with a trail to read. */
  const subjectsOf = (from: GameView): readonly string[] => [
    ...new Set(
      relations(from).all.flatMap((relation) =>
        relation.to === undefined ? [relation.from] : [relation.from, relation.to],
      ),
    ),
  ]

  const drawn = (from: GameView, naming: GameView): readonly string[] => {
    const all = relations(from)
    const names = entityNames(naming)
    return subjectsOf(from).flatMap((subject) =>
      relationLines(all, subject, names).flatMap((line) =>
        // Both copies of the fact: the words on the control, and the accessible name a screen
        // reader is given for it (`ui/game/RelationTrail.tsx`). A leak into either one is a leak.
        line.ends.flatMap((end) => [end.name, `${line.label} ${end.name}`]),
      ),
    )
  }

  /**
   * The other surface: what every seat at this table is *called*.
   *
   * One string per seat, because it is one string per seat on screen — `table.ts` joins it once
   * and the panel's heading, the battlefield's region label and the life total's owner are the
   * same value read three times. Sweeping it here covers all three.
   */
  const seatNames = (from: GameView): readonly string[] => seats(from).map((seat) => seat.name)

  it.each(
    GAMEVIEWS.flatMap((name) => REDUCTIONS.map(([how, reduce]) => [name, how, reduce] as const)),
  )('%s, %s', (name, _how, reduce) => {
    const from = GameView.parse(readFixture(name))
    const forbidden = forbiddenIds(readFixture(name))
    const reduced = reduce(from)

    for (const text of [...drawn(from, reduced), ...seatNames(reduced)]) {
      const leaked = [...forbidden].filter((id) => text === id || text.split(' ').includes(id))
      expect(leaked).toEqual([])
    }
  })

  it('sweeps a trail that has something in it, and a table with somebody at it', () => {
    // Without this the whole suite above could pass by finding nothing to check — the one way a
    // guard like this goes quietly blind. Both surfaces, because both are swept.
    const board = GameView.parse(readFixture('gameview-board.json'))
    expect(drawn(board, board).length).toBeGreaterThan(20)
    expect(seatNames(board)).toEqual(['Ada', 'Bo'])
  })

  it('forbids the seat ids as well as the object ids', () => {
    // The half added by #692, and the half that would go blind first: the sweep is only as wide
    // as the set it sweeps against, and a `SEATS` entry quietly dropped would take every seat
    // surface out of this guard while every assertion above still passed.
    const forbidden = forbiddenIds(readFixture('gameview-board.json'))
    expect([...forbidden]).toEqual(expect.arrayContaining(['p1', 'p2', 'perm_vivien']))

    // And a table nobody named still contributes its seats, which is where the defect lived.
    expect([...forbiddenIds(readFixture('gameview-actions.json'))]).toEqual(
      expect.arrayContaining(['p0', 'p1']),
    )
  })

  it('has a reduction that actually takes the names away', () => {
    // And the other half of the same worry: a reduction that changed nothing would assert
    // nothing. The dense board names every end it states; stripped of everything that describes
    // an object, only its seats can still be named.
    const board = GameView.parse(readFixture('gameview-board.json'))
    const stripped = REDUCTIONS.at(-1)![1](board)

    expect(drawn(board, stripped)).toEqual(expect.arrayContaining(['Bo', 'attacking Bo']))
    expect(drawn(board, stripped).length).toBeLessThan(drawn(board, board).length)
  })
})

describe('the readable copy of a drawn line', () => {
  it('says every relationship the overlay would draw, in the same words', () => {
    const board = view({
      battlefield: [
        permanent('perm_ogre', 'Onakke Ogre', { attacking: true, attacking_player: 'p2' }),
        permanent('perm_bear', 'Grizzly Bears', { blocking: ['perm_ogre'] }),
      ],
    })
    for (const id of ['perm_ogre', 'perm_bear']) {
      const lines = trail(board, id)
      const note = relationNote(lines)
      // Every phrase, and every end it could name. An arrow is `aria-hidden`; this is what a
      // screen reader is given instead, so nothing may be missing from it.
      for (const line of lines) {
        expect(note).toContain(line.label)
        for (const end of line.ends) expect(note).toContain(end.name)
      }
    }
    expect(relationNote(trail(board, 'perm_bear'))).toContain('blocking Onakke Ogre')
  })

  it('is empty for an object the view relates to nothing', () => {
    expect(relationNote([])).toBe('')
  })

  it('keeps a phrase whose other end the view never named', () => {
    expect(
      relationNote([{ kind: 'attacking', direction: 'from', label: 'attacking', ends: [] }]),
    ).toBe('attacking')
  })
})
