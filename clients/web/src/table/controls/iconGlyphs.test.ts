/**
 * The icon-button glyph guard (issue #583).
 *
 * The defect was not a wrong glyph — it was two controls that had no way to know
 * about each other. The activity badge's picture is derived in
 * `table/stack/activityFeed.ts` and the game-menu handle's is drawn in
 * `table/controls/ControlCluster.tsx`, and both are 44 ⌀ circles from a family
 * whose whole premise is that a control's rank reads off its silhouette. Two
 * files, one silhouette, and nothing that could fail when they picked the same
 * picture.
 *
 * These tests are that missing link: both drawn glyphs come from `iconGlyphs.ts`
 * and both are compared here. A re-collision is a red test rather than something
 * a reviewer has to spot in a screenshot.
 *
 * jsdom draws no glyph and measures no font, so nothing here claims the two are
 * *legible* at 44 px in both environment themes — that is the maintainer's
 * browser check, as `control-language.md` §3.1 says.
 */
import { describe, expect, it } from 'vitest';
import type { GameLogEntry, GameView } from '../../protocol';
import { deriveActivity } from '../stack/activityFeed';
import { ACTIVITY_GLYPH, MENU_GLYPH, iconGlyphsCollide } from './iconGlyphs';

describe('icon-button glyphs', () => {
  it('never gives the two match icon buttons the same picture', () => {
    expect(iconGlyphsCollide(ACTIVITY_GLYPH, MENU_GLYPH)).toBe(false);
  });

  it('gives the activity badge the declared quiet glyph and the count when unread', () => {
    const log: GameLogEntry[] = [
      {
        sequence: 1,
        event: { type: 'spell_cast', player: 'p2', card: { id: 'c1', name: 'Shock' } },
      },
    ];
    const view: Pick<GameView, 'log' | 'player_names'> = {
      log,
      player_names: { p1: 'Imogen', p2: 'Sorel' },
    };

    expect(deriveActivity(view).badgeText).toBe(ACTIVITY_GLYPH);
    // The unread count replaces the glyph, which is why the collision was at its
    // worst in the quiet state a new player starts in — the one state where the
    // badge falls back to a bare mark.
    expect(deriveActivity(view, { unreadCount: 3 }).badgeText).toBe('3');
  });

  it('states the collision test rather than leaving it to a reader', () => {
    // The function exists so the comparison lives in one place, not so it can be
    // clever. Two different strings differ; one string collides with itself.
    expect(iconGlyphsCollide('☰', '☰')).toBe(true);
    expect(iconGlyphsCollide('☰', '≡')).toBe(false);
  });
});
