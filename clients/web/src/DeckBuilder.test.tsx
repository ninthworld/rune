import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { DeckBuilder } from './DeckBuilder';
import { STARTER_DECKLISTS, decklistCounts, decklistSize } from './decklists';
import { CATALOG_VIEW } from './catalog-view.fixture';
import {
  MemorySavedDeckDb,
  configureSavedDeckStore,
  loadSavedDeck,
  resetSavedDeckStore,
  saveDeck,
  type SavedDeckDb,
} from './deck/savedDeckStore';

// A default in-memory saved-deck store for every test so the builder's panel never
// touches real IndexedDB; individual saved-deck tests reconfigure with their own db.
beforeEach(() => {
  configureSavedDeckStore({ db: new MemorySavedDeckDb(), now: () => 1 });
});

afterEach(() => {
  cleanup();
  resetSavedDeckStore();
});

/** A backing store whose operations reject — the "storage unavailable" case. */
function failingDb(): SavedDeckDb {
  const fail = (): Promise<never> => Promise.reject(new Error('storage unavailable'));
  return { getAll: fail, get: fail, put: fail, delete: fail };
}

/** Render the builder over the fixture catalog with sensible defaults. */
function renderBuilder(overrides: Partial<Parameters<typeof DeckBuilder>[0]> = {}): {
  onSubmit: ReturnType<typeof vi.fn>;
  onClose: ReturnType<typeof vi.fn>;
} {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  render(
    <DeckBuilder
      catalog={CATALOG_VIEW}
      format={CATALOG_VIEW.formats[0]}
      initialCounts={{}}
      onSubmit={onSubmit}
      onClose={onClose}
      {...overrides}
    />,
  );
  return { onSubmit, onClose };
}

describe('DeckBuilder (issue #368)', () => {
  it('lists every supported card from the catalog', () => {
    renderBuilder();
    for (const card of CATALOG_VIEW.cards) {
      expect(screen.getByTestId(`deck-builder-card-${card.functional_id}`)).toBeDefined();
    }
  });

  it('shows each card’s cost and type line for browsing', () => {
    renderBuilder();
    const angel = screen.getByTestId('deck-builder-card-serra_angel');
    // The cost renders through the shared CardFace cost disc — one swatched pip
    // per symbol at the screen-space tiers (card-representation §3.5) — rather
    // than the raw brace string; the type line renders verbatim.
    expect(angel.textContent).toContain('3WW');
    expect(angel.textContent).toContain('Creature — Angel');
  });

  it('renders every card surface through the shared DOM card component (#508)', () => {
    renderBuilder({ initialCounts: { serra_angel: 1 } });
    // Pool entries render a hand-tier CardFace — the 0.715 portrait card, the
    // only silhouette that carries a cost disc and a type bar (§3.3).
    const pool = screen.getByTestId('deck-builder-card-serra_angel');
    const poolFace = pool.querySelector('[data-tier="hand"]');
    expect(poolFace).not.toBeNull();
    expect(poolFace?.getAttribute('role')).toBe('img');
    expect(poolFace?.getAttribute('aria-label')).toBe('Serra Angel');
    // Running-deck entries render a chip-tier CardFace — no bespoke card markup.
    const deckRow = screen.getByTestId('deck-builder-deck-row-serra_angel');
    expect(deckRow.querySelector('[data-tier="chip"]')).not.toBeNull();
    // The panel carries the scene-token elevation ladder (a --deck-* custom property).
    const panel = screen.getByTestId('deck-builder');
    expect(panel.style.getPropertyValue('--deck-elev-held')).not.toBe('');
  });

  it('inspects a card with its rules text through the shared inspect treatment', () => {
    renderBuilder();
    fireEvent.click(screen.getByTestId('deck-builder-inspect-serra_angel'));
    // The universal inspect surface brings the real card face forward (issue
    // #569) and renders the server-computed rules text on it verbatim.
    const inspect = screen.getByTestId('card-inspect');
    const face = within(inspect).getByRole('img', { name: 'Serra Angel' });
    expect(face.getAttribute('data-tier')).toBe('inspect');
    expect(face.textContent).toContain('Flying, vigilance');
  });

  it('adds and removes copies, tracking per-card and running counts', () => {
    renderBuilder();
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('0 cards');
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('0');

    // Two Shocks and one Angel.
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-serra_angel'));
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('2');
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('3 cards');

    // Remove one Shock: the per-card and running counts both drop.
    fireEvent.click(screen.getByTestId('deck-builder-remove-shock'));
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('1');
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('2 cards');
  });

  it('cannot remove below zero (the remove control disables at zero)', () => {
    renderBuilder();
    const remove = screen.getByTestId('deck-builder-remove-shock') as HTMLButtonElement;
    expect(remove.disabled).toBe(true);
  });

  it('submits the built list as functional ids with duplicates repeated', () => {
    const { onSubmit } = renderBuilder();
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-serra_angel'));

    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const cards = onSubmit.mock.calls[0][0] as string[];
    expect(cards).toHaveLength(3);
    expect(cards.filter((c) => c === 'shock')).toHaveLength(2);
    expect(cards.filter((c) => c === 'serra_angel')).toHaveLength(1);
  });

  it('displays the format’s advertised deck rules as information (no legality here)', () => {
    renderBuilder();
    const rules = screen.getByTestId('deck-builder-format');
    // The strict 1v1 format: min 40, four copies with basics exempt, two players.
    expect(rules.textContent).toContain('Minimum 40 cards');
    expect(rules.textContent).toContain('Up to 4 copies');
    expect(rules.textContent).toContain('basic lands exempt');
    expect(rules.textContent).toContain('2 players');
  });

  it('reads a permissive format’s absent bounds as “no limit” honestly', () => {
    renderBuilder({ format: CATALOG_VIEW.formats[1] });
    const rules = screen.getByTestId('deck-builder-format');
    expect(rules.textContent).toContain('No minimum deck size');
    expect(rules.textContent).toContain('No copy limit');
    expect(rules.textContent).toContain('2–8 players');
  });

  it('seeds from a starter deck as a starting point for editing', () => {
    const starter = STARTER_DECKLISTS[0];
    renderBuilder({ initialCounts: decklistCounts(starter) });
    expect(screen.getByTestId('deck-builder-total').textContent).toBe(
      `${decklistSize(starter)} cards`,
    );
  });

  it('loads a starter into the builder with one tap, then lets it be edited', () => {
    const starter = STARTER_DECKLISTS[0];
    const { onSubmit } = renderBuilder();
    fireEvent.click(screen.getByTestId(`deck-builder-starter-${starter.id}`));
    expect(screen.getByTestId('deck-builder-total').textContent).toBe(
      `${decklistSize(starter)} cards`,
    );

    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    expect((onSubmit.mock.calls[0][0] as string[]).length).toBe(decklistSize(starter));
  });

  it('shows a loading state until the catalog arrives, without a dead screen', () => {
    const { onClose } = renderBuilder({ catalog: null });
    expect(screen.getByTestId('deck-builder-loading')).toBeDefined();
    // The pool is absent, but the modal stays interactive (Close/Cancel present).
    expect(screen.queryByTestId('deck-builder-pool')).toBeNull();
    fireEvent.click(screen.getByTestId('deck-builder-close'));
    expect(onClose).toHaveBeenCalled();
  });

  it('surfaces a rejection over the modal while preserving the built list', () => {
    // A rejection arrives as the lobby's non-fatal error; the builder keeps its state.
    render(
      <DeckBuilder
        catalog={CATALOG_VIEW}
        format={CATALOG_VIEW.formats[0]}
        initialCounts={{ shock: 3 }}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        error="That deck was rejected. Pick a deck and submit again."
      />,
    );
    expect(screen.getByTestId('deck-builder-error').textContent).toContain('rejected');
    // State preserved: the three Shocks are still in the deck for correction.
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('3');
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('3 cards');
  });

  it('displays the server’s specific deck-rejection reason, naming the card, with state preserved (issue #395)', () => {
    // The structured reason (rendered by the server from deck-legality data) flows in
    // through the same `error` prop; the builder shows it verbatim, keeps the built
    // list for correction, and still offers Submit (no client-side legality gate).
    const onSubmit = vi.fn();
    render(
      <DeckBuilder
        catalog={CATALOG_VIEW}
        format={CATALOG_VIEW.formats[0]}
        initialCounts={{ shock: 5 }}
        onSubmit={onSubmit}
        onClose={vi.fn()}
        error="Shock appears 5 times, above the 4-copy limit"
      />,
    );
    const shown = screen.getByTestId('deck-builder-error').textContent ?? '';
    expect(shown).toContain('Shock');
    expect(shown).toContain('above the 4-copy limit');
    // Builder state preserved for correction.
    expect(screen.getByTestId('deck-builder-count-shock').textContent).toBe('5');
    // Submit stays available — the client never pre-validates legality.
    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it('closes on Escape, backdrop, and Cancel (full keyboard + pointer operability)', () => {
    const onClose = vi.fn();
    render(
      <DeckBuilder
        catalog={CATALOG_VIEW}
        format={CATALOG_VIEW.formats[0]}
        initialCounts={{}}
        onSubmit={vi.fn()}
        onClose={onClose}
      />,
    );
    fireEvent.keyDown(screen.getByTestId('deck-builder'), { key: 'Escape' });
    fireEvent.click(screen.getByTestId('deck-builder-cancel'));
    fireEvent.click(screen.getByTestId('deck-builder-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(3);
  });
});

describe('DeckBuilder commander designation (issue #396)', () => {
  // The commander format advertised in the fixture: requires a commander (#394).
  const COMMANDER_FORMAT = CATALOG_VIEW.formats[2];

  it('shows no designation affordance when the format does not require a commander', () => {
    // A non-commander format (the 1v1 duel): with a legendary creature in the deck, the
    // builder still offers no designation control (gated by the advertised flag, #394).
    renderBuilder({ format: CATALOG_VIEW.formats[0], initialCounts: { jedit_ojanen: 1 } });
    expect(screen.queryByTestId('deck-builder-commander-status')).toBeNull();
    expect(screen.queryByTestId('deck-builder-designate-jedit_ojanen')).toBeNull();
  });

  it('lets the player designate exactly one card, rendering it distinctly', () => {
    renderBuilder({
      format: COMMANDER_FORMAT,
      initialCounts: { jedit_ojanen: 1, serra_angel: 1 },
    });
    // No commander yet: the status prompts for one.
    expect(screen.getByTestId('deck-builder-commander-none')).toBeDefined();

    // Designate the legendary creature.
    fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
    expect(screen.getByTestId('deck-builder-commander-name').textContent).toBe('Jedit Ojanen');
    expect(screen.getByTestId('deck-builder-commander-badge-jedit_ojanen')).toBeDefined();
    const designate = screen.getByTestId('deck-builder-designate-jedit_ojanen');
    expect(designate.getAttribute('aria-pressed')).toBe('true');

    // Designating another card moves the designation — never two at once.
    fireEvent.click(screen.getByTestId('deck-builder-designate-serra_angel'));
    expect(screen.getByTestId('deck-builder-commander-name').textContent).toBe('Serra Angel');
    expect(screen.queryByTestId('deck-builder-commander-badge-jedit_ojanen')).toBeNull();
    expect(screen.getByTestId('deck-builder-commander-badge-serra_angel')).toBeDefined();
  });

  it('clears the designation on demand before submit', () => {
    renderBuilder({ format: COMMANDER_FORMAT, initialCounts: { jedit_ojanen: 1 } });
    fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
    expect(screen.getByTestId('deck-builder-commander-name')).toBeDefined();
    fireEvent.click(screen.getByTestId('deck-builder-commander-clear'));
    expect(screen.queryByTestId('deck-builder-commander-name')).toBeNull();
    expect(screen.getByTestId('deck-builder-commander-none')).toBeDefined();
  });

  it('submits the built list carrying the designated commander', () => {
    const { onSubmit } = renderBuilder({
      format: COMMANDER_FORMAT,
      initialCounts: { jedit_ojanen: 1, forest: 2 },
    });
    fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
    fireEvent.click(screen.getByTestId('deck-builder-submit'));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    const [cards, commander] = onSubmit.mock.calls[0] as [string[], string | undefined];
    expect(cards.filter((c) => c === 'jedit_ojanen')).toHaveLength(1);
    expect(cards.filter((c) => c === 'forest')).toHaveLength(2);
    expect(commander).toBe('jedit_ojanen');
  });

  it('never sends a commander in a non-commander format even if one was seeded', () => {
    const { onSubmit } = renderBuilder({
      format: CATALOG_VIEW.formats[0],
      initialCounts: { jedit_ojanen: 1 },
      initialCommander: 'jedit_ojanen',
    });
    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    const [, commander] = onSubmit.mock.calls[0] as [string[], string | undefined];
    expect(commander).toBeUndefined();
  });

  it('drops a designation whose card is removed from the deck', () => {
    renderBuilder({ format: COMMANDER_FORMAT, initialCounts: { jedit_ojanen: 1 } });
    fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
    expect(screen.getByTestId('deck-builder-commander-name')).toBeDefined();
    // Remove the last copy: the designation clears rather than referencing a card the
    // deck no longer holds.
    fireEvent.click(screen.getByTestId('deck-builder-remove-jedit_ojanen'));
    expect(screen.queryByTestId('deck-builder-commander-name')).toBeNull();
  });

  it('seeds the designation from the starting deck and preserves it across a rejection', () => {
    // A server rejection surfaces as the lobby error over the modal; the built list AND
    // the designation must survive for correction (paired with #395).
    render(
      <DeckBuilder
        catalog={CATALOG_VIEW}
        format={COMMANDER_FORMAT}
        initialCounts={{ jedit_ojanen: 1, forest: 2 }}
        initialCommander="jedit_ojanen"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        error="That deck was rejected. Designate a legal commander and submit again."
      />,
    );
    expect(screen.getByTestId('deck-builder-error').textContent).toContain('rejected');
    // State preserved: the designation and the list are both intact.
    expect(screen.getByTestId('deck-builder-commander-name').textContent).toBe('Jedit Ojanen');
    expect(screen.getByTestId('deck-builder-count-forest').textContent).toBe('2');
  });

  it('hints likely candidates from the catalog type line without enforcing legality', () => {
    renderBuilder({
      format: COMMANDER_FORMAT,
      initialCounts: { jedit_ojanen: 1, shock: 1 },
    });
    // The legendary creature is hinted; a non-legendary is not — but both remain
    // designatable (the server, not the client, decides eligibility).
    expect(screen.getByTestId('deck-builder-commander-hint-jedit_ojanen')).toBeDefined();
    expect(screen.queryByTestId('deck-builder-commander-hint-shock')).toBeNull();
    expect(screen.getByTestId('deck-builder-designate-shock')).toBeDefined();
  });

  it('round-trips a designation through save and load', async () => {
    const db = new MemorySavedDeckDb();
    configureSavedDeckStore({ db, now: () => 1 });
    renderBuilder({ format: COMMANDER_FORMAT, initialCounts: { jedit_ojanen: 1, forest: 2 } });
    await screen.findByTestId('deck-builder-saved');

    fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
    fireEvent.change(screen.getByTestId('deck-builder-deck-name'), {
      target: { value: 'My General' },
    });
    fireEvent.click(screen.getByTestId('deck-builder-save'));
    await waitFor(async () =>
      expect((await loadSavedDeck('My General'))?.commander).toBe('jedit_ojanen'),
    );

    // Reload it into a fresh builder: the designation comes back with the list.
    cleanup();
    resetSavedDeckStore();
    configureSavedDeckStore({ db, now: () => 2 });
    renderBuilder({ format: COMMANDER_FORMAT });
    fireEvent.click(await screen.findByTestId('deck-builder-load-My General'));
    expect(screen.getByTestId('deck-builder-commander-name').textContent).toBe('Jedit Ojanen');
  });
});

describe('DeckBuilder saved decks (issue #369, ADR 0027)', () => {
  it('saves a built deck under a name and lists it on return in a new session', async () => {
    // The device's storage (one MemorySavedDeckDb instance) outlives the singleton.
    const db = new MemorySavedDeckDb();
    configureSavedDeckStore({ db, now: () => 1 });
    renderBuilder();
    await screen.findByTestId('deck-builder-saved');

    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.change(screen.getByTestId('deck-builder-deck-name'), {
      target: { value: 'My Burn' },
    });
    fireEvent.click(screen.getByTestId('deck-builder-save'));
    await screen.findByTestId('deck-builder-saved-row-My Burn');

    // New session: drop the singleton and re-open a fresh builder against the same
    // device storage — the saved deck is still there.
    cleanup();
    resetSavedDeckStore();
    configureSavedDeckStore({ db, now: () => 2 });
    renderBuilder();
    expect(await screen.findByTestId('deck-builder-saved-row-My Burn')).toBeDefined();
  });

  it('loads a saved deck, edits it, re-saves with an overwrite confirm, then deletes it', async () => {
    const db = new MemorySavedDeckDb();
    configureSavedDeckStore({ db, now: () => 1 });
    await saveDeck({ name: 'Angels', cards: [{ functional_id: 'serra_angel', count: 2 }] });
    renderBuilder();
    await screen.findByTestId('deck-builder-saved-row-Angels');

    // Load it into the builder.
    fireEvent.click(screen.getByTestId('deck-builder-load-Angels'));
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('2 cards');

    // Edit: add a Shock, then re-save under the same name — overwrite needs intent.
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    expect(screen.getByTestId('deck-builder-total').textContent).toBe('3 cards');
    fireEvent.click(screen.getByTestId('deck-builder-save'));
    // No silent data loss: an explicit overwrite confirmation is required.
    fireEvent.click(await screen.findByTestId('deck-builder-overwrite-confirm'));
    await waitFor(async () => {
      const reloaded = await loadSavedDeck('Angels');
      expect(reloaded?.cards.reduce((n, c) => n + c.count, 0)).toBe(3);
    });

    // Delete, also behind an explicit confirm.
    fireEvent.click(screen.getByTestId('deck-builder-delete-Angels'));
    fireEvent.click(screen.getByTestId('deck-builder-delete-confirm-Angels'));
    await waitFor(() => expect(screen.queryByTestId('deck-builder-saved-row-Angels')).toBeNull());
    expect(await loadSavedDeck('Angels')).toBeUndefined();
  });

  it('submits a saved deck through the unchanged submit_deck gate without corrupting the saved copy', async () => {
    const db = new MemorySavedDeckDb();
    configureSavedDeckStore({ db, now: () => 1 });
    await saveDeck({ name: 'Test', cards: [{ functional_id: 'shock', count: 4 }] });
    const { onSubmit } = renderBuilder();
    await screen.findByTestId('deck-builder-saved-row-Test');

    fireEvent.click(screen.getByTestId('deck-builder-load-Test'));
    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    // Submission is the same flat identity list the existing gate carries.
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const cards = onSubmit.mock.calls[0][0] as string[];
    expect(cards.filter((c) => c === 'shock')).toHaveLength(4);

    // A format rejection (a server-side submit_deck outcome) never touches the saved
    // copy — it remains intact for re-submission to a different format.
    expect((await loadSavedDeck('Test'))?.cards).toEqual([{ functional_id: 'shock', count: 4 }]);
  });

  it('degrades to the bundled-starters experience when device storage is unavailable', async () => {
    configureSavedDeckStore({ db: failingDb() });
    const { onSubmit } = renderBuilder();
    // The storage probe rejects: the panel hides rather than breaking the screen.
    await waitFor(() => expect(screen.queryByTestId('deck-builder-saved')).toBeNull());
    // The bundled-starters flow still works end to end.
    fireEvent.click(screen.getByTestId(`deck-builder-starter-${STARTER_DECKLISTS[0].id}`));
    fireEvent.click(screen.getByTestId('deck-builder-submit'));
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it('exports the versioned JSON and imports it back into an equivalent deck', async () => {
    const db = new MemorySavedDeckDb();
    configureSavedDeckStore({ db, now: () => 1 });
    await saveDeck({
      name: 'Export Me',
      cards: [
        { functional_id: 'shock', count: 2 },
        { functional_id: 'serra_angel', count: 1 },
      ],
    });
    renderBuilder();
    await screen.findByTestId('deck-builder-saved-row-Export Me');

    // Export produces the schema-versioned document.
    fireEvent.click(screen.getByTestId('deck-builder-export-Export Me'));
    const output = (await screen.findByTestId('deck-builder-export-output')) as HTMLTextAreaElement;
    const doc = JSON.parse(output.value);
    expect(doc.schema).toBe('rune.deck');
    expect(doc.version).toBe(2);

    // Import round-trips it back into the builder as an equivalent working deck.
    fireEvent.change(screen.getByTestId('deck-builder-import-text'), {
      target: { value: output.value },
    });
    fireEvent.click(screen.getByTestId('deck-builder-import'));
    await waitFor(() =>
      expect(screen.getByTestId('deck-builder-total').textContent).toBe('3 cards'),
    );
  });
});

describe('DeckBuilder deck-list motion (issue #508 — add/remove/designate)', () => {
  const COMMANDER_FORMAT = CATALOG_VIEW.formats[2];

  /** Stub `matchMedia` so `prefers-reduced-motion` reports the given state. */
  function stubReducedMotion(reduced: boolean): void {
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: query.includes('reduce') ? reduced : false,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const seq = (id: string): string | null =>
    screen.getByTestId(`deck-builder-deck-row-${id}`).getAttribute('data-change-seq');

  it('mounts a new card row as an enter (change-seq 0) and ticks it only on copy changes', () => {
    renderBuilder();
    // A brand-new card row is an enter, not a count change — the mount animation owns it.
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    expect(seq('shock')).toBe('0');
    // Adding and removing copies of the SAME card keeps the row and ticks the seq,
    // so the view re-triggers the count animation React would otherwise swallow.
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    expect(seq('shock')).toBe('1');
    fireEvent.click(screen.getByTestId('deck-builder-remove-shock'));
    expect(seq('shock')).toBe('2');
    // A different card entering is its own mount (seq 0) and leaves shock's seq intact.
    fireEvent.click(screen.getByTestId('deck-builder-add-serra_angel'));
    expect(seq('serra_angel')).toBe('0');
    expect(seq('shock')).toBe('2');
  });

  it('holds a removed row through its exit before unmounting (motion on)', () => {
    vi.useFakeTimers();
    try {
      renderBuilder();
      fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
      fireEvent.click(screen.getByTestId('deck-builder-remove-shock'));
      // The final copy is gone from the counts, but the row is held, marked leaving,
      // so its zone-travel exit can play.
      const row = screen.getByTestId('deck-builder-deck-row-shock');
      expect(row.getAttribute('data-phase')).toBe('leaving');
      // After one zone-travel duration it unmounts.
      act(() => vi.advanceTimersByTime(400));
      expect(screen.queryByTestId('deck-builder-deck-row-shock')).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('drops a removed row immediately under reduced motion (snap, no exit)', () => {
    stubReducedMotion(true);
    renderBuilder();
    fireEvent.click(screen.getByTestId('deck-builder-add-shock'));
    fireEvent.click(screen.getByTestId('deck-builder-remove-shock'));
    expect(screen.queryByTestId('deck-builder-deck-row-shock')).toBeNull();
  });

  it('plays a designation transition when a row’s commander status flips', () => {
    vi.useFakeTimers();
    try {
      renderBuilder({ format: COMMANDER_FORMAT, initialCounts: { jedit_ojanen: 1 } });
      fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
      const row = screen.getByTestId('deck-builder-deck-row-jedit_ojanen');
      expect(row.getAttribute('data-designating')).toBe('true');
      // The micro-class pulse clears itself; the designation (functional state) stays.
      act(() => vi.advanceTimersByTime(200));
      expect(
        screen.getByTestId('deck-builder-deck-row-jedit_ojanen').getAttribute('data-designating'),
      ).toBeNull();
      expect(
        screen.getByTestId('deck-builder-designate-jedit_ojanen').getAttribute('aria-pressed'),
      ).toBe('true');
    } finally {
      vi.useRealTimers();
    }
  });

  it('snaps designation under reduced motion (no transition, state intact)', () => {
    stubReducedMotion(true);
    renderBuilder({ format: COMMANDER_FORMAT, initialCounts: { jedit_ojanen: 1 } });
    fireEvent.click(screen.getByTestId('deck-builder-designate-jedit_ojanen'));
    const row = screen.getByTestId('deck-builder-deck-row-jedit_ojanen');
    expect(row.getAttribute('data-designating')).toBeNull();
    expect(
      screen.getByTestId('deck-builder-designate-jedit_ojanen').getAttribute('aria-pressed'),
    ).toBe('true');
  });
});
