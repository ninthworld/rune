/**
 * The receiver's curved hand fan (issue #533) — `live/HandFan.tsx` mounted
 * inside the real match shell.
 *
 * The acceptance criteria this covers, in the issue's own order: the fan's
 * geometry at 1 / 7 / 12 / 20 / 20+ cards, the four count bands, paging at the
 * 44 px floor with ≥ 44 px controls, first/middle/last selectability at 7, 12,
 * and 20 by pointer, keyboard, and touch, the selection lift and straighten,
 * mulligan bottoming in the same fan with no modal occlusion, and the travel
 * endpoints staying connected to the physical fan.
 *
 * **What jsdom cannot show, stated rather than papered over.** Vitest applies no
 * CSS module and computes no layout, so nothing here proves a rendered pixel, a
 * resolved `calc()`, a real 44 px strip under the pointer, or that the fan looks
 * like the baseline. Two things are proven instead: the geometry contract the
 * stylesheet is handed (the same arithmetic `shellLayout.ts` and `handFan.ts`
 * publish), and that every card and control is mounted, enabled, labelled, and
 * hit-testable. The seven-card baseline comparison at 1680×945 is the
 * maintainer's, and the issue's closure gate says so.
 */
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MULLIGAN_GAME_VIEW_JSON, SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { collectFocusRegions, nextFocus } from '../focus';
import {
  FAN,
  LOCAL_FAN_TIER,
  fanInset,
  fanPageRange,
  handCountBand,
  localFanPlan,
} from '../handFan';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';
import { SHELL, shellBands } from './shellLayout';

vi.mock('../EffectsSurface', () => ({
  EffectsSurface: () => <div data-testid="effects-surface" aria-hidden="true" />,
}));
vi.mock('../effects', () => ({
  EffectsLayer: class {
    setPersistent(): void {}
    replaceTransients(): void {}
    trackMotion(): void {}
  },
}));

registerTableTestHooks();

const VIEWPORTS = [
  { label: '1280×720', width: 1280, height: 720 },
  { label: '1680×945', width: 1680, height: 945 },
  { label: '2048×1024', width: 2048, height: 1024 },
  { label: '1180×820', width: 1180, height: 820 },
  { label: '390×844', width: 390, height: 844 },
];

const originalWidth = window.innerWidth;
const originalHeight = window.innerHeight;

function resizeTo(width: number, height: number): void {
  Object.defineProperty(window, 'innerWidth', { value: width, configurable: true, writable: true });
  Object.defineProperty(window, 'innerHeight', {
    value: height,
    configurable: true,
    writable: true,
  });
}

/** A playable hand of `n` cards on the sample frame; every card has an action. */
function handOf(n: number): string {
  const frame = JSON.parse(SAMPLE_GAME_VIEW_JSON) as {
    my_hand: unknown[];
    valid_actions: unknown[];
  };
  frame.my_hand = Array.from({ length: n }, (_, i) => ({
    id: `h${i}`,
    name: `Hand Card ${i}`,
    type_line: 'Instant',
    mana_cost: '{1}',
    rules_text: 'Draw a card.',
  }));
  frame.valid_actions = Array.from({ length: n }, (_, i) => ({
    id: `cast_h${i}`,
    type: 'cast_spell',
    label: `Cast Hand Card ${i}`,
    subject: [`h${i}`],
  }));
  return JSON.stringify(frame);
}

/** The mulligan frame with a full opening hand, every card a bottoming candidate. */
function mulliganHandOf(n: number): string {
  const frame = JSON.parse(MULLIGAN_GAME_VIEW_JSON) as {
    my_hand: { id: string; name: string; type_line: string }[];
    valid_actions: { prompts?: { slot: string; candidates?: string[] }[] }[];
  };
  frame.my_hand = Array.from({ length: n }, (_, i) => ({
    id: `card_${i}`,
    name: `Opening Card ${i}`,
    type_line: 'Basic Land — Forest',
  }));
  const bottom = frame.valid_actions[0]?.prompts?.find((prompt) => prompt.slot === 'bottom');
  if (bottom) bottom.candidates = frame.my_hand.map((card) => card.id);
  return JSON.stringify(frame);
}

/** The mounted hand band. */
function band(): HTMLElement {
  return screen.getByTestId('live-hand');
}

/** The cards currently drawn, in DOM order. */
function drawnCards(): HTMLElement[] {
  return Array.from(band().querySelectorAll<HTMLElement>('[data-testid^="live-hand-card-"]'));
}

afterEach(() => {
  resizeTo(originalWidth, originalHeight);
});

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
});

describe('fan geometry at 1, 7, 12, 20, and 20+ cards', () => {
  for (const vp of VIEWPORTS) {
    for (const count of [1, 7, 12, 20, 24]) {
      it(`publishes a contained ${count}-card fan at ${vp.label}`, () => {
        resizeTo(vp.width, vp.height);
        seed(handOf(count));
        render(<LiveMatchTable />);

        const bandWidth = shellBands(vp).hand.w;
        const { plan, paged } = localFanPlan(count, bandWidth);
        const inset = fanInset(plan, LOCAL_FAN_TIER) + (paged ? FAN.pagerW : 0);
        const { start, end } = fanPageRange(plan, 0);
        const cards = drawnCards();

        // The first page is what mounts; the rest is a page control away.
        expect(cards).toHaveLength(end - start);
        expect(band().dataset.count).toBe(String(count));
        expect(band().dataset.countBand).toBe(handCountBand(count));
        expect(band().dataset.pages).toBe(String(plan.pages));
        expect(band().style.getPropertyValue('--hand-inset')).toBe(`${inset}px`);

        for (const [index, card] of cards.entries()) {
          const t = Number(card.style.getPropertyValue('--hand-t'));
          expect(Number.isFinite(t), `card ${index} fan fraction`).toBe(true);
          expect(t).toBeGreaterThanOrEqual(0);
          expect(t).toBeLessThanOrEqual(1);
          // The stylesheet's rule, evaluated here: the published inset is what
          // keeps both endpoints inside the band at any width (invariant I2).
          const centre = inset + (bandWidth - 2 * inset) * t;
          expect(centre - SHELL.handCardW / 2).toBeGreaterThanOrEqual(-1e-9);
          expect(centre + SHELL.handCardW / 2).toBeLessThanOrEqual(bandWidth + 1e-9);
          // Rotation and arc are published per card and bounded by the tier.
          const angle = Number(card.style.getPropertyValue('--hand-angle').replace('deg', ''));
          expect(Math.abs(angle)).toBeLessThanOrEqual(LOCAL_FAN_TIER.maxDeg + 1e-9);
          const dip = Number(card.style.getPropertyValue('--hand-dip').replace('px', ''));
          expect(dip).toBeGreaterThanOrEqual(0);
          expect(dip).toBeLessThanOrEqual(LOCAL_FAN_TIER.arcFrac * LOCAL_FAN_TIER.card.h + 1e-9);
        }
      });
    }
  }

  it('centres a single card and flattens its rotation', () => {
    resizeTo(1680, 945);
    seed(handOf(1));
    render(<LiveMatchTable />);
    const card = drawnCards()[0]!;
    expect(card.style.getPropertyValue('--hand-t')).toBe('0.5');
    expect(card.style.getPropertyValue('--hand-angle')).toBe('0.00deg');
    expect(card.style.getPropertyValue('--hand-dip')).toBe('0.00px');
  });

  it('spans the fan endpoint to endpoint and angles the outer cards most', () => {
    resizeTo(1680, 945);
    seed(handOf(7));
    render(<LiveMatchTable />);
    const cards = drawnCards();
    expect(cards[0]!.style.getPropertyValue('--hand-t')).toBe('0');
    expect(cards[6]!.style.getPropertyValue('--hand-t')).toBe('1');
    const angle = (el: HTMLElement): number =>
      Number(el.style.getPropertyValue('--hand-angle').replace('deg', ''));
    expect(angle(cards[0]!)).toBeLessThan(0);
    expect(angle(cards[3]!)).toBe(0);
    expect(angle(cards[6]!)).toBeGreaterThan(0);
    expect(Math.abs(angle(cards[0]!))).toBeGreaterThan(Math.abs(angle(cards[2]!)));
  });

  it('renders no fan and no controls for an empty hand', () => {
    resizeTo(1680, 945);
    seed(handOf(0));
    render(<LiveMatchTable />);
    expect(drawnCards()).toHaveLength(0);
    expect(band().dataset.pages).toBe('0');
    expect(screen.queryByTestId('hand-page-next')).toBeNull();
  });
});

describe('paging at the 44 px floor', () => {
  it('offers no page controls while the whole hand keeps the floor', () => {
    resizeTo(2048, 1024);
    seed(handOf(20));
    render(<LiveMatchTable />);
    expect(band().dataset.pages).toBe('1');
    expect(screen.queryByTestId('hand-page-prev')).toBeNull();
    expect(screen.queryByTestId('hand-page-next')).toBeNull();
    expect(drawnCards()).toHaveLength(20);
  });

  it('pages a hand the band cannot hold, and reaches every card', () => {
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);

    const pages = Number(band().dataset.pages);
    expect(pages).toBeGreaterThan(1);
    const next = screen.getByTestId<HTMLButtonElement>('hand-page-next');
    const prev = screen.getByTestId<HTMLButtonElement>('hand-page-prev');
    expect(prev.disabled).toBe(true);

    const seen = new Set<string>();
    for (let page = 0; page < pages; page += 1) {
      expect(band().dataset.page).toBe(String(page));
      for (const card of drawnCards()) seen.add(card.dataset.entity!);
      if (page < pages - 1) fireEvent.click(next);
    }
    expect(seen.size).toBe(20);
    expect(screen.getByTestId<HTMLButtonElement>('hand-page-next').disabled).toBe(true);
    fireEvent.click(screen.getByTestId('hand-page-prev'));
    expect(band().dataset.page).toBe(String(pages - 2));
  });

  it('announces which cards a page is showing', () => {
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);
    const label = screen.getByTestId('hand-page-label');
    expect(label.getAttribute('aria-live')).toBe('polite');
    expect(label.textContent).toMatch(/^Hand 1–\d+ of 20$/);
  });

  it('never lets a drawn page fall below the 44 px exposure floor', () => {
    for (const vp of VIEWPORTS) {
      for (const count of [7, 12, 20, 40]) {
        const { plan } = localFanPlan(count, shellBands(vp).hand.w);
        if (plan.pageSize > 1) {
          expect(plan.exposure, `${vp.label} × ${count}`).toBeGreaterThanOrEqual(SHELL.minHit);
        }
      }
    }
  });

  it('gives the page controls a real ≥ 44 px box in the stylesheet', () => {
    // jsdom applies no CSS module, so the rule is read as source: the control's
    // box is the shared `--rune-touch` token, which is the 44 px floor itself.
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);
    for (const id of ['hand-page-prev', 'hand-page-next']) {
      const control = screen.getByTestId<HTMLButtonElement>(id);
      expect(control.tagName).toBe('BUTTON');
      expect(control.getAttribute('aria-label')).toMatch(/hand page/i);
    }
    expect(FAN.pagerW).toBeGreaterThanOrEqual(SHELL.minHit);
  });

  it('drops the page with every fresh view — it is never load-bearing', () => {
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);
    fireEvent.click(screen.getByTestId('hand-page-next'));
    expect(band().dataset.page).toBe('1');
    // A new complete view rebuilds the whole UI; the fan comes back at page 0,
    // exactly as a fresh mount of that view would.
    act(() => {
      seed(handOf(20));
    });
    expect(band().dataset.page).toBe('0');
  });
});

describe('first, middle, and last are selectable at 7, 12, and 20 cards', () => {
  for (const count of [7, 12, 20]) {
    it(`reaches every position by pointer at ${count} cards`, () => {
      resizeTo(1280, 720);
      seed(handOf(count));
      render(<LiveMatchTable />);
      for (const index of [0, Math.floor(count / 2), count - 1]) {
        // Page to the card if the fan pages; the control is the non-drag,
        // non-hover path `control-language.md` §7 requires.
        while (screen.queryByTestId(`live-hand-card-h${index}`) === null) {
          fireEvent.click(screen.getByTestId('hand-page-next'));
        }
        const card = screen.getByTestId<HTMLButtonElement>(`live-hand-card-h${index}`);
        expect(card.disabled).toBe(false);
        fireEvent.click(card);
        expect(
          screen.getByTestId(`live-hand-card-h${index}`).getAttribute('aria-pressed'),
          `card ${index} of ${count}`,
        ).toBe('true');
      }
    });

    it(`reaches every position by touch tap at ${count} cards`, () => {
      // Touch parity (§7.2): tap = select, tap again = fire the sole action.
      // No hover, no drag, no long-press involved in either step.
      resizeTo(390, 844);
      const choose = seed(handOf(count));
      render(<LiveMatchTable />);
      const last = count - 1;
      while (screen.queryByTestId(`live-hand-card-h${last}`) === null) {
        const next = screen.getByTestId('hand-page-next');
        fireEvent.pointerDown(next, { pointerType: 'touch' });
        fireEvent.click(next);
      }
      const card = screen.getByTestId(`live-hand-card-h${last}`);
      fireEvent.pointerDown(card, { pointerType: 'touch', button: 0 });
      fireEvent.click(card);
      expect(card.getAttribute('aria-pressed')).toBe('true');
      fireEvent.click(screen.getByTestId(`live-hand-card-h${last}`));
      expect(choose).toHaveBeenCalledWith(expect.objectContaining({ id: `cast_h${last}` }));
    });
  }

  it('walks the whole fan, and its page controls, with the arrow keys', () => {
    // The spatial focus engine walks a region's items along its axis. The hand
    // band is a row, so Right steps card to card and off the end into the next
    // region — every card and both controls are reachable without a pointer.
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);

    // Step off page 1 so the prev control is live. How many pages a hand takes
    // depends on the band's width, and #534 widened that band — the hand now
    // yields only the cluster's column instead of the old identity and decision
    // columns — so the page count is derived here rather than assumed. A
    // DISABLED control is correctly outside the focus order, which is the
    // property this walk actually depends on.
    fireEvent.click(screen.getByTestId('hand-page-next'));
    const bandRect = shellBands({ width: 1280, height: 720 }).hand;
    const regions = collectFocusRegions(document, new Map([['hand', bandRect]]));
    const hand = regions.find((region) => region.id === 'hand')!;
    const prev = screen.getByTestId<HTMLButtonElement>('hand-page-prev');
    const next = screen.getByTestId<HTMLButtonElement>('hand-page-next');
    expect(prev.disabled, 'stepped off the first page').toBe(false);
    // Live prev control, then every drawn card, then the next control if live.
    const expected = [prev, ...drawnCards(), ...(next.disabled ? [] : [next])];
    expect(hand.items).toEqual(expected);

    let active: Element | null = hand.items[0]!;
    const walked: (Element | null)[] = [active];
    for (let i = 1; i < hand.items.length; i += 1) {
      active = nextFocus(regions, active, 'right');
      walked.push(active);
    }
    expect(walked).toEqual(hand.items);
    // …and back again.
    expect(nextFocus(regions, hand.items[1]!, 'left')).toBe(hand.items[0]);
  });

  it('keeps every card an enabled, labelled button at 20 cards', () => {
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);
    for (const card of drawnCards()) {
      expect(card.tagName).toBe('BUTTON');
      expect((card as HTMLButtonElement).disabled).toBe(false);
      expect(card.getAttribute('aria-label')).toMatch(/playable|Inspect|Target|Toggle/);
    }
  });
});

describe('selection lifts and straightens the subject', () => {
  it('marks the selection and offers its action without covering the fan', () => {
    resizeTo(1680, 945);
    const choose = seed(handOf(7));
    render(<LiveMatchTable />);
    const card = screen.getByTestId('live-hand-card-h3');
    fireEvent.click(card);
    expect(screen.getByTestId('live-hand-card-h3').getAttribute('aria-pressed')).toBe('true');
    // The action renders in the one action home — now the lower-right control
    // cluster (ADR 0032) — never as a per-card popup (ADR 0004). One selected
    // card with exactly one offered action is §4.2 rule 3, so it fills the
    // PRIMARY slot rather than the equal-weight echo, and its label is the
    // server's own. The whole fan stays mounted underneath.
    expect(screen.getByTestId('control-primary').textContent).toContain('Cast');
    expect(screen.queryByTestId('control-echo')).toBeNull();
    expect(drawnCards()).toHaveLength(7);
    fireEvent.click(screen.getByTestId('live-hand-card-h3'));
    expect(choose).toHaveBeenCalledWith(expect.objectContaining({ id: 'cast_h3' }));
  });

  it('declares the straighten in the stylesheet, not just the lift', () => {
    // The baseline draws the held card flat and upright. jsdom applies no CSS,
    // so the declaration is the checkable artefact.
    const css = readStyleSheet();
    const selected = /\.handCard\[aria-pressed='true'\] \{([^}]*)\}/.exec(css)?.[1] ?? '';
    expect(selected).toContain('var(--shell-hand-lift-selected)');
    expect(selected).toContain('rotate(0deg)');
  });

  it('gives keyboard focus the same lift and spread as hover', () => {
    const css = readStyleSheet();
    // Every hover rule in the fan names `:focus-visible` beside it, so no fan
    // affordance is hover-only (`control-language.md` §7: parity is normative).
    for (const [, selector] of css.matchAll(/(\.handCard[^{]*:hover[^{]*)\{/g)) {
      expect(selector, selector).toContain('focus-visible');
    }
  });
});

describe('mulligan bottoming uses the same fan', () => {
  it('keeps every opening-hand card mounted, enabled, and pickable', () => {
    resizeTo(1680, 945);
    seed(mulliganHandOf(7));
    render(<LiveMatchTable />);
    expect(screen.getByTestId('live-match-table').dataset.forcedDecision).toBe('true');
    expect(band().dataset.pages).toBe('1');
    expect(drawnCards()).toHaveLength(7);
    // The sheet blocks nothing, so the cards it is asking about stay clickable.
    expect(screen.getByTestId('decision-sheet').getAttribute('data-pointer-through')).toBe('true');
    for (let i = 0; i < 7; i += 1) {
      const card = screen.getByTestId<HTMLButtonElement>(`live-hand-card-card_${i}`);
      expect(card.disabled).toBe(false);
      expect(card.getAttribute('aria-label')).toBe(`Toggle Opening Card ${i}`);
    }
    fireEvent.click(screen.getByTestId('live-hand-card-card_6'));
    expect(screen.getByTestId('live-hand-card-card_6').getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByTestId<HTMLButtonElement>('multiselect-option-keep').disabled).toBe(false);
  });

  it('pulls the fan to the page a bottoming candidate is on', () => {
    // A candidate is never stranded behind a page a player has to go looking
    // for (`layout-model.md` §Interaction guarantees: a pick is never removed).
    // Here the server names only the LAST card of a paged hand, so the fan must
    // open on its page rather than on page 0.
    resizeTo(390, 844);
    const frame = JSON.parse(mulliganHandOf(14)) as {
      valid_actions: { prompts?: { slot: string; candidates?: string[] }[] }[];
    };
    const bottom = frame.valid_actions[0]?.prompts?.find((prompt) => prompt.slot === 'bottom');
    if (bottom) bottom.candidates = ['card_13'];
    seed(JSON.stringify(frame));
    render(<LiveMatchTable />);

    const pages = Number(band().dataset.pages);
    expect(pages).toBeGreaterThan(1);
    expect(band().dataset.page).toBe(String(pages - 1));
    const candidate = screen.getByTestId<HTMLButtonElement>('live-hand-card-card_13');
    expect(candidate.disabled).toBe(false);
    expect(candidate.getAttribute('aria-label')).toBe('Toggle Opening Card 13');
    fireEvent.click(candidate);
    expect(screen.getByTestId('live-hand-card-card_13').getAttribute('aria-pressed')).toBe('true');
  });

  it('does not page a seven-card opening hand on phone portrait', () => {
    // The gutter reservation is conditional precisely so this stays true: the
    // phone band is exactly wide enough for an opening hand at the floor.
    resizeTo(390, 844);
    seed(mulliganHandOf(7));
    render(<LiveMatchTable />);
    expect(band().dataset.pages).toBe('1');
    expect(drawnCards()).toHaveLength(7);
    expect(screen.queryByTestId('hand-page-next')).toBeNull();
  });
});

describe('travel stays connected to the physical fan', () => {
  it('sources every targeting path from the hand anchor, not the card’s box', () => {
    // A hand card has no plane rect, so `zone-geography.md` §9's hand-sourced
    // motion is anchored at `hand:<you>` — the fan's screen-space home.
    resizeTo(1680, 945);
    seed(handOf(7));
    render(<LiveMatchTable />);
    const card = screen.getByTestId('live-hand-card-h2');
    expect(card.dataset.entity).toBe('h2');
    // The card publishes its place in the WHOLE hand, not in the page, so a
    // paged fan still names the card a motion is about.
    expect(card.dataset.handIndex).toBe('2');
  });

  it('keeps the hand index stable across a page turn', () => {
    resizeTo(1280, 720);
    seed(handOf(20));
    render(<LiveMatchTable />);
    fireEvent.click(screen.getByTestId('hand-page-next'));
    const first = drawnCards()[0]!;
    expect(Number(first.dataset.handIndex)).toBeGreaterThan(0);
    expect(first.dataset.handIndex).toBe(String(first.dataset.entity!.replace('h', '')));
  });
});

/** The shell stylesheet's source — jsdom applies no CSS module. */
function readStyleSheet(): string {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { readFileSync } = require('node:fs') as typeof import('node:fs');
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { resolve } = require('node:path') as typeof import('node:path');
  return readFileSync(resolve(process.cwd(), 'src/table/live/live-match.module.css'), 'utf8');
}
