/**
 * The scale gate: the whole supported range, asserted instead of eyeballed.
 *
 * This is the check that replaces "the maintainer changes the zoom and the resolution and finds
 * what broke". `docs/client-design.md` states a contract — nothing clipped, no region of the
 * board scrolling, every tier-1 fact on screen, no type below its floor — and states it for
 * *every* supported screen rather than for the desktop it was designed on. A contract that is
 * only ever checked at one viewport is a contract about that viewport.
 *
 * The whole supported range is about 25× in area, from 640×360 to 3440×1440, and browser zoom is
 * the same problem wearing a different hat: a browser at 200% does not scale the page, it halves
 * the layout viewport, so 1280×720 at 200% *is* 640×360. That is why `page.setViewportSize` is
 * the entire mechanism here and no zoom API is needed — the viewport list is the scene model's,
 * and every band in `docs/client-design.md` §4 is in it.
 *
 * **What is asserted is the contract, never a position.** Nothing here pins a coordinate, a
 * width, or which side of the screen a rail is on. Those are design decisions and the design is
 * expected to move; "the text in this box is not cut off" is not. For the same reason everything
 * tier 1 is addressed by role and accessible name rather than by class, so the restyling that is
 * coming can rename every selector in the stylesheet without touching this file.
 *
 * **A `test.fail()` in this file is a known defect with an owner, not a disabled test.** The
 * contract is asserted in full at every band; where the client does not meet it yet the case
 * carries an annotation naming the issue that retires it and the number the browser measured.
 * Playwright still runs the assertion for real and reports it as an *expected* failure — so the
 * suite is green today and the gate still says exactly what is broken, where, and by how much.
 *
 * The reason it is `test.fail()` and never `test.skip()`: **Playwright fails the run when an
 * expected failure unexpectedly passes.** The moment #659 or #660 makes one of these assertions
 * true, this suite goes red and stays red until the annotation is deleted. That is what keeps
 * the ledger honest — it cannot rot, it cannot silently under-report, and it cannot outlive the
 * defect it describes. Deleting the row in `KNOWN` below is part of the definition of done for
 * the issue that row names, and the assertion under it is that issue's acceptance test.
 *
 * A check that is *expected* to be red is a check nobody reads, and by the time it went green
 * nobody would notice. Every failure this file found is still asserted, at full strength, in a
 * form that a green pipeline can carry. See #652 and the issues under it.
 *
 * Three things are measured at every band, and the third was added with #678. The board's text
 * against the box it was given; the board's regions against their own overflow; and **the dock's
 * controls against the dock's band** — a question drawn bigger than the band it was allocated
 * does not clip its own text and does not make anything scroll, so neither of the first two can
 * see it. The buttons are simply below the box, and `overflow: hidden` takes them away.
 *
 * The **non-blocking** tier (ADR 0011) — breadth lives here so breadth never gates a merge on
 * browser flake, and `smoke.spec.ts` remains the one blocking path.
 */
import { expect, test, type Locator, type Page } from '@playwright/test'

import { fixture, pageFits, serveFrames } from './frames'

/**
 * The screens the client claims to work on, and one it does not.
 *
 * Each is annotated with the band `src/scene.ts` puts it in, because the band is what decides
 * which arrangement is supposed to answer for it — and because a failure that is confined to one
 * band says something different from one that happens everywhere.
 */
const VIEWPORTS = [
  { name: '1920×1080 — Wide, the size it is designed at', width: 1920, height: 1080 },
  { name: '1440×900 — Wide', width: 1440, height: 900 },
  { name: '1280×720 — Wide, the floor of Optimized', width: 1280, height: 720 },
  { name: '640×360 — Short, 1280×720 at 200% zoom', width: 640, height: 360 },
  { name: '390×844 — Tall, a phone in portrait', width: 390, height: 844 },
  { name: '844×390 — Short, the same phone turned', width: 844, height: 390 },
  { name: '3440×1440 — Ultrawide', width: 3440, height: 1440 },
  { name: '320×480 — Tall, the smallest supported screen', width: 320, height: 480 },
] as const

/** Below §1's floor on both edges: no arrangement to attempt, so the client must say so. */
const UNSUPPORTED = { width: 300, height: 400 }

/**
 * The ledger: what the client is known to get wrong today, at which size, and what fixes it.
 *
 * One entry per **(viewport, assertion)** pair and no coarser. A row is what turns a failing
 * assertion into an expected failure, so a row covering two assertions would let a regression in
 * one hide behind the other — fixing the battlefield at 1440×900 has to flip exactly one thing.
 * Every row names the issue that retires it and the number the browser measured against
 * `gameview-board.json` at that size, because a ledger entry a reader has to go re-measure
 * before they dare delete it is one nobody deletes.
 *
 * Rows are deleted, never edited down. When the defect is fixed the assertion passes, Playwright
 * reports an unexpected pass, and the run is red until the row goes. When the last row goes, so
 * does this table and the `known()` call that reads it.
 */
interface Known {
  /** §2: text drawn into a box smaller than the text. */
  clipped?: string
  /** §3: a region of the board that scrolls, or that is reached by scrolling. */
  scrolls?: string
}

/**
 * Keyed by `width×height` rather than by the prose name, so renaming a band does not silently
 * orphan a row into a permanently-red unexpected pass.
 *
 * The `clipped` rows are relationship trails, and that repetition is the finding rather than noise:
 * a trail's control needs up to 328px in 192px on a 3440px ultrawide exactly as it does on a phone,
 * because that box is not derived from the viewport.
 *
 * The exception is the one thing in the table that *is* derived from the viewport and is there on
 * purpose: **#686 keeps creatures, other permanents and lands in their own rows at every desktop
 * size, and the cards pay for it.** At 1440×900 and below, a field the scene budgets for two 100px
 * rows is divided three ways, and the tile that comes out — 54×76, then 47×66 — has a name band a
 * few pixels short of the longest single words the fixture has. Those rows carry the measurement
 * rather than hiding it, the field marks itself with `data-below-floor`, and the room they want is
 * `scene.ts`'s `SPLIT_ROWS`. It is the deliberate trade of §3 — "cannot fit three rows? Make the
 * cards smaller" — reported at the point where a person has to judge whether it went too far.
 *
 * First measured against `main` at 9084a1c, which is #667 — the redrawn card. That merge retired
 * more than half of what this gate first reported: clipped elements went from 16–34 per viewport
 * to 8–9, and the nine `span.badge` misses of the 9px card floor are gone entirely.
 *
 * #659 — the scene, placing every region at a computed box — retired the rest of the type floor
 * (all eight `floor` rows, plus the two `mana__pip` misses of the 9px card floor that those rows
 * had under-counted) and both rows for the unsupported viewport. It did **not** retire a single
 * `scrolls` row, and the reason is worth stating: each of those rows named exactly two regions,
 * the battlefields and the stack, and both of them overflowed because what was *inside* them had
 * no packing yet.
 *
 * #660 — the packed battlefield — took the battlefields out of every one of those rows. **A field
 * now holds its content at every viewport in this file**, from one permanent to forty: the tiles
 * are sized from the room and the count, they take a second line before they fan, and below a
 * 100px row they are chips. What is left in each `scrolls` row is the stack alone, so the rows are
 * narrowed to it rather than deleted, and each one retires with #662.
 *
 * The follow-up to #676 — `fit.ts`'s advance table made the upper bound it always claimed to be —
 * retired every `clipped` element a permanent drew that was an *estimation* error.
 * `Creature — Ogre Warrior` needing 130px in a 124px band at three sizes, and `Colossal Dreadmaw`
 * and `Gravedigger` missing their 72×100 tile's name band on a phone, were all the estimator coming
 * up 5–9% short of what the browser draws; the table is measured now and a unit test pins the
 * direction of the error. What #676 also did was merge the rows, and #686 reverses that: the names
 * listed at 1440×900 and below are back, at smaller tiles, and this time they are a stated cost
 * rather than a wrong number. 1920×1080 draws its three rows at 73×102 and cuts nothing.
 */
const KNOWN: Record<string, Known> = {
  '1920×1080': {
    clipped:
      '#663 — 6 relationship trails, and nothing else: `Return target creature card…` needs 328px ' +
      'in 192px, `Equipped creature deals 1 damage…` 304px in 192px (×3), `+1: Look at the top ' +
      'four cards…` 234px in 192px, `Lightning Strike deals 3 damage…` 242px in 192px. Nothing a ' +
      'permanent draws is cut here any more.',
    scrolls:
      '#662 — the stack alone: `section.rail__zone` holds 1307px in a 912px box. Both ' +
      'battlefields hold their own content now (#660), which is what took them out of this row.',
  },
  '1440×900': {
    clipped:
      '#663 — the same 6 trails the 1920×1080 row lists, at the same numbers — **and two card ' +
      'names**, which are #686 and not #663: keeping creatures, other permanents and lands in ' +
      'their own rows costs this field a 54×76 tile, and a 48px band draws `Gravedigge` and ' +
      "`Marauder's` 50px wide at the 9px floor. Two pixels each, on the two longest single words " +
      'the fixture has. The row says so itself — `data-below-floor="54×76"` — and whether it has ' +
      "gone too far is the maintainer's call, not this file's. The room it wants is the " +
      "scene's: `SPLIT_ROWS` budgets a field for two rows of 100px, and a board with three groups " +
      'divides that between three.',
    scrolls: '#662 — the stack alone: `section.rail__zone` holds 1307px in a 750px box.',
  },
  '1280×720': {
    clipped:
      '#663 — the same 6 trails again, and **four card names** for #686, for the reason the ' +
      '1440×900 row gives: three rows in a field budgeted for two draw a 47×66 tile, and its ' +
      "41px band cuts `Gearsmith` (46px), `Gravedigge` (50px), `Marauder's` (50px) and " +
      '`Mountain` (43px) at the 9px floor. Between 2px and 9px each, and every one of them a ' +
      'single word, which no second line can help. This is the split being paid for in card ' +
      'size (§3) at the size where the payment starts to show.',
    scrolls: '#662 — the stack alone: `section.rail__zone` holds 1307px in a 589px box.',
  },
  '640×360': {
    clipped:
      '#663 — 8 relationship trails and nothing else: four under permanents (up to 328px in ' +
      '192px) and four in the seat bars, where the band is narrow enough that `Serra Angel` ' +
      'needs 61px in 48px. Every permanent here is a chip, and no chip cuts its name.',
    scrolls: '#662 — the stack alone: the collapsed `section.badge-rail` holds 333px in 130px.',
  },
  '390×844': {
    clipped:
      '#663 — 8 trails, four of them squeezed into the seat bars of a 390px screen, where ' +
      '`Serra Angel` gets 11px and `Thopter` 8px. Plus the same four card names the 1280×720 ' +
      'row lists, at the same 47×66 tile: a portrait phone keeps its three rows now, and this ' +
      'is what they cost it.',
    scrolls: '#662 — the stack alone: the collapsed `section.badge-rail` holds 333px in 130px.',
  },
  '844×390': {
    clipped: '#663 — 6 relationship trails, four under permanents and two in the seat bars.',
    scrolls: '#662 — the stack alone: the collapsed `section.badge-rail` holds 333px in 130px.',
  },
  '3440×1440': {
    clipped:
      '#663 — 6 relationship trails on 4.9 megapixels of screen, which is the point: a trail ' +
      'needs up to 328px in 192px here exactly as it does on a phone, because that box is not ' +
      'derived from the room available. Nothing a permanent draws is cut at this size, and the ' +
      'permanents here are the designed 130×182 in two rows.',
    scrolls:
      '#662 — the stack alone: `section.rail__zone` holds 1307px in a 1235px box, on the largest ' +
      'screen the client supports.',
  },
  '320×480': {
    clipped:
      '#663 — 6 relationship trails: four under permanents and two in seat bars 40px and 50px ' +
      'wide. The board itself fits — chips, two lines of them, on the smallest supported screen.',
    scrolls: '#662 — the stack alone: the collapsed `section.badge-rail` holds 333px in 130px.',
  },

  // The one case that is not one of the eight: a pile open changes the numbers.
  '1440×900 with a pile open': {
    scrolls:
      '#662 — the same stack rail the 1440×900 row describes. Nothing *inside* the pile is ' +
      'reported, which is the exemption working; this row is the board beside it, and the board ' +
      'beside it now holds its permanents.',
    clipped: '#663 — the same 6 the 1440×900 row describes, unchanged by the pile.',
  },
}

/**
 * Mark the running test as a known defect, and say which one in the report.
 *
 * `test.fail()` rather than `test.skip()`, always: the assertion still runs, and Playwright
 * turns the whole run red the moment it starts passing. The annotation puts the row's text in
 * the reporter output next to the test name, so the ledger reads from the CI log without anyone
 * opening this file.
 *
 * Given `undefined` it does nothing at all, which is what deleting a row does.
 */
function known(entry: string | undefined): void {
  if (entry === undefined) return
  test.info().annotations.push({ type: 'known defect', description: entry })
  test.fail()
}

/** The ledger rows for one viewport, or an empty set once they have all been retired. */
const knownAt = (viewport: { width: number; height: number }): Known =>
  KNOWN[`${viewport.width}×${viewport.height}`] ?? {}

/** The dense board — twelve permanents, seven objects on the stack — at one viewport, painted. */
async function table(page: Page, viewport: { width: number; height: number }) {
  await page.setViewportSize({ width: viewport.width, height: viewport.height })
  await serveFrames(page, [fixture('gameview-board.json')])
  await page.goto('/')
  await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()
}

/**
 * The board, region by region, addressed the way a player's screen reader addresses it.
 *
 * Roles and accessible names, not classes: the surface work under #652 rewrites the stylesheet
 * and moves markup between components, and a gate written against `.field--you` would be deleted
 * by the first PR it was supposed to hold. A region the ladder has legitimately removed reports
 * a count of zero and is skipped — the sweeps below say what must not be true of a region that
 * exists, not which regions must.
 *
 * The `floor` is §2's type floor for the text this region carries: 9px effective on a card,
 * 11px on chrome. Where a region carries both — the stack rail carries card faces and the words
 * around them — it is given the card's floor, which is the lenient reading; a gate that fails on
 * something that is not a defect is a gate that gets turned off.
 *
 * The match line is the one region addressed by attribute rather than by role. A `<header>`
 * inside `<main>` is not a `banner` — it has no role a name can reach — so its accessible name
 * is used directly, which is still the name and not a class.
 */
const BOARD = (page: Page): { region: string; floor: number; locator: Locator }[] => [
  { region: 'the match line', floor: 11, locator: page.locator('[aria-label="Match"]') },
  { region: 'the turn', floor: 11, locator: page.getByRole('list', { name: 'Turn steps' }) },
  { region: 'the seats', floor: 11, locator: page.getByRole('region', { name: /seat$/ }) },
  {
    region: 'the battlefields',
    floor: 9,
    locator: page.getByRole('region', { name: /battlefield$/ }),
  },
  { region: 'the stack', floor: 9, locator: page.getByRole('region', { name: 'Stack' }) },
  { region: 'your hand', floor: 9, locator: page.getByRole('region', { name: 'Your hand' }) },
  {
    region: 'the action affordance',
    floor: 11,
    locator: page.getByRole('region', { name: 'Actions' }),
  },
]

/**
 * What a piece of text on the table measured, and what it was given room for.
 *
 * One measurement feeds two assertions — clipping and the type floor — because they are two
 * questions about the same element and taking them separately would walk the DOM twice to learn
 * the same thing.
 */
interface Measured {
  region: string
  /** The type floor this region's text is held to, in px. */
  floor: number
  element: string
  text: string
  fontSize: number
  scrollWidth: number
  clientWidth: number
  scrollHeight: number
  clientHeight: number
}

/**
 * Every piece of text drawn inside a region, measured against the box it was given.
 *
 * *Every* element that directly owns text, rather than a list of the classes text is known to
 * be drawn in: the failure this is here to catch is a name rendered as `C…`, and a list of
 * classes only catches it in the places somebody remembered to list. Elements a pixel or less
 * on either edge are skipped, which is how the visually-hidden text a screen reader is given —
 * a life total's unit, a stat's label — stays out of a measurement about pixels.
 *
 * Runs in the page and closes over nothing: Playwright serializes the function, so the region's
 * name and floor arrive as an argument rather than from this scope.
 */
async function textIn(page: Page): Promise<Measured[]> {
  const measured: Measured[] = []
  for (const { region, floor, locator } of BOARD(page)) {
    measured.push(
      ...(await locator.evaluateAll(
        (roots, about: { region: string; floor: number }) => {
          const found: Measured[] = []
          for (const root of roots) {
            for (const node of [root, ...root.querySelectorAll('*')]) {
              const el = node as HTMLElement
              // Only what owns text of its own. A wrapper's overflow is its children's problem,
              // and reporting both would name the same defect twice.
              const owns = [...el.childNodes].some(
                (child) => child.nodeType === 3 && (child.textContent ?? '').trim() !== '',
              )
              if (!owns) continue
              const style = getComputedStyle(el)
              if (style.visibility === 'hidden' || style.display === 'none') continue
              const rect = el.getBoundingClientRect()
              if (rect.width <= 1 || rect.height <= 1) continue
              const classes =
                typeof el.className === 'string' && el.className.trim() !== ''
                  ? `.${el.className.trim().split(/\s+/).join('.')}`
                  : ''
              found.push({
                region: about.region,
                floor: about.floor,
                element: `${el.tagName.toLowerCase()}${classes}`,
                text: (el.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 48),
                fontSize: Math.round(parseFloat(style.fontSize) * 100) / 100,
                scrollWidth: el.scrollWidth,
                clientWidth: el.clientWidth,
                scrollHeight: el.scrollHeight,
                clientHeight: el.clientHeight,
              })
            }
          }
          return found
        },
        { region, floor },
      )),
    )
  }
  return measured
}

/** A box that is reached by scrolling, and the numbers that say by how much. */
interface Scrolling {
  region: string
  element: string
  scrollWidth: number
  clientWidth: number
  scrollHeight: number
  clientHeight: number
}

/**
 * Every board region that scrolls, or that is *reached* by scrolling.
 *
 * Three things are measured for each region, and the third is the one worth explaining. The
 * region itself, because that is where a battlefield's own scrollbar lives. Its scrollable
 * descendants, because a row that scrolls inside a region that does not is the same defect one
 * level down. And its scrollable **ancestors**, because a stack rail that is perfectly sized
 * inside a column the player has to scroll to reach is not a region that fits — and the column
 * carries no accessible name of its own, so the only way to reach it is upward from something
 * that does.
 *
 * The walk upward stops at `<body>`: whether the *page* scrolls is a separate assertion with a
 * separate answer, and `pageFits` already asks it.
 *
 * **What is exempt, explicitly and by name: a pile opened on demand.** §3 grants it in as many
 * words — "piles opened on demand are not the board and may scroll" — and it is exempt here
 * because the board is an enumerated list of regions that does not include it, rather than
 * because a selector was loose enough to let it through. Nothing else is exempt. The side column
 * is likewise not swept: preview, log, and settle are tier 3, and the log has always scrolled.
 */
async function boardScrolling(page: Page): Promise<Scrolling[]> {
  const found: Scrolling[] = []
  for (const { region, locator } of BOARD(page)) {
    found.push(
      ...(await locator.evaluateAll((roots, about: string) => {
        const out: Scrolling[] = []
        const describe = (el: HTMLElement) => {
          const classes =
            typeof el.className === 'string' && el.className.trim() !== ''
              ? `.${el.className.trim().split(/\s+/).join('.')}`
              : ''
          return `${el.tagName.toLowerCase()}${classes}`
        }
        const overflows = (el: HTMLElement) =>
          el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1
        const scrolls = (el: HTMLElement) => {
          const style = getComputedStyle(el)
          return /auto|scroll/.test(`${style.overflowX} ${style.overflowY}`)
        }
        const report = (el: HTMLElement, how: string) =>
          out.push({
            region: `${about} — ${how}`,
            element: describe(el),
            scrollWidth: el.scrollWidth,
            clientWidth: el.clientWidth,
            scrollHeight: el.scrollHeight,
            clientHeight: el.clientHeight,
          })

        for (const node of roots) {
          const root = node as HTMLElement
          if (overflows(root)) report(root, 'the region does not hold its own content')
          for (const child of root.querySelectorAll('*')) {
            const el = child as HTMLElement
            if (scrolls(el) && overflows(el)) report(el, 'something inside it scrolls')
          }
          for (let el = root.parentElement; el && el !== document.body; el = el.parentElement) {
            if (scrolls(el) && overflows(el)) report(el, 'it is reached by scrolling')
          }
        }
        return out
      }, region)),
    )
  }
  return found
}

/**
 * The dock's own box, which is the band `scene()` allocated and nothing to do with its contents.
 *
 * Read off the region rather than off a class: what is asserted about it is that it does not
 * change with the count, and a selector would tie that to the current markup.
 */
const dockBox = (page: Page) =>
  page
    .getByRole('region', { name: 'Actions' })
    .evaluate((root) => root.getBoundingClientRect().toJSON() as DOMRect)

/**
 * Everything the dock draws that is not inside the dock.
 *
 * A different question from `textIn` and from `boardScrolling`, and neither of them can see it:
 * a button pushed past the bottom of a box with `overflow: hidden` is not clipping its own text
 * and is not making anything scroll. It is simply gone, and it is the specific way a player lost
 * the controls they were being asked to press (#678).
 *
 * `checkVisibility` rather than a `display`/`visibility` pair, because a closed `<details>` in
 * current Chrome hides its contents with `content-visibility` — which still reports a box, at
 * coordinates well outside the dock. Measuring those would report the disclosure that is doing
 * exactly what a disclosure should as the defect this is looking for.
 */
async function outsideTheDock(page: Page): Promise<string[]> {
  return page.getByRole('region', { name: 'Actions' }).evaluate((root) => {
    const box = root.getBoundingClientRect()
    const out: string[] = []
    for (const node of root.querySelectorAll('button, input, summary, p, li')) {
      const el = node as HTMLElement
      if (
        !el.checkVisibility({
          contentVisibilityAuto: true,
          opacityProperty: true,
          visibilityProperty: true,
        })
      ) {
        continue
      }
      const rect = el.getBoundingClientRect()
      if (rect.width <= 1 || rect.height <= 1) continue
      // One pixel of slack for the sub-pixel rounding a fractional viewport produces, and no more.
      if (
        rect.top < box.top - 1 ||
        rect.bottom > box.bottom + 1 ||
        rect.left < box.left - 1 ||
        rect.right > box.right + 1
      ) {
        out.push(
          `${el.tagName.toLowerCase()} "${(el.textContent ?? '').replace(/\s+/g, ' ').trim().slice(0, 40)}" ` +
            `${Math.round(rect.top)}–${Math.round(rect.bottom)} in ${Math.round(box.top)}–${Math.round(box.bottom)}`,
        )
      }
    }
    return out
  })
}

/** The keep-or-mulligan contract fixture, painted at one viewport. */
async function mulligan(page: Page, viewport: { width: number; height: number }) {
  await page.setViewportSize({ width: viewport.width, height: viewport.height })
  await serveFrames(page, [fixture('gameview-prompts.json')])
  await page.goto('/')
  await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()
}

/** Arm the action the dock is offering by that name, and wait for its questions to be drawn. */
async function armed(page: Page, label: string) {
  const dock = page.getByRole('region', { name: 'Actions' })
  // Exact: the turn strip carries a step called "Declare blockers" and it is a different control.
  await dock.getByRole('button', { name: label, exact: true }).click()
  await expect(page.getByRole('region', { name: 'Choices' })).toBeVisible()
}

/**
 * A board where the answer is *on the board*: one attacker, and `count` creatures that may block.
 *
 * There is no committed fixture for a blocking declaration, so the view is written out here from
 * `docs/protocol.md` — the same way `views.spec.ts` writes out the combat shapes. Every candidate
 * is a permanent the board draws, which is the case §6.5 is about: the dock carries the tally and
 * the two controls, and the twenty subjects are answered where they lie.
 */
const blocking = (count: number) => ({
  you: 'p0',
  phase: 'declare_blockers',
  turn: 6,
  me: { life: 20, library_size: 40 },
  opponents: [{ player_id: 'p1', life: 20, hand_size: 3, library_size: 40, graveyard_size: 0 }],
  seat_order: ['p0', 'p1'],
  active_player: 'p1',
  priority_player: 'p0',
  battlefield: [
    {
      id: 'perm_attacker',
      controller: 'p1',
      owner: 'p1',
      attacking_player: 'p0',
      card: {
        id: 'perm_attacker',
        name: 'Colossal Dreadmaw',
        type_line: 'Creature — Dinosaur',
        card_types: ['creature'],
        power: '6',
        toughness: '6',
      },
    },
    ...Array.from({ length: count }, (_, index) => ({
      id: `perm_block_${index}`,
      controller: 'p0',
      owner: 'p0',
      card: {
        id: `perm_block_${index}`,
        name: `Wall of Vines ${index + 1}`,
        type_line: 'Creature — Plant Wall',
        card_types: ['creature'],
        power: '0',
        toughness: '3',
      },
    })),
  ],
  valid_actions: [
    {
      id: 'blk',
      type: 'declare_blockers',
      label: 'Declare blockers',
      token: 'tblk',
      requirements: [
        {
          slot: 'blockers',
          prompt: 'Choose which creatures block',
          candidates: Array.from({ length: count }, (_, index) => `perm_block_${index}`),
        },
      ],
    },
  ],
})

for (const viewport of VIEWPORTS) {
  test.describe(viewport.name, () => {
    /**
     * The sweep sees the board, which is what makes the two sweeps below mean anything.
     *
     * A `textIn` that found nothing would satisfy both filters while proving nothing at all —
     * the one way this file could be quietly wrong. It is its own test, and an unannotated one,
     * for exactly that reason: folded into an expected failure, a sweep that had gone blind
     * would report as the defect it was hiding rather than as itself. Held to passing at every
     * size, and passing at every size today.
     */
    test('sees the text the board draws', async ({ page }) => {
      await table(page, viewport)
      expect((await textIn(page)).length).toBeGreaterThan(20)
    })

    /**
     * The assertion that would have caught `C…`.
     *
     * §2 is explicit that the order is shrink → wrap → remove and that truncation is not a step
     * in it: a name a player cannot read is not a smaller version of the information, it is the
     * absence of it. So nothing that draws text may need more room than it was given, in either
     * axis — one pixel of slack for the sub-pixel rounding a fractional viewport produces, and
     * no more.
     */
    test('draws no text it then cuts off', async ({ page }) => {
      known(knownAt(viewport).clipped)
      await table(page, viewport)
      expect(
        (await textIn(page)).filter(
          (m) => m.scrollWidth > m.clientWidth + 1 || m.scrollHeight > m.clientHeight + 1,
        ),
      ).toEqual([])
    })

    /**
     * §3's last line, which is the one the whole document was written to make true: "No region
     * of the board ever scrolls. A board that has to be scrolled to be seen is not a board."
     * When content exceeds the room the ladder applies — cards shrink, rows merge, faces become
     * chips — and it never falls through to overflow.
     */
    test('makes no region of the board reachable only by scrolling', async ({ page }) => {
      known(knownAt(viewport).scrolls)
      await table(page, viewport)
      expect(await boardScrolling(page)).toEqual([])
    })

    /** And the page under all of it holds still, in both axes. */
    test('does not scroll the page itself', async ({ page }) => {
      await table(page, viewport)
      expect(await pageFits(page)).toEqual({ x: true, y: true })
    })

    /**
     * Tier 1, in full: everything §2 says is visible without a gesture, at this size.
     *
     * Addressed by role and accessible name throughout. What is asserted is that each fact is on
     * screen, not where it is: whether it is *readable* once it is there is what the two sweeps
     * above ask, and the arrangement that places it is §4's business and changes band by band.
     * This list does not change at all.
     */
    test('keeps every tier-1 fact on screen without a gesture', async ({ page }) => {
      await table(page, viewport)

      // Whose priority it is, and what is being asked. Two different facts: the seat that may
      // act, and the sentence the dock states about the player's own obligation.
      await expect(page.getByText(/Priority: ?Ada/)).toBeVisible()
      const dock = page.getByRole('region', { name: 'Actions' })
      await expect(dock.getByRole('status').first()).not.toBeEmpty()

      // The action affordance: a fixed, known place, with the actions the server offered in it.
      await expect(dock).toBeVisible()
      await expect(dock.getByRole('button', { name: 'Pass' })).toBeVisible()

      // Your hand — your option set. It may be a peek strip at this size; it is never absent,
      // and what is in it is still identifiable.
      const hand = page.getByRole('region', { name: 'Your hand' })
      await expect(hand).toBeVisible()
      await expect(hand.getByRole('button', { name: /^Lightning Strike/ })).toBeVisible()

      // Both battlefields, named as non-negotiable.
      const fields = page.getByRole('region', { name: /battlefield$/ })
      await expect(fields).toHaveCount(2)
      await expect(fields.first()).toBeVisible()
      await expect(fields.last()).toBeVisible()

      // Every seat's life total. Read from the seat rather than from a class, so a bar that is
      // rebuilt keeps answering — and asserted for both seats, because §2's fold is symmetric
      // and life is the one thing on either bar that may never fold.
      const seats = page.getByRole('region', { name: /seat$/ })
      await expect(seats).toHaveCount(2)
      await expect(
        page.getByRole('region', { name: 'Your seat' }).getByText('14 life'),
      ).toBeVisible()
      await expect(
        page.getByRole('region', { name: 'Bo (p2) seat' }).getByText('9 life'),
      ).toBeVisible()

      // The stack is seven deep, so it is the most urgent thing on screen, and §3 says its top
      // item by name never degrades. The client names it "Resolves next" rather than leaving a
      // player to infer which end of a column is the top.
      const stack = page.getByRole('region', { name: 'Stack' })
      await expect(stack).toBeVisible()
      const top = stack.getByRole('listitem').filter({ hasText: 'Resolves next' })
      await expect(top).toHaveCount(1)
      await expect(top).toContainText('Equipped creature deals 1 damage to each of two targets.')

      // The current step. It changes what every option means, and it costs one word.
      await expect(page.getByRole('heading', { name: /Declare blockers/ })).toBeVisible()
    })

    /**
     * §2's floors: 9px effective for text on a card, 11px for chrome.
     *
     * Deliberately small, and that is the point — XMage fits a complete name, cost, type line,
     * keywords and P/T into a 72×100 tile at roughly 9px, and complete-and-small is readable
     * where large-and-truncated is not. Below the floor the rule is not "shrink further", it is
     * §3: drop the secondary text, then the face itself becomes a chip.
     */
    test('renders no text below its type floor', async ({ page }) => {
      await table(page, viewport)
      expect((await textIn(page)).filter((m) => m.fontSize < m.floor)).toEqual([])
    })

    /**
     * The defect #678 is about, asserted where it happened: a question bigger than the band.
     *
     * The dock's box is `scene()`'s and is as little as 44px on half of the viewports in this
     * list, so a question drawn at desktop type had its controls pushed out of the band and
     * `overflow: hidden` cut them — the player losing the very buttons they were being asked to
     * press. It is a different failure from the two sweeps above and needs its own measurement:
     * nothing there is clipping its own text, and the dock is not scrolling. The controls are
     * simply *below the box*.
     *
     * Both questions the issue names, in sequence, so what is asserted is a real path through a
     * prompt rather than one frame of it: the mulligan decision, and the bottoming question that
     * only appears once *keep* is chosen — the case that made this three rows deep.
     */
    test('cuts nothing off the question it is asking', async ({ page }) => {
      await mulligan(page, viewport)
      expect(await outsideTheDock(page)).toEqual([])

      await armed(page, 'Keep or mulligan')
      expect(await outsideTheDock(page)).toEqual([])

      // The follow-up question, and the tally that is the only statement of how far the answer
      // has got. Both have to be inside the band, and the controls under them with it.
      await page.getByRole('button', { name: 'Keep this hand' }).click()
      await expect(page.getByRole('region', { name: 'Choices' })).toContainText('0 of 1')
      expect(await outsideTheDock(page)).toEqual([])
    })

    /**
     * §5, applied to the dock: the band responds to *whether* the game is asking, never to how
     * much there is to ask about. Two legal blockers and twenty are the same question.
     *
     * A sweep over the count rather than a pair of expected pixel values — the recurring defect
     * in this client is a number that is not monotone across a boundary, and a table of
     * expectations is exactly what fails to see one.
     */
    test('is the same dock for twenty legal blockers as for two', async ({ page }) => {
      const heights: number[] = []
      for (const count of [2, 6, 20]) {
        await page.setViewportSize({ width: viewport.width, height: viewport.height })
        await serveFrames(page, [blocking(count)])
        await page.goto('/')
        await armed(page, 'Declare blockers')

        // Every one of them is on the board, highlighted, and answers the slot the server listed
        // it in. None of them is a button in the dock, which is what used to make this grow.
        await expect(
          page.getByRole('region', { name: 'Your battlefield' }).getByRole('listitem'),
        ).toHaveCount(count)
        await expect(
          page.getByRole('region', { name: 'Choices' }).getByRole('button', { name: /^Wall/ }),
        ).toHaveCount(0)

        expect(await outsideTheDock(page)).toEqual([])
        heights.push((await dockBox(page)).height)
      }
      expect(new Set(heights).size).toBe(1)
    })
  })
}

/**
 * The count a field has to absorb, at both ends of the supported range.
 *
 * §5's rule is that a region is sized by the viewport and the count is absorbed by the cards, so
 * the interesting board is the one no committed fixture has: twenty permanents a seat, which is
 * three times what the field was ever laid out for. Nothing about the field may change — not its
 * height, not its position, and above all not whether it scrolls. What changes is the tiles: they
 * shrink to §5's floor, take a second line where the height is going spare, and fan where it is
 * not.
 *
 * Asserted on the battlefields specifically rather than through the whole-board sweep above,
 * because that sweep is still red for the stack (#662) at both of these sizes and an assertion
 * that cannot go green says nothing about the thing it is named after.
 */
/**
 * §5's split, and what it costs — the two halves of the same rule, at every supported size.
 *
 * **Creatures, other permanents and lands stay in their own rows and the cards shrink to make
 * that possible** (§3, "The split is kept, and the cards get smaller"). The board is read by
 * category at a glance, and the maintainer's report on a 1920×1080 board that had merged them was
 * unambiguous: *"the creatures and lands mixing together is unacceptable."*
 *
 * What it costs is card size, and §5 makes 72×100 a **review threshold** rather than a
 * stop-drawing line: below it the client still draws the card and *reports* that it had to. That
 * report is `data-below-floor` on the row, carrying the size it came out at, and this is the gate
 * asserting on it — not as a failure, because whether a 47×66 card is too small is exactly the
 * judgment §3 leaves to a person, but as a measurement that appears in the run rather than in
 * nobody's head. What it does assert is the part that is not a judgment: a tile the client says is
 * under the minimum still names its card.
 */
test.describe('the split, and what it costs', () => {
  for (const viewport of VIEWPORTS) {
    test(`keeps a row per category, and says what it cost, at ${viewport.name}`, async ({
      page,
    }) => {
      await table(page, viewport)
      const fields = page.getByRole('region', { name: /battlefield$/ })
      await expect(fields).toHaveCount(2)

      // The fixture's board has creatures and lands on both halves, so a field that draws one
      // row has merged them. `scene.ts` merges below its own threshold (§3, step 6) — the phone
      // sizes here are exactly that — so the assertion is over the sizes §1 calls Optimized.
      const rows = await fields.first().getByRole('list').count()
      if (viewport.width >= 1280 && viewport.height >= 720) {
        expect(rows, `${viewport.name}: one list means the categories merged`).toBeGreaterThan(1)
      }

      // Every row reporting a sub-minimum tile, with the size it drew, in the run's own output.
      const under = await page
        .locator('.field__row[data-below-floor]')
        .evaluateAll((nodes) =>
          nodes.map(
            (node) =>
              `${node.getAttribute('aria-label') ?? '?'}: ${node.getAttribute('data-below-floor')}`,
          ),
        )
      if (under.length > 0) {
        test.info().annotations.push({
          type: 'under §5’s 72×100 minimum',
          description: `${viewport.name} — ${[...new Set(under)].join(', ')}`,
        })
      }

      // And a card the client drew under the minimum is still a card: it names itself. That is
      // the line §6 draws — either the name fits, or the tile was never a card.
      for (const row of await page.locator('.field__row[data-below-floor]').all()) {
        const named = await row
          .getByRole('listitem')
          .evaluateAll((tiles) =>
            tiles.map((tile) => (tile.querySelector('.card__name')?.textContent ?? '').trim()),
          )
        expect(named.filter((name) => name === '')).toEqual([])
      }
    })
  }
})

test.describe('a field holding twenty permanents a seat', () => {
  /** The dense board with its permanents cloned up to twenty a seat, ids and cards distinct. */
  const crowded = () => {
    const base = fixture('gameview-board.json')
    const battlefield = base.battlefield as Record<string, unknown>[]
    const source = battlefield.filter((permanent) => permanent.controller === 'p1')
    const extra: Record<string, unknown>[] = []
    for (const seat of ['p1', 'p2']) {
      const have = battlefield.filter((permanent) => permanent.controller === seat).length
      for (let index = 0; index < 20 - have; index++) {
        const from = source[index % source.length] as Record<string, unknown>
        const card = { ...(from.card as Record<string, unknown>) }
        const id = `${seat}_copy_${index}`
        card.id = id
        extra.push({
          ...from,
          id,
          card,
          controller: seat,
          owner: seat,
          physical_card: `${String(from.physical_card)}_${id}`,
          // The clones are plain permanents: a copy of an attacker would state an attack the
          // rest of the view knows nothing about.
          attacking: undefined,
          attacking_player: undefined,
          blocking: undefined,
          attached_to: undefined,
        })
      }
    }
    return { ...base, battlefield: [...battlefield, ...extra] }
  }

  for (const viewport of [
    { name: '1920×1080', width: 1920, height: 1080 },
    { name: '640×360', width: 640, height: 360 },
  ]) {
    test(`draws all forty of them, unscrolled, at ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height })
      await serveFrames(page, [crowded()])
      await page.goto('/')
      await expect(page.getByRole('region', { name: 'Actions' })).toBeVisible()

      const fields = page.getByRole('region', { name: /battlefield$/ })
      await expect(fields).toHaveCount(2)

      // Every permanent has a box — an object with none can be identified by no gesture, and it
      // is what the relationship overlay looks for (#663).
      for (const field of await fields.all()) {
        await expect(field.getByRole('listitem')).toHaveCount(20)
      }

      // And the field holds them. Not "scrolls to them": there is no scrollable ancestor and no
      // overflow to reach, at three times the count the box was laid out for.
      expect(
        await fields.evaluateAll((roots) =>
          roots
            .filter(
              (root) =>
                root.scrollWidth > root.clientWidth + 1 ||
                root.scrollHeight > root.clientHeight + 1,
            )
            .map(
              (root) =>
                `${root.scrollWidth}×${root.scrollHeight} in ${root.clientWidth}×${root.clientHeight}`,
            ),
        ),
      ).toEqual([])

      // Each of them still says which card it is. The visible name may be abbreviated on the
      // board (§6) and it may be partly covered by the tile after it; what it may never be is
      // absent, and the whole name is on the tile for a screen reader either way.
      const named = await fields
        .first()
        .getByRole('listitem')
        .evaluateAll((tiles) =>
          tiles.map((tile) => (tile.querySelector('.card__name')?.textContent ?? '').trim()),
        )
      expect(named).toHaveLength(20)
      expect(named.filter((name) => name === '')).toEqual([])
    })
  }
})

/**
 * §6's turn, at every band: **a tapped permanent stays on its own half of the table.**
 *
 * A quarter turn is the one thing on the board whose *painted* rectangle is not its laid-out one,
 * so it is the one thing the region sweeps above cannot see: `scrollWidth` says nothing about a
 * card rotated out over its neighbour or off the edge of the field. The room it turns into is
 * reserved by `pack.ts` for every tile whether or not anything is tapped, and this is that
 * reservation asserted where a player meets it — with the fixture's three tapped permanents, at
 * every supported size, including the ones where a permanent is a chip and does not turn at all.
 */
test.describe('a permanent that taps', () => {
  for (const viewport of VIEWPORTS) {
    test(`turns inside its own field at ${viewport.name}`, async ({ page }) => {
      await table(page, viewport)
      const fields = page.getByRole('region', { name: /battlefield$/ })
      await expect(fields).toHaveCount(2)

      const escaped = await fields.evaluateAll((roots) => {
        const over: string[] = []
        for (const root of roots) {
          const bounds = root.getBoundingClientRect()
          for (const node of root.querySelectorAll<HTMLElement>('.card')) {
            // The painted rectangle, which is what a rotation changes. One pixel of slack for the
            // sub-pixel rounding a fractional viewport produces, and no more.
            const box = node.getBoundingClientRect()
            if (
              box.left < bounds.left - 1 ||
              box.right > bounds.right + 1 ||
              box.top < bounds.top - 1 ||
              box.bottom > bounds.bottom + 1
            ) {
              over.push(
                `${node.querySelector('.card__name')?.textContent ?? '?'}: ` +
                  `${Math.round(box.left)}…${Math.round(box.right)} in ` +
                  `${Math.round(bounds.left)}…${Math.round(bounds.right)}`,
              )
            }
          }
        }
        return over
      })
      expect(escaped).toEqual([])

      // And the fixture really does have something turned at this size, so the assertion above is
      // about a board with a turn in it rather than about one without.
      const marked = await page
        .locator('.card--tapped')
        .evaluateAll((nodes) =>
          nodes.map((node) => getComputedStyle(node as HTMLElement).transform),
        )
      expect(marked.length).toBeGreaterThan(0)
    })
  }
})

/**
 * The one exemption §3 grants, stated where it can be seen rather than left to a selector.
 *
 * A pile opened on demand is not the board: it is a list a player asked for, it can be fifty
 * cards long, and it may scroll. It is exempt from the sweep above because `BOARD` enumerates
 * the regions the board is made of and a pile is not one of them — which is a decision on the
 * record, not a wildcard that happens to miss it. With one open, the board is still held to
 * every rule it was held to before.
 */
test.describe('a pile opened on demand', () => {
  /** The board at 1440×900 with a ten-card graveyard open beside it. */
  const openPile = async (page: Page) => {
    await table(page, { width: 1440, height: 900 })
    await page
      .getByRole('region', { name: 'Your seat' })
      .getByRole('button', { name: /^Graveyard/ })
      .click()
  }

  test('is exempt, and opens with everything in it', async ({ page }) => {
    await openPile(page)
    const pile = page.getByRole('region', { name: 'Graveyard' })
    await expect(pile).toBeVisible()
    await expect(pile.getByRole('listitem')).toHaveCount(10)
  })

  // Whatever the pile does with its own overflow, the board beside it is held to the rules it
  // was held to before — split in two so that fixing the scrolling does not quietly carry the
  // clipping with it, and so a pile that started dragging the board around would be visible as
  // one of these turning red rather than as an unchanged failure in a test asserting both.
  test('leaves the board no more scrollable than it already was', async ({ page }) => {
    known(KNOWN['1440×900 with a pile open']?.scrolls)
    await openPile(page)
    expect(await boardScrolling(page)).toEqual([])
  })

  test('leaves the board no more clipped than it already was', async ({ page }) => {
    known(KNOWN['1440×900 with a pile open']?.clipped)
    await openPile(page)
    expect(
      (await textIn(page)).filter(
        (m) => m.scrollWidth > m.clientWidth + 1 || m.scrollHeight > m.clientHeight + 1,
      ),
    ).toEqual([])
  })
})

/**
 * Below the floor, the answer is a sentence — not a board that is technically present.
 *
 * §1 gives Unsupported one commitment: "Say so plainly, in place of a broken board." *In place
 * of* is the whole of it. A notice layered over a table that is still being drawn underneath is
 * the failure mode this forbids: it costs the same layout, it leaves a player poking at
 * something that half works, and it makes "unsupported" a decoration rather than a decision.
 * `scene.ts` already answers `unsupported` for this viewport and returns no regions at all.
 */
test.describe('a viewport below the floor', () => {
  const belowTheFloor = async (page: Page) => {
    await page.setViewportSize(UNSUPPORTED)
    await serveFrames(page, [fixture('gameview-board.json')])
    await page.goto('/')
  }

  // The two halves of "say so plainly, **in place of** a broken board" are separate assertions
  // with separate answers, so they are separate tests. A notice that shipped layered over the
  // table would satisfy the first and fail the second, and a single test asserting both could
  // not tell anyone which.
  test('says so, plainly', async ({ page }) => {
    await belowTheFloor(page)
    await expect(page.getByText(/too small|not supported|unsupported/i)).toBeVisible()
  })

  // Nothing that would let a player believe the game is playable here. Asked as one poll over
  // all three counts rather than three assertions in a row, because three in a row stop at the
  // first: the ledger would then record "a battlefield exists" and say nothing about the hand or
  // the dock, and the row would under-report the defect it exists to describe.
  test('draws no board underneath the notice', async ({ page }) => {
    await belowTheFloor(page)
    await expect
      .poll(async () => ({
        battlefields: await page.getByRole('region', { name: /battlefield$/ }).count(),
        hands: await page.getByRole('region', { name: 'Your hand' }).count(),
        docks: await page.getByRole('region', { name: 'Actions' }).count(),
      }))
      .toEqual({ battlefields: 0, hands: 0, docks: 0 })
  })

  test('does not scroll the page', async ({ page }) => {
    await belowTheFloor(page)
    expect(await pageFits(page)).toEqual({ x: true, y: true })
  })
})
