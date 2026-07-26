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
  PIP_COUNT,
  menuRung,
  pipRowWidth,
} from './controlTokens';

/**
 * The §3.1 values a rung restates, keyed by the suffix the `--rune-menu-*` set
 * uses. The type ramp is not in {@link CONTROL} (it is not control geometry),
 * so those baselines are stated here against `chrome/tokens.css`'s own scale.
 */
const SCALED_BY_RUNG: Record<string, number> = {
  'control-h-primary': CONTROL.hPrimary,
  'control-h': CONTROL.h,
  'control-hit': CONTROL.hit,
  'control-w-cluster': CONTROL.wCluster,
  'control-w-pair': CONTROL.wPair,
  'cluster-margin': CONTROL.clusterMargin,
  'cluster-gap': CONTROL.clusterGap,
  touch: 44,
  'type-display': 30,
  'type-title': 20,
  'type-heading': 16,
  'type-body-lg': 14,
  'type-body': 13,
  'type-caption': 12,
  'type-micro': 11,
  'type-action': 24,
};

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
 * has to hold is that it is a *restatement of §3.1* rather than a second size
 * vocabulary, that it never draws a control smaller than the match does, and
 * that the stylesheet and the mirror cannot drift apart.
 */
describe('the menu rung', () => {
  const tokens = css('../../chrome/tokens.css');

  it('declares a fluid restatement of every §3.1 token it scales', () => {
    // A rung is `clamp(§3.1 value, base / basis × 100vmin, base × cap)`, one
    // entry per token, and the numbers are recomputed here from `MENU_RUNG` so
    // the stylesheet cannot drift from the mirror.
    //
    // It is a clamp of LENGTHS rather than one unitless multiplier because a
    // multiplier cannot be derived from the viewport: `calc(100vmin / 620)` is
    // a length, so `clamp(1, <length>, 1.6)` is invalid and every property
    // multiplying by it is dropped. `pregame/menuRung.test.ts` resolves the
    // whole chain; this is the mirror's own half.
    for (const [name, rung] of Object.entries(MENU_RUNG)) {
      const prefix = name === 'dense' ? 'dense-' : '';
      for (const [token, base] of Object.entries(SCALED_BY_RUNG)) {
        const declared = new RegExp(
          `--rune-menu-${prefix}${token}:\\s*clamp\\((\\d+)px,\\s*([\\d.]+)vmin,\\s*([\\d.]+)px\\);`,
        ).exec(tokens);
        expect(declared, `--rune-menu-${prefix}${token} is not a clamp of lengths`).not.toBeNull();
        expect(Number(declared![1]), `--rune-menu-${prefix}${token} floor`).toBe(base);
        expect(Number(declared![2]), `--rune-menu-${prefix}${token} fluid term`).toBeCloseTo(
          (base / rung.basis) * 100,
          3,
        );
        expect(Number(declared![3]), `--rune-menu-${prefix}${token} ceiling`).toBeCloseTo(
          base * rung.max,
          3,
        );
      }
    }
  });

  it('leaves the frame trim off every rung (issue #571 keeps its exact offset)', () => {
    // `--rune-control-chamfer-face` is the frame outline offset inward by the
    // stroke — an exact derivation that only holds at the drawn values. Scaling
    // one of the pair and not the other is how the trim went uneven the first
    // time, so neither may appear in a rung set at all.
    for (const trim of ['control-chamfer', 'control-frame-w', 'primary-frame-w']) {
      expect(tokens, `${trim} must not be scaled by a rung`).not.toContain(`--rune-menu-${trim}:`);
      expect(tokens).not.toContain(`--rune-menu-dense-${trim}:`);
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
 * The control family does not know the rung exists, and that is the mechanism.
 * A menu surface re-points the §3.1 tokens (`pregame/pregame.module.css`), so
 * every rule here keeps reading exactly one token and a control on a menu is
 * the same component as the control in a match, drawn larger. These parse the
 * stylesheet rather than the DOM, because jsdom computes no layout and `var()`
 * arithmetic never resolves there.
 */
describe('the control family reads one token per length', () => {
  const controls = css('./controls.module.css');
  const tokens = css('../../chrome/tokens.css');

  it('never multiplies a §3.1 token by anything', () => {
    // The first cut multiplied every plate by a `--rune-control-scale` that was
    // invalid at computed-value time, which dropped the declarations outright.
    // There is no multiplier now, and there is no token to reintroduce one.
    expect(controls).not.toContain('--rune-control-scale');
    expect(tokens).not.toContain('--rune-control-scale');
    for (const token of [
      '--rune-control-hit',
      '--rune-control-h',
      '--rune-control-h-primary',
      '--rune-control-w-cluster',
      '--rune-control-w-pair',
      '--rune-type-action',
      '--rune-type-body-lg',
    ]) {
      // The token itself is never a factor. (`.primary .face` does subtract the
      // frame stroke, which is arithmetic ON the plate and not a scale OF it.)
      expect(
        new RegExp(`var\\(${token}\\)\\s*\\*`).test(controls),
        `${token} is multiplied in controls.module.css`,
      ).toBe(false);
    }
  });

  it('sizes every variant from a token a rung can re-point', () => {
    // The other half: a plate that hard-codes a length cannot follow a rung, so
    // a menu would draw it at match size beside controls that grew.
    for (const [selector, token] of [
      ['.button', '--rune-control-hit'],
      ['.face', '--rune-control-h'],
      ['.primary', '--rune-control-w-cluster'],
      ['.primaryCompact', '--rune-control-w-pair'],
      ['.icon', '--rune-control-hit'],
    ] as [string, string][]) {
      const at = controls.indexOf(`${selector} {`);
      expect(at, `${selector} has no rule`).toBeGreaterThanOrEqual(0);
      const block = /\{([^}]*)\}/.exec(controls.slice(at))?.[1] ?? '';
      expect(block, `${selector} does not size from ${token}`).toContain(`var(${token})`);
    }
  });
});
