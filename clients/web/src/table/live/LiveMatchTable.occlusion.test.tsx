/**
 * Non-occlusion regressions for the match shell (issue #528).
 *
 * The three failures this pins are the ones the issue names, each reproduced in
 * the shipped code before the fix:
 *
 * 1. **The mulligan could be covered.** The decision sheet sat at `z-index: 14`
 *    while the shell's top bar sat at `20`, so a top-anchored keep/mulligan
 *    prompt was painted over — and, because the bar is opaque and hit-testable,
 *    clicks aimed at the prompt landed on the bar. A covered mulligan is a
 *    soft-lock, which is what #451 fixed one layer down.
 * 2. **The hand was staged outside its band.** `bottom: -72px` inside an
 *    `overflow: hidden` band cut roughly half of every card off, and a raw
 *    `10%…90%` fan span clipped the first and last card on every band narrower
 *    than ~660 px — the tablet floor and phone portrait.
 * 3. **The compact composition overlapped hand and controls.** The hand spanned
 *    both columns underneath an opaque decisions panel, so at 390×844 most of a
 *    seven-card fan was behind the prompt asking about it.
 *
 * **jsdom computes no layout**, and Vitest does not process the CSS modules at
 * all. Nothing below proves a pixel. What it proves is the structure the paint
 * order depends on: which element is a sibling of which region (and therefore
 * which stacking context traps it), what geometry the shell publishes, and that
 * every hand card and every part of the decision are mounted, enabled, and
 * hit-testable at each supported viewport. The rendered result is the
 * maintainer's browser check.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MULLIGAN_GAME_VIEW_JSON, SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';
import {
  LAYER,
  SHELL,
  handCardBounds,
  handFanSpacing,
  shellBands,
  shellStyleVars,
} from './shellLayout';

/**
 * Read a stylesheet from the source tree. Vitest does not process CSS modules,
 * so a rule can only be asserted as text — which is enough for the two things
 * that matter here: which custom properties an offset is composed from, and
 * which rung it declares.
 */
function readStyleSheet(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

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

/** The viewports issue #528 requires, plus the envelope's desktop reference. */
const VIEWPORTS = [
  { label: '1280×720', width: 1280, height: 720 },
  { label: '1440×900', width: 1440, height: 900 },
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

/**
 * The mulligan frame with a full seven-card opening hand — the case acceptance
 * criterion 1 names. Built from the shipped two-card wire fixture so the prompt
 * shapes stay the canonical ones.
 */
function sevenCardMulliganJson(): string {
  const frame = JSON.parse(MULLIGAN_GAME_VIEW_JSON) as {
    my_hand: { id: string; name: string; type_line: string }[];
    valid_actions: { prompts?: { slot: string; candidates?: string[] }[] }[];
  };
  frame.my_hand = Array.from({ length: 7 }, (_, i) => ({
    id: `card_${i}`,
    name: `Opening Card ${i}`,
    type_line: 'Basic Land — Forest',
  }));
  const bottom = frame.valid_actions[0]?.prompts?.find((p) => p.slot === 'bottom');
  if (bottom) bottom.candidates = frame.my_hand.map((card) => card.id);
  return JSON.stringify(frame);
}

describe('match shell occlusion', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    resizeTo(originalWidth, originalHeight);
  });

  describe('the mulligan decision is never covered by fixed chrome', () => {
    for (const vp of VIEWPORTS) {
      it(`stages the whole keep/mulligan decision above the shell at ${vp.label}`, () => {
        resizeTo(vp.width, vp.height);
        seed(sevenCardMulliganJson());
        render(<LiveMatchTable />);

        const shell = screen.getByTestId('live-match-table');
        const area = screen.getByTestId('decision-area');

        // The area must be a SIBLING of the shell regions. Anything rendered
        // inside `.scene`, `.hand`, or `.cluster` is trapped in that region's
        // stacking context and can never outrank it, no matter what z-index it
        // declares. ADR 0032 removed the permanent regions but NOT this rule —
        // it is the whole reason contextual chrome is safe, and it is why #567
        // did not simply nest the decision inside the cluster it now sits over.
        expect(area.parentElement).toBe(shell);
        for (const region of ['hand', 'actions']) {
          const el = shell.querySelector(`[data-focus-region="${region}"]`);
          expect(el?.contains(area), `decision area nested inside ${region}`).not.toBe(true);
        }

        // The whole decision is present at once, on ONE surface: the question,
        // both named options, and the controls, beside the control cluster —
        // the relocated one action home (ADR 0032).
        expect(screen.getByTestId('decision-prompt').textContent).toContain('bottom');
        const keep = screen.getByTestId<HTMLButtonElement>('multiselect-option-keep');
        const mulligan = screen.getByTestId<HTMLButtonElement>('multiselect-option-mulligan');
        expect(area.contains(keep)).toBe(true);
        expect(mulligan.disabled).toBe(false);
        expect(keep.disabled).toBe(true); // owes exactly one bottomed card
        expect(screen.getByTestId('control-cluster')).toBeTruthy();

        // #567's first bullet: the question is drawn once. There is no strip
        // restating it and no control-less plaque beside it.
        expect(document.querySelectorAll('[data-decision-prompt]')).toHaveLength(1);
        expect(document.querySelectorAll('[data-testid="decision-area"]')).toHaveLength(1);
        expect(screen.queryByTestId('prompt-banner')).toBeNull();
        expect(screen.queryByTestId('decision-sheet')).toBeNull();
        expect(screen.queryByTestId('decision-plaque')).toBeNull();
      });
    }

    it('passes pointer events through to the cards the prompt is asking about', () => {
      // Issue #451's fix, re-pinned here because the layer it depends on moved
      // again: the area takes no pointer events outside its own plate, so the
      // on-canvas/hand picks stay clickable and nothing dims them either.
      resizeTo(390, 844);
      seed(sevenCardMulliganJson());
      render(<LiveMatchTable />);

      const area = screen.getByTestId('decision-area');
      expect(area.getAttribute('data-pointer-through')).toBe('true');

      fireEvent.click(screen.getByTestId('live-hand-card-card_3'));
      expect(screen.getByTestId('live-hand-card-card_3').getAttribute('aria-pressed')).toBe('true');
      expect(screen.getByTestId<HTMLButtonElement>('multiselect-option-keep').disabled).toBe(false);
    });

    it('clears the hand band at both compositions, in the geometry that ships', () => {
      // jsdom computes no layout, so this reads the CONTRACT the stylesheet is
      // written from: the decision area is placed from `--shell-cluster-h` and
      // `--shell-hand-h`, and at each composition the offsets it composes leave
      // the hand band untouched. Mulligan bottoming happens on those cards.
      const sheet = readStyleSheet('../decision/decision.module.css');
      const area = /\.area\s*\{([\s\S]*?)\}/.exec(sheet)?.[1] ?? '';
      const compact = /@media \(max-width: 899px\)[\s\S]*?\.area\s*\{([\s\S]*?)\}/.exec(sheet)?.[1];

      // Full composition: the cluster's own column, which `shellBands` already
      // subtracts from the hand band's width, offset above the cluster.
      expect(area).toContain('var(--rune-control-w-cluster)');
      expect(area).toContain('--shell-cluster-h');
      expect(area).not.toContain('--shell-hand-h');

      // Compact: the hand spans the full width above the cluster, so the area
      // has to clear the whole band, not just the cluster.
      expect(compact).toBeDefined();
      expect(compact).toContain('--shell-hand-h');
      expect(compact).toContain('--shell-cluster-h');

      // Both bands are published by `shellStyleVars`, so the numbers the offsets
      // add up are the numbers the shell ships.
      const published = shellStyleVars({ width: 390, height: 844 }) as Record<string, string>;
      expect(published['--shell-cluster-h']).toBeDefined();
      expect(published['--shell-hand-h']).toBeDefined();
    });

    it('never lets the decision rung be traded down to a chrome rung', () => {
      // I3: a decision the server is waiting on outranks every chrome region.
      const sheet = readStyleSheet('../decision/decision.module.css');
      expect(sheet).toContain('z-index: var(--rune-z-decision)');
      expect(LAYER.decision).toBeGreaterThan(LAYER.shellTop);
      expect(LAYER.decision).toBeGreaterThan(LAYER.shellRaised);
      expect(LAYER.decision).toBeGreaterThan(LAYER.shell);
    });

    it('keeps every hand card mounted and enabled while the decision is forced', () => {
      resizeTo(390, 844);
      seed(sevenCardMulliganJson());
      render(<LiveMatchTable />);

      expect(screen.getByTestId('live-match-table').dataset.forcedDecision).toBe('true');
      for (let i = 0; i < 7; i += 1) {
        const card = screen.getByTestId<HTMLButtonElement>(`live-hand-card-card_${i}`);
        expect(card.disabled).toBe(false);
      }
    });
  });

  describe('the hand is staged inside its own band', () => {
    for (const vp of VIEWPORTS) {
      it(`fans a seven-card hand entirely inside the band at ${vp.label}`, () => {
        resizeTo(vp.width, vp.height);
        seed(sevenCardMulliganJson());
        render(<LiveMatchTable />);

        const band = shellBands(vp).hand;
        const cards = Array.from({ length: 7 }, (_, i) =>
          screen.getByTestId<HTMLElement>(`live-hand-card-card_${i}`),
        );
        expect(cards).toHaveLength(7);

        for (const [index, card] of cards.entries()) {
          const raw = card.style.getPropertyValue('--hand-t');
          expect(raw, `card ${index} publishes a fan fraction`).not.toBe('');
          // The retired percentage span must not come back alongside it.
          expect(card.style.getPropertyValue('--hand-left')).toBe('');
          const t = Number(raw);
          expect(Number.isFinite(t), `card ${index} has a numeric fan fraction`).toBe(true);
          expect(t).toBeGreaterThanOrEqual(0);
          expect(t).toBeLessThanOrEqual(1);
          // The stylesheet turns `--hand-t` into a left inset by half a card;
          // this is that same arithmetic, so a clipped card fails here.
          const { left, right } = handCardBounds(t, band.w);
          expect(left, `card ${index} left edge`).toBeGreaterThanOrEqual(0);
          expect(right, `card ${index} right edge`).toBeLessThanOrEqual(band.w);
        }

        // The fan spans the band's full usable width, endpoints included.
        expect(cards[0]!.style.getPropertyValue('--hand-t')).toBe('0');
        expect(cards[6]!.style.getPropertyValue('--hand-t')).toBe('1');
        // Each card keeps a hit strip at or above the 44 px touch floor.
        expect(handFanSpacing(7, band.w)).toBeGreaterThanOrEqual(SHELL.minHit);
      });
    }

    it('centers a single-card hand instead of pinning it to an edge', () => {
      resizeTo(1280, 720);
      seed(SAMPLE_GAME_VIEW_JSON);
      render(<LiveMatchTable />);
      const cards = Array.from(
        document.querySelectorAll<HTMLElement>('[data-testid^="live-hand-card-"]'),
      );
      expect(cards.length).toBeGreaterThan(0);
      if (cards.length === 1) {
        expect(cards[0]!.style.getPropertyValue('--hand-t')).toBe('0.5');
      }
    });
  });

  describe('the shell publishes one geometry contract', () => {
    for (const vp of VIEWPORTS) {
      it(`applies the safe-area/track variables at ${vp.label}`, () => {
        resizeTo(vp.width, vp.height);
        seed(SAMPLE_GAME_VIEW_JSON);
        render(<LiveMatchTable />);

        const shell = screen.getByTestId('live-match-table');
        const expected = shellStyleVars(vp) as Record<string, string>;
        for (const [name, value] of Object.entries(expected)) {
          expect(shell.style.getPropertyValue(name), `${name} at ${vp.label}`).toBe(value);
        }
      });
    }

    it('switches composition at the one shared breakpoint', () => {
      // The composition flag, `shellLayout.isCompactShell`, and the stylesheet's
      // media query all move together; this pins the flag half.
      resizeTo(SHELL.compactBreakpoint - 1, 900);
      seed(SAMPLE_GAME_VIEW_JSON);
      const { unmount } = render(<LiveMatchTable />);
      expect(screen.getByTestId('live-match-table').dataset.composition).toBe('compact');
      // The cluster is the one action home at EVERY composition — it degrades
      // to the compact form (panel 6b) rather than collapsing away, so there is
      // no geometry at which the player loses their controls.
      expect(screen.getByTestId('control-cluster')).toBeTruthy();
      unmount();

      resizeTo(SHELL.compactBreakpoint, 900);
      render(<LiveMatchTable />);
      expect(screen.getByTestId('live-match-table').dataset.composition).toBe('full');
      expect(screen.getByTestId('control-cluster')).toBeTruthy();
    });

    it('claims battlefield width for the stack only while it is drawn', () => {
      // #534: "empty stack/log consumes no permanent battlefield width". The
      // canonical fixture carries a two-deep stack, so here the stage IS drawn
      // and the staging box pays for its column; the empty case is the geometry
      // half, pinned in `shellLayout.test.ts`.
      resizeTo(1440, 900);
      seed(SAMPLE_GAME_VIEW_JSON);
      render(<LiveMatchTable />);
      expect(screen.getByTestId('stack-stage')).toBeTruthy();

      const vp = { width: 1440, height: 900 };
      const drawn = shellBands(vp, {}, { stackPresent: true });
      const idle = shellBands(vp);
      expect(drawn.staging.w).toBeLessThan(idle.staging.w);
      expect(idle.staging.w).toBe(idle.viewport.w);
    });
  });
});
