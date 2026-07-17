/**
 * The bundled OFL display face (issue #322).
 *
 * Guards the asset + wiring so the identity face cannot silently regress: the WOFF2
 * ships, its OFL license travels with it, `@font-face` registers the bundled family,
 * and `--rune-font-display` leads with it while keeping the system-stack fallback.
 * No test asserts the *rendered* font (that would be environment-dependent and is
 * explicitly out of scope) — only that the asset and its registration are present.
 */
import { readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const here = resolve(process.cwd(), 'src/chrome');

describe('bundled display face (issue #322)', () => {
  it('ships the WOFF2 asset, and it is small (subset, not the full face)', () => {
    const size = statSync(resolve(here, 'fonts/rune-display.woff2')).size;
    expect(size).toBeGreaterThan(2_000); // a real font, not an empty file
    expect(size).toBeLessThan(60_000); // subset, not the ~390 KB raw TTF
  });

  it('commits the OFL license text alongside the asset', () => {
    const ofl = readFileSync(resolve(here, 'fonts/OFL.txt'), 'utf8');
    expect(ofl).toContain('SIL Open Font License');
  });

  it('registers the bundled family via @font-face with a swap fallback', () => {
    const css = readFileSync(resolve(here, 'tokens.css'), 'utf8');
    expect(css).toContain('@font-face');
    expect(css).toContain("font-family: 'RUNE Display'");
    expect(css).toContain('rune-display.woff2');
    expect(css).toContain('font-display: swap');
  });

  it('points --rune-font-display at the bundled face, keeping the system fallback', () => {
    const css = readFileSync(resolve(here, 'tokens.css'), 'utf8');
    const decl = css.split('--rune-font-display:')[1]?.split(';')[0] ?? '';
    expect(decl).toContain("'RUNE Display'");
    // The former geometric system stack stays as the fallback.
    expect(decl).toContain('Futura');
    expect(decl).toContain('sans-serif');
  });
});
