/**
 * The control geometry mirror's drift guard (issue #534).
 *
 * `controlTokens.ts` restates lengths that `chrome/tokens.css` already declares,
 * because layout code needs them as numbers it can add up. That duplication is
 * only safe while something fails when the two disagree — this is that
 * something, and it is the same discipline `shellLayout.test.ts` applies to the
 * `--rune-z-*` ladder.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  CONTROL,
  CONTROL_TOKEN_NAMES,
  MENU_RUNG,
  MENU_RUNG_TOKEN_NAMES,
  PIP_COUNT,
  menuRung,
  pipRowWidth,
} from './controlTokens';

/**
 * Read a stylesheet from the client source tree — the same helper shape
 * `shellLayout.test.ts` uses. It has to be a function: the module-level
 * `import.meta.url` is not a `file:` URL under Vitest's SSR transform, so
 * resolving the path eagerly throws before any test runs.
 */
function css(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

describe('control geometry mirror', () => {
  const tokens = css('../../chrome/tokens.css');

  it('agrees with every token it mirrors in chrome/tokens.css', () => {
    for (const [key, token] of Object.entries(CONTROL_TOKEN_NAMES)) {
      const match = new RegExp(`${token}:\\s*(\\d+)px;`).exec(tokens);
      expect(match, `${token} is not declared as a px length in chrome/tokens.css`).not.toBeNull();
      expect(Number(match![1]), `${token} drifted from CONTROL.${key}`).toBe(
        CONTROL[key as keyof typeof CONTROL],
      );
    }
  });

  it('mirrors every geometry token the stylesheet declares', () => {
    // The other direction: a token added to the §2.2 block without a mirror here
    // is a number layout code will re-type as a literal. The colour half of §2.1
    // is deliberately NOT mirrored (see `controlTokens.ts`), so only the lengths
    // named by the spec's geometry table are required to appear.
    const geometry = [
      '--rune-control-h-primary',
      '--rune-control-h',
      '--rune-control-hit',
      '--rune-control-w-cluster',
      '--rune-control-w-pair',
      '--rune-control-chamfer',
      '--rune-plaque-h',
      '--rune-plaque-point',
      '--rune-control-frame-w',
      '--rune-primary-frame-w',
      '--rune-pip-size',
      '--rune-pip-pitch',
    ];
    const mirrored = new Set(Object.values(CONTROL_TOKEN_NAMES));
    for (const token of geometry) {
      expect(mirrored.has(token), `${token} is declared but not mirrored in CONTROL`).toBe(true);
    }
  });

  it('keeps every drawn plate reachable at the 44px floor (D2)', () => {
    // The plates are drawn BELOW the floor on purpose — 36px is the baseline's
    // proportion — and the hit box is what has to clear it. If someone "fixes"
    // this by growing the plates instead, the cluster's vertical rhythm changes
    // and panel 7's proportions shift, which is exactly the trade C4 records.
    expect(CONTROL.h).toBeLessThan(CONTROL.hit);
    expect(CONTROL.hit).toBe(44);
    expect(CONTROL.hPrimary).toBeGreaterThanOrEqual(CONTROL.hit);
  });

  it('fits the five phase-group pips inside the plaque', () => {
    // D3 renders the five shipped PHASE_GROUPS, not the 4 and 3 pips the two
    // baseline panels happen to draw — those counts are illustrative of the
    // form, not a model of the turn.
    expect(pipRowWidth()).toBe(CONTROL.pipSize + (PIP_COUNT - 1) * CONTROL.pipPitch);
    expect(pipRowWidth()).toBeLessThan(CONTROL.wCluster);
  });

  it('has no pip row at all for an empty group list', () => {
    expect(pipRowWidth(0)).toBe(0);
  });
});

/**
 * The menu rung (§3.4, issue #566). The rung is the pregame's answer to "the
 * controls are too small to read as the primary thing on the screen", so what
 * has to hold is that it is a *multiplier on §3.1* rather than a second size
 * vocabulary, that it never draws a control smaller than the match does, and
 * that the stylesheet and the mirror cannot drift apart.
 */
describe('the menu rung', () => {
  const tokens = css('../../chrome/tokens.css');

  it('agrees with the clamp declared in chrome/tokens.css', () => {
    for (const [name, token] of Object.entries(MENU_RUNG_TOKEN_NAMES)) {
      const declared = new RegExp(
        `${token}:\\s*clamp\\(([\\d.]+),\\s*calc\\(100vmin\\s*/\\s*(\\d+)\\),\\s*([\\d.]+)\\);`,
      ).exec(tokens);
      expect(declared, `${token} is not declared as a 100vmin clamp`).not.toBeNull();
      const rung = MENU_RUNG[name as keyof typeof MENU_RUNG];
      expect(
        [Number(declared![1]), Number(declared![2]), Number(declared![3])],
        `${token} drifted from MENU_RUNG.${name}`,
      ).toEqual([rung.min, rung.basis, rung.max]);
    }
  });

  it('never draws a menu control smaller than the match control (the floor is 1)', () => {
    // The whole rung is a growth term. A rung below 1 would put a pregame
    // control under the 44px floor the icon button pins D1's scale anchor at.
    for (const rung of Object.values(MENU_RUNG)) expect(rung.min).toBe(1);
    for (const name of Object.keys(MENU_RUNG) as (keyof typeof MENU_RUNG)[]) {
      // Phone portrait, the smallest supported geometry: no growth at all.
      expect(menuRung(name, { width: 390, height: 844 })).toBe(1);
      // …and every control is therefore still at least the drawn plate.
      expect(CONTROL.hit * menuRung(name, { width: 390, height: 844 })).toBeGreaterThanOrEqual(44);
    }
  });

  it('grows the open rung at the desktop references and caps it on a 4K panel', () => {
    // The reference envelope of `presentation-budgets.md` §Device and browser
    // envelope, which is what "reads as the primary control at the supported
    // viewport envelope" is measured against.
    expect(menuRung('open', { width: 1280, height: 800 })).toBeCloseTo(1.29, 2);
    expect(menuRung('open', { width: 1180, height: 820 })).toBeCloseTo(1.32, 2);
    expect(menuRung('open', { width: 1440, height: 900 })).toBeCloseTo(1.45, 2);
    expect(menuRung('open', { width: 3840, height: 2160 })).toBe(MENU_RUNG.open.max);
    // The primary pill at the 1440×900 reference: 268×56 becomes ~389×81 with a
    // ~35px label. That is the "one action on the screen" the issue asks for.
    expect(CONTROL.wCluster * menuRung('open', { width: 1440, height: 900 })).toBeGreaterThan(380);
  });

  it('keeps the dense rung under the open one at every geometry', () => {
    // The ready room's ring only reads while its height clears half a seat plus
    // half the centre plaque, and a seat scales with the rung. The dense rung is
    // the one that keeps that inequality true at the 900px-tall reference.
    for (const size of [
      { width: 1280, height: 800 },
      { width: 1440, height: 900 },
      { width: 1920, height: 1080 },
      { width: 3840, height: 2160 },
    ]) {
      expect(menuRung('dense', size)).toBeLessThan(menuRung('open', size));
    }
    expect(menuRung('dense', { width: 1440, height: 900 })).toBe(1);
  });

  it('spends the rung on an ultrawide as gutter, not as bigger controls', () => {
    // 21:9 at the same height gets the same rung as 16:9: `vmin`, never `vw`.
    expect(menuRung('open', { width: 3440, height: 1440 })).toBe(
      menuRung('open', { width: 2560, height: 1440 }),
    );
  });
});

/**
 * The rung reaches a control through exactly one property, and the control
 * family is the only thing that spends it. These parse the stylesheets rather
 * than the DOM, because jsdom computes no layout and `var()` arithmetic never
 * resolves there.
 */
describe('the control family at the rung', () => {
  const controls = css('./controls.module.css');
  const tokens = css('../../chrome/tokens.css');

  it('multiplies every plate length and label size by --rune-control-scale', () => {
    // Each of these is a §3.1 value; none of them may reach the stylesheet
    // unmultiplied, or a menu control would draw at match scale beside one that
    // did not.
    for (const token of [
      '--rune-control-hit',
      '--rune-control-h',
      '--rune-control-h-primary',
      '--rune-control-w-cluster',
      '--rune-control-w-pair',
      '--rune-type-action',
      '--rune-type-body-lg',
      '--rune-type-heading',
      '--rune-type-title',
    ]) {
      for (const use of controls.matchAll(new RegExp(`var\\(${token}\\)[^;]*`, 'g'))) {
        expect(use[0], `${token} is used without the rung in controls.module.css`).toContain(
          'var(--rune-control-scale)',
        );
      }
    }
  });

  it('leaves the frame trim off the rung (issue #571 keeps its exact offset)', () => {
    // `--rune-control-chamfer-face` is the frame outline offset inward by the
    // stroke — an exact derivation that only holds at the drawn values. Scaling
    // one of the pair and not the other is how the trim went uneven the first
    // time.
    for (const token of ['--rune-control-chamfer', '--rune-control-frame-w']) {
      for (const use of controls.matchAll(new RegExp(`var\\(${token}\\)[^;]*`, 'g'))) {
        expect(use[0], `${token} is trim and must not scale`).not.toContain(
          'var(--rune-control-scale)',
        );
      }
    }
  });

  it('is unscaled by default, so the match is untouched', () => {
    expect(tokens).toContain('--rune-control-scale: 1;');
  });
});
