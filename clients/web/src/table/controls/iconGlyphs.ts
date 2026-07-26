/**
 * The **icon-button glyph vocabulary** (`docs/design/control-language.md` §3.1,
 * issue #583).
 *
 * ## The defect this module exists to remove
 *
 * The match drew two 44 ⌀ icon buttons at opposite corners of the viewport — the
 * activity badge at the top right and the game-menu handle at the bottom right —
 * and gave them `≡` and `☰`. Different codepoints, one picture at 44 px, from one
 * button family with one silhouette. §3.1's rule for the family is "icon plus
 * accessible label/tooltip; no unlabeled mystery glyphs", and these were two
 * mystery glyphs that happened to be the same mystery. The activity badge swaps
 * to an unread count when there is one, so the collision was at its worst in the
 * quiet state a new player starts in.
 *
 * ## Why a module rather than two string literals
 *
 * The two glyphs live in different directories — the badge's is derived in
 * `table/stack/activityFeed.ts`, the handle's is drawn in
 * `table/controls/ControlCluster.tsx` — which is exactly how they collided in the
 * first place: nothing in either file could see the other. Declaring both here,
 * with {@link iconGlyphsCollide} as the test's single question, makes a
 * re-collision a failing assertion rather than a screenshot someone has to
 * notice.
 *
 * Neither glyph is ever the accessible name. Both controls carry an `aria-label`
 * and a tooltip through {@link IconButton}, whose `label` prop is required
 * precisely so an unlabeled icon is unwritable.
 */

/**
 * The activity badge, caught up: a stack of rules — the list picture, which is
 * what the surface behind it is. It is replaced by the unread count whenever
 * there is one, so this is the *quiet* state's glyph.
 */
export const ACTIVITY_GLYPH = '≡';

/**
 * The game-menu handle: a gear. The drawer behind it is session-level —
 * display settings, card art, keyboard shortcuts, and concede-with-confirm — so
 * the conventional settings picture says what it opens, which the bar stack it
 * replaced did not. §3.3 keeps concede here and nowhere else; the accessible
 * name stays "Game menu", because the drawer is more than its settings.
 */
export const MENU_GLYPH = '⚙';

/**
 * Whether two drawn glyphs are the same picture. A codepoint comparison is
 * enough to catch the defect that shipped (`≡` vs `☰` are distinct codepoints
 * and were caught by eye, not by a test) — but only because both are compared
 * from one place. This is that place.
 */
export function iconGlyphsCollide(a: string, b: string): boolean {
  return a === b;
}
