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
import { CONTROL, CONTROL_TOKEN_NAMES, PIP_COUNT, pipRowWidth } from './controlTokens';

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
