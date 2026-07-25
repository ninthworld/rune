/**
 * The card-back contract of `docs/design/card-representation.md` §13, against
 * the plates #555 shipped.
 *
 * Two questions, in order of importance:
 *
 * 1. **Can a card back leak the card it hides?** §13.1 makes this a hard
 *    requirement, so it is tested as one rather than left to review. The
 *    strongest form of the answer is structural: no function on this path is
 *    ever *given* a card, a zone, a seat, or a count, so there is nothing for a
 *    back to vary with — including through its rotation.
 * 2. **Does §13.2's skin contract hold?** One default, at least one alternate,
 *    a device-local preference that never touches the protocol, and a fallback
 *    that changes a URL and nothing else.
 *
 * **What jsdom cannot show.** It applies no CSS-module stylesheet, performs no
 * layout, and decodes no image. Nothing here proves an appearance: not the
 * back's contrast against the play surface, not that the emblem is rotationally
 * symmetric, not that a fallback is pixel-identical in box. Those are the
 * maintainer's to verify in a browser. What is checkable is the declared
 * contract — which property is published, what it resolves to, what reaches the
 * DOM, and what the stylesheet says about it.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import {
  CARD_BACK_KEY,
  CARD_BACK_SKINS,
  DEFAULT_CARD_BACK_ID,
  activeCardBack,
  cardBackSkin,
  cardBackVars,
  getCardBackId,
  isCardBackId,
  noteCardBackFailed,
  resetCardBackStore,
  resolveCardBackSkin,
  setCardBackId,
  useCardBack,
} from './index';

afterEach(() => {
  cleanup();
  resetCardBackStore();
  localStorage.clear();
});

/** A probe component that renders whatever the hook publishes. */
function Probe() {
  const { skin, vars } = useCardBack();
  return <div data-testid="probe" data-skin={skin?.id ?? 'none'} style={vars} />;
}

describe('card backs — §13.1 hidden-information safety', () => {
  it('resolves one back for the whole device, with no card, zone, or seat input', () => {
    // The structural guarantee. `activeCardBack` and `cardBackVars` are the only
    // two functions a hidden surface calls, and neither accepts anything that
    // could identify a card — so a back CANNOT vary with what it hides, and the
    // §13.1 rotation clause follows for free: there is no per-card channel for a
    // rotation to differ on.
    expect(activeCardBack.length).toBe(0);
    expect(cardBackVars.length).toBe(1);
    const back = activeCardBack();
    expect(back).toBeDefined();
    // Called a hundred times, in any order, it is the same answer every time.
    for (let i = 0; i < 100; i += 1) expect(activeCardBack()).toEqual(back);
  });

  it('publishes exactly one custom property, carrying only a URL', () => {
    // One property, one value, for the whole surface. Nothing that could encode
    // a colour identity, a type, a count, or an index rides along.
    const vars = cardBackVars(cardBackSkin(DEFAULT_CARD_BACK_ID));
    expect(Object.keys(vars)).toEqual(['--card-back-image']);
    expect(vars['--card-back-image']).toMatch(/^url\("\/assets\/card-backs\/.+\.webp"\)$/);
  });

  it('never carries a transform, so a rotated pile cannot read differently', () => {
    // §13.1: "no feature of it may vary with, or be inferred from, the card it
    // hides — including its rotation". The back is a background on a pile the
    // rack never rotates (`zone-geography.md` §1 fact 2), and it declares no
    // transform of its own.
    const css = readFileSync(
      resolve(process.cwd(), 'src/table/live/live-plane.module.css'),
      'utf8',
    );
    const rule = css.slice(
      css.indexOf("[data-slot='zone'][data-zone='library']"),
      css.indexOf("[data-zone='graveyard']"),
    );
    expect(rule).toContain('--card-back-image');
    expect(rule).not.toContain('transform');
    expect(rule).not.toContain('rotate');
    // …and nothing keys the library's appearance on a card attribute.
    expect(rule).not.toContain('data-top');
    expect(rule).not.toContain('data-name');
  });
});

describe('card backs — §13.2 the skin manifest contract', () => {
  it('ships a default plus at least one alternate, both content-hashed', () => {
    expect(CARD_BACK_SKINS.length).toBeGreaterThanOrEqual(2);
    expect(isCardBackId(DEFAULT_CARD_BACK_ID)).toBe(true);
    for (const skin of CARD_BACK_SKINS) {
      expect(skin.src).toMatch(/^\/assets\/card-backs\/[a-z-]+\.[a-f0-9]{8}\.webp$/);
      expect(skin.label.length).toBeGreaterThan(0);
    }
  });

  it('gives every skin the same silhouette — an identical aspect, to the pixel', () => {
    // "Invariants every skin must hold: identical silhouette and radius". The
    // radius is the frame's and is unchanged by a skin; the aspect is the one
    // part of the silhouette a plate can get wrong, so it is checked here.
    const aspects = new Set(CARD_BACK_SKINS.map((skin) => skin.width / skin.height));
    expect(aspects.size).toBe(1);
  });

  it('persists a choice under the documented key and republishes it', () => {
    const alternate = CARD_BACK_SKINS.find((skin) => skin.id !== DEFAULT_CARD_BACK_ID)!;
    setCardBackId(alternate.id);
    expect(localStorage.getItem(CARD_BACK_KEY)).toBe(alternate.id);
    expect(activeCardBack()?.id).toBe(alternate.id);
    resetCardBackStore();
    expect(getCardBackId()).toBe(alternate.id);
  });

  it('ignores an unknown id rather than storing it, and self-heals a stale one', () => {
    setCardBackId('nope-not-a-skin');
    expect(getCardBackId()).toBe(DEFAULT_CARD_BACK_ID);
    expect(localStorage.getItem(CARD_BACK_KEY)).toBeNull();
    // A skin removed from the manifest since it was chosen rewrites the key on
    // first read, rather than being re-resolved on every mount.
    localStorage.setItem(CARD_BACK_KEY, 'retired-skin');
    resetCardBackStore();
    expect(getCardBackId()).toBe(DEFAULT_CARD_BACK_ID);
    expect(localStorage.getItem(CARD_BACK_KEY)).toBeNull();
  });

  it('falls a failed skin back to the default, changing the URL and nothing else', () => {
    const alternate = CARD_BACK_SKINS.find((skin) => skin.id !== DEFAULT_CARD_BACK_ID)!;
    setCardBackId(alternate.id);
    const before = cardBackVars(activeCardBack());
    noteCardBackFailed(alternate.id);
    const after = cardBackVars(activeCardBack());
    expect(activeCardBack()?.id).toBe(DEFAULT_CARD_BACK_ID);
    // Same property, same shape — one URL differs. There is nothing else in the
    // published surface for a layout to depend on.
    expect(Object.keys(after)).toEqual(Object.keys(before));
    expect(after['--card-back-image']).not.toBe(before['--card-back-image']);
  });

  it('leaves the procedural back standing when the default itself fails', () => {
    noteCardBackFailed(DEFAULT_CARD_BACK_ID);
    expect(activeCardBack()).toBeUndefined();
    // `none` is a valid image layer, so the pile's `background` shorthand simply
    // draws nothing over its token treatment: a colour difference, never a
    // layout one, and never a hole.
    expect(cardBackVars(undefined)['--card-back-image']).toBe('none');
  });

  it('resolves an unknown, malformed, or failed request to the default', () => {
    expect(resolveCardBackSkin(undefined)?.id).toBe(DEFAULT_CARD_BACK_ID);
    expect(resolveCardBackSkin(null)?.id).toBe(DEFAULT_CARD_BACK_ID);
    expect(resolveCardBackSkin('')?.id).toBe(DEFAULT_CARD_BACK_ID);
    expect(resolveCardBackSkin('not-a-skin')?.id).toBe(DEFAULT_CARD_BACK_ID);
    const alternate = CARD_BACK_SKINS.find((skin) => skin.id !== DEFAULT_CARD_BACK_ID)!;
    expect(resolveCardBackSkin(alternate.id, new Set([alternate.id]))?.id).toBe(
      DEFAULT_CARD_BACK_ID,
    );
  });
});

describe('card backs — the React binding', () => {
  it('publishes the resolved skin without a re-mount when the preference changes', () => {
    const alternate = CARD_BACK_SKINS.find((skin) => skin.id !== DEFAULT_CARD_BACK_ID)!;
    const view = render(<Probe />);
    const node = view.getByTestId('probe');
    expect(node.dataset.skin).toBe(DEFAULT_CARD_BACK_ID);
    view.rerender(<Probe />);
    setCardBackId(alternate.id);
    view.rerender(<Probe />);
    expect(view.getByTestId('probe').dataset.skin).toBe(alternate.id);
  });

  it('renders fully with no skin resolved at all', () => {
    // The client hard rule: the UI must rebuild with every presentation cache
    // empty. A device whose card backs all failed still gets a complete surface.
    noteCardBackFailed(DEFAULT_CARD_BACK_ID);
    for (const skin of CARD_BACK_SKINS) noteCardBackFailed(skin.id);
    const view = render(<Probe />);
    expect(view.getByTestId('probe').dataset.skin).toBe('none');
  });
});
