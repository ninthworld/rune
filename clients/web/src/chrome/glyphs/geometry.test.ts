/**
 * Glyph vocabulary invariants (issue #317).
 *
 * The load-bearing test is {@link describe} "keyword coverage": it reads the engine's
 * card catalog directly and asserts the glyph set covers exactly the keywords the
 * catalog ships — so adding a catalog keyword without a glyph fails CI here rather
 * than rendering an empty gap on a card face (an acceptance criterion of #317).
 */
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { GLYPHS, GLYPH_NAMES, GLYPH_VIEWBOX, keywordGlyphName, type GlyphName } from './geometry';

/**
 * The engine card catalog — the single source of truth for shipped keywords.
 * Resolved from the package root (vitest's cwd is `clients/web`, both for
 * `npm test` and `make client-check`), which reaches the workspace crates.
 */
const CATALOG_DIR = resolve(process.cwd(), '../../crates/rune-engine/data/catalog');

/** Every distinct keyword that appears on any card in the shipped catalog. */
function catalogKeywords(): Set<string> {
  const keywords = new Set<string>();
  for (const file of readdirSync(CATALOG_DIR)) {
    if (!file.endsWith('.json')) continue;
    const card = JSON.parse(readFileSync(`${CATALOG_DIR}/${file}`, 'utf8')) as {
      keywords?: string[];
    };
    for (const kw of card.keywords ?? []) keywords.add(kw);
  }
  return keywords;
}

/** The keyword names the glyph vocabulary covers (the `kw-<wire>` glyphs). */
function glyphKeywords(): Set<string> {
  return new Set(GLYPH_NAMES.filter((n) => n.startsWith('kw-')).map((n) => n.slice('kw-'.length)));
}

describe('keyword coverage', () => {
  it('the catalog ships a non-empty keyword set', () => {
    // Guards the test itself: if the catalog path breaks, this fails loudly rather
    // than the coverage assertions vacuously passing over an empty set.
    expect(catalogKeywords().size).toBeGreaterThan(0);
  });

  it('every keyword on a shipped card has a glyph — no empty gaps', () => {
    for (const kw of catalogKeywords()) {
      expect(keywordGlyphName(kw), `missing glyph for catalog keyword "${kw}"`).not.toBeNull();
    }
  });

  it('the glyph vocabulary mirrors the engine keyword set and covers the catalog', () => {
    // The glyph vocabulary mirrors the engine's closed keyword set (ability.rs /
    // card.rs `Keyword`), which is a superset of whatever keywords the current
    // catalog happens to ship — the M19 catalog (ADR 0026) uses only a subset, so
    // some glyphs (e.g. first_strike, deathtouch) are valid engine keywords with no
    // current card. The two invariants: the glyph set is exactly the nine engine
    // keywords, and every keyword a shipped card uses has a glyph (no gaps).
    expect([...glyphKeywords()].sort()).toEqual([
      'deathtouch',
      'double_strike',
      'first_strike',
      'flying',
      'haste',
      'lifelink',
      'reach',
      'trample',
      'vigilance',
    ]);
    for (const kw of catalogKeywords()) {
      expect(glyphKeywords().has(kw), `catalog keyword "${kw}" has no glyph`).toBe(true);
    }
  });

  it('maps a keyword wire name to its `kw-` glyph', () => {
    expect(keywordGlyphName('flying')).toBe('kw-flying');
    expect(keywordGlyphName('first_strike')).toBe('kw-first_strike');
    expect(keywordGlyphName('nonexistent_keyword')).toBeNull();
  });
});

describe('glyph geometry', () => {
  it('covers the whole vocabulary from one source', () => {
    // Basic-land, zone, phase, keyword, state, and seat/ready families are all present.
    const expected: GlyphName[] = [
      'land-plains',
      'land-island',
      'land-swamp',
      'land-mountain',
      'land-forest',
      'zone-library',
      'zone-graveyard',
      'zone-exile',
      'phase-beginning',
      'phase-main',
      'phase-combat',
      'phase-ending',
      'kw-flying',
      'kw-reach',
      'kw-vigilance',
      'kw-haste',
      'kw-first_strike',
      'kw-trample',
      'kw-deathtouch',
      'kw-lifelink',
      'kw-double_strike',
      'tap',
      'ready',
      'seat',
    ];
    for (const name of expected) expect(GLYPH_NAMES).toContain(name);
  });

  it('every glyph has geometry and an accessible title', () => {
    for (const name of GLYPH_NAMES) {
      const def = GLYPHS[name];
      expect(def.title.length, `${name} needs a title`).toBeGreaterThan(0);
      expect(def.elements.length, `${name} needs geometry`).toBeGreaterThan(0);
    }
  });

  it('all geometry stays inside the shared 0..24 box', () => {
    // Off-box coordinates would clip at chip scale; keep every mark inside the frame.
    const inBox = (v: number) => v >= 0 && v <= GLYPH_VIEWBOX;
    for (const name of GLYPH_NAMES) {
      for (const el of GLYPHS[name].elements) {
        if (el.kind === 'circle') {
          expect(inBox(el.cx - el.r) && inBox(el.cx + el.r), `${name} circle x`).toBe(true);
          expect(inBox(el.cy - el.r) && inBox(el.cy + el.r), `${name} circle y`).toBe(true);
        } else {
          for (const [x, y] of el.points) {
            expect(inBox(x) && inBox(y), `${name} point ${x},${y}`).toBe(true);
          }
        }
      }
    }
  });

  it('renders distinct geometry per glyph (no accidental duplicates)', () => {
    const shapes = GLYPH_NAMES.map((n) => JSON.stringify(GLYPHS[n].elements));
    expect(new Set(shapes).size).toBe(GLYPH_NAMES.length);
  });
});
