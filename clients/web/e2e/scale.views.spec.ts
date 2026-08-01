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
  /** §2: text below its type floor — 9px on a card, 11px on chrome. */
  floor?: string
  /** §1: the notice a viewport below the floor is owed. */
  notice?: string
  /** §1: *in place of* — no board drawn underneath that notice. */
  board?: string
}

/**
 * Keyed by `width×height` rather than by the prose name, so renaming a band does not silently
 * orphan a row into a permanently-red unexpected pass.
 *
 * Six of the clipped elements are the same six at every size, and that repetition is the finding
 * rather than noise: a relationship trail's control needs 304px in 192px on a 3440px ultrawide
 * exactly as it does on a phone, and the stack's top item needs 44px of height in 30px
 * everywhere, because none of those numbers is derived from the viewport. The floor rows are
 * identical at all eight sizes for the same reason — the sizes are in `rem`, and a `rem` does not
 * shrink with the screen.
 *
 * Measured against `main` at 9084a1c, which is #667 — the redrawn card. That merge retired more
 * than half of what this gate first reported: clipped elements went from 16–34 per viewport to
 * 8–9, and the nine `span.badge` misses of the 9px card floor are gone entirely. What is left is
 * what #667 was never going to reach.
 */
const KNOWN: Record<string, Known> = {
  '1920×1080': {
    clipped:
      '#660/#662/#663 — 9 elements. Six are relationship trails: `Return target creature card…` ' +
      'needs 328px in 192px, `Equipped creature deals 1 damage…` 304px in 192px (×3), ' +
      '`Lightning Strike deals 3 damage…` 242px in 192px. Two are type lines a packed box would ' +
      'give room to — `Creature — Dinosaur` 119px in 116px, `Creature — Ogre Warrior` 118px in ' +
      '116px — and the last is the stack’s top item, wrapping to 44px of height in a 30px band.',
    scrolls:
      '#659/#660/#662 — `.field--you` holds 690px of content in a 323px box and `.field--opponent` ' +
      '472px in 329px; the stack’s `div.rail` is 1438px inside 775px. When the board is packed ' +
      'rather than scrolled these fit.',
    floor:
      '#659 — 14 elements, all chrome, all against the 11px floor: `span.strip__name` ×12 (the ' +
      'turn rail’s step names) at 8.8px, `p.seat__pool` and `span.pip--restricted` at 10.56px.',
  },
  '1440×900': {
    clipped:
      '#660/#662/#663 — 8 elements: the same six trails, `Creature — Ogre Warrior` 106px in ' +
      '102px, and the stack’s top item at 44px of height in 30px.',
    scrolls:
      '#659/#660/#662 — `.field--you` holds 633px of content in a 247px box, the single clearest ' +
      'case in the file: two and a half times what it can show, resolved with a scrollbar instead ' +
      'of packing. `.field--opponent` 434px in 254px; `div.rail` 1438px in 624px.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },
  '1280×720': {
    clipped:
      '#660/#662/#663 — 8 elements: the same six trails, `Onakke Ogre` wrapping to 32px of height ' +
      'in a 16px name band, and the stack’s top item at 44px in 30px.',
    scrolls:
      '#659/#660/#662 — `.field--you` holds 706px in a 169px box, `.field--opponent` 373px in ' +
      '175px, `div.rail` 1438px in 468px.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },
  '640×360': {
    clipped:
      '#660/#662/#663 — 9 elements: the same six trails, `Colossal Dreadmaw` 69px in a 66px name ' +
      'band, `Gravedigger` 72px in 66px, and the stack’s top item at 44px in 30px.',
    scrolls:
      '#659/#660/#662 — both battlefields collapse to a 16px box holding 749px and 1073px; ' +
      '`.seat--opponent` is 202px wide in 149px; `div.rail` 1438px in 42px.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },
  '390×844': {
    clipped:
      '#660/#662/#663 — 8 elements: the same six trails, `Marauder’s Axe` wrapping to 32px of ' +
      'height in a 16px name band, and the stack’s top item at 44px in 30px.',
    scrolls:
      '#659/#660/#662 — `.field--you` holds 1359px in a 16px box and `.field--opponent` 985px in ' +
      '16px; both seats are 202px and 83px wide in 19px; the dock’s section is 117px wide in ' +
      '107px; `div.rail` 1438px in 320px.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },
  '844×390': {
    clipped:
      '#660/#662/#663 — 9 elements: the same six trails, `Colossal Dreadmaw` 69px in a 66px name ' +
      'band, `Gravedigger` 72px in 66px, and the stack’s top item at 44px in 30px.',
    scrolls:
      '#659/#660/#662 — `.field--you` holds 1026px in a 16px box, `.field--opponent` 605px in ' +
      '16px, `div.rail` 1438px in 115px.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },
  '3440×1440': {
    clipped:
      '#660/#662/#663 — 9 elements on 4.9 megapixels of screen, which is the point: the six ' +
      'trails still need up to 328px in 192px, two type lines 119px in 116px, and the stack’s ' +
      'top item 44px of height in 30px, because none of those boxes is derived from the room ' +
      'available.',
    scrolls:
      '#659/#660/#662 — `.field--you` holds 690px in a 541px box and `div.rail` 1438px in 1135px, ' +
      'on the largest screen the client supports.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },
  '320×480': {
    clipped:
      '#660/#662/#663 — 9 elements: the same six trails, `Colossal Dreadmaw` 69px in a 66px name ' +
      'band, `Gravedigger` 72px in 66px, and the stack’s top item at 44px in 30px.',
    scrolls:
      '#659/#660/#662 — nearly every region: `header.match` 85px wide in 56px, both seats in a ' +
      '19px box, `.field--you` 1089px in 16px, `section.hand` 150px in 99px, the dock 258px in ' +
      '106px, and `div.rail` 1438px in **6px**.',
    floor:
      '#659 — the same 14: `span.strip__name` ×12 at 8.8px, `p.seat__pool` and ' +
      '`span.pip--restricted` at 10.56px, against an 11px chrome floor.',
  },

  // The two cases that are not one of the eight: a pile open changes the numbers, and the
  // unsupported viewport is below §1's floor on both edges and answers different questions.
  '1440×900 with a pile open': {
    scrolls:
      '#659/#660/#662 — the board behind the pile is the 1440×900 board plus the graveyard’s own ' +
      'permanents: `.field--you` holds 827px in a 247px box, `.field--opponent` 434px in 254px, ' +
      '`div.rail` 1438px in 624px. Nothing *inside* the pile is reported, which is the exemption ' +
      'working; this row is the board beside it.',
    clipped: '#660/#662/#663 — the same 8 the 1440×900 row describes, unchanged by the pile.',
  },
  '300×400': {
    notice:
      '#659 — there is no notice of any kind at 300×400, and `page.getByText(/too small|not ' +
      'supported|unsupported/i)` finds nothing. `scene.ts` already classifies this viewport as ' +
      '`unsupported` and returns no regions for it; nothing renders that answer yet. Whichever ' +
      'surface adds the notice should expect to adjust that regex — the wording is not agreed.',
    board:
      '#659 — the whole board is drawn instead, at a size §1 says is unsupported: ' +
      '`{ battlefields: 2, hands: 1, docks: 1 }` against `{ 0, 0, 0 }`. This is the *in place of* ' +
      'half, and before the split it sat behind the notice assertion and was never reached — the ' +
      'ledger would have recorded one defect where there are two.',
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
      known(knownAt(viewport).floor)
      await table(page, viewport)
      expect((await textIn(page)).filter((m) => m.fontSize < m.floor)).toEqual([])
    })
  })
}

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
    known(KNOWN['300×400']?.notice)
    await belowTheFloor(page)
    await expect(page.getByText(/too small|not supported|unsupported/i)).toBeVisible()
  })

  // Nothing that would let a player believe the game is playable here. Asked as one poll over
  // all three counts rather than three assertions in a row, because three in a row stop at the
  // first: the ledger would then record "a battlefield exists" and say nothing about the hand or
  // the dock, and the row would under-report the defect it exists to describe.
  test('draws no board underneath the notice', async ({ page }) => {
    known(KNOWN['300×400']?.board)
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
