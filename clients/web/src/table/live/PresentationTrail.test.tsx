/**
 * The presentation caption (issue #594): what one staged moment says, and what
 * the surface refuses to be.
 *
 * Two halves. The first renders the component directly over a staged moment,
 * because the words are the whole deliverable — a caption that cannot distinguish
 * "countered" from "fizzled" from "died" has thrown away the only information a
 * board diff could not recover. The second mounts the real
 * {@link LiveMatchTable} and lets the clock run, which is the only place the
 * wiring (hook → surface → shell) is actually proved, including the case that
 * matters most for compatibility: a server that sends no window at all.
 */
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GameView, MomentKind, PresentationMoment } from '../../protocol';
import { SAMPLE_GAME_VIEW, SAMPLE_GAME_VIEW_JSON } from '../../game-view.fixture';
import { registerTableTestHooks, seed } from '../table-test-support';
import { LiveMatchTable } from './LiveMatchTable';
import { PresentationTrail } from './PresentationTrail';
import { PRESENTATION_DWELL, type StagedMoment } from './presentationTrail';

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

/** One moment at a fixed position; only the kind varies between cases. */
function moment(kind: MomentKind, overrides: Partial<PresentationMoment> = {}): PresentationMoment {
  return { id: 412, batch: 57, turn: 5, phase: 'precombat_main', count: 1, kind, ...overrides };
}

/** Stage a moment the way the scheduler would. */
function stage(
  kind: MomentKind,
  overrides: Partial<PresentationMoment> = {},
  travel = false,
): StagedMoment {
  return { moment: moment(kind, overrides), dwellMs: PRESENTATION_DWELL.other, travel };
}

/** The naming view: two seats with real display names. */
const NAMES: Pick<GameView, 'player_names'> = {
  player_names: { p1: 'Ari', p2: 'Bex' },
};

/** The caption's text, with the count suffix included as rendered. */
function captionText(): string {
  return screen.getByTestId('presentation-trail-caption').textContent ?? '';
}

describe('PresentationTrail — what one moment says (issue #594)', () => {
  const bolt = { id: 's1', name: 'Quickfire Bolt' };
  const bear = { id: 'perm_xyz', name: 'Grizzly Bears' };

  const cases: Array<[string, MomentKind, string]> = [
    ['a cast', { kind: 'cast', player: 'p2', object: bolt }, 'Bex casts Quickfire Bolt'],
    ['a resolution', { kind: 'resolved', player: 'p2', object: bolt }, 'Quickfire Bolt resolves'],
    ['a counter', { kind: 'countered', player: 'p2', object: bolt }, 'Quickfire Bolt is countered'],
    ['a fizzle', { kind: 'fizzled', player: 'p2', object: bolt }, 'Quickfire Bolt fizzles'],
    ['a death', { kind: 'died', object: bear }, 'Grizzly Bears dies'],
    [
      'a zone move',
      { kind: 'zone_move', object: bear, from: 'battlefield', to: 'graveyard' },
      'Grizzly Bears: the battlefield → the graveyard',
    ],
    [
      'damage to a player',
      { kind: 'damage', target: { kind: 'player', player: 'p2' }, amount: 3 },
      'Bex takes 3 damage',
    ],
    [
      'damage to a permanent',
      { kind: 'damage', target: { kind: 'permanent', permanent: bear }, amount: 2 },
      'Grizzly Bears takes 2 damage',
    ],
    ['a life gain', { kind: 'life', player: 'p1', amount: 4 }, 'Ari gains 4 life'],
    ['a life loss', { kind: 'life', player: 'p1', amount: -4 }, 'Ari loses 4 life'],
    [
      'an attack',
      { kind: 'attacked', player: 'p1', attackers: [bear] },
      'Ari attacks with Grizzly Bears',
    ],
    [
      'no attackers',
      { kind: 'attacked', player: 'p1', attackers: [] },
      'Ari declares no attackers',
    ],
    [
      'a block',
      { kind: 'blocked', player: 'p2', blocks: [{ blocker: bear, attacker: bolt }] },
      'Bex blocks with Grizzly Bears',
    ],
    ['a draw', { kind: 'drew', player: 'p1', count: 2 }, 'Ari draws 2 cards'],
    ['a turn change', { kind: 'turn_change', turn: 6, active_player: 'p2' }, 'Turn 6 — Bex'],
    ['a phase change', { kind: 'phase_change', phase: 'declare_attackers' }, 'Declare Attackers'],
    [
      'an elimination',
      { kind: 'eliminated', player: 'p2', reason: 'life_zero' },
      'Bex is eliminated (life total reached zero)',
    ],
    [
      'a win',
      { kind: 'game_over', result: { winner: 'p1', losers: ['p2'], reason: 'concede' } },
      'Game over — Ari wins (conceded)',
    ],
    [
      'a draw result',
      { kind: 'game_over', result: { losers: ['p1', 'p2'], reason: 'decked' } },
      'Game over — draw (drew from an empty library)',
    ],
  ];

  it.each(cases)('captions %s', (_label, kind, expected) => {
    render(<PresentationTrail staged={stage(kind)} view={NAMES} />);
    expect(captionText()).toBe(expected);
  });

  it('names the object the server retained, not one it looks up now', () => {
    // The bear is long gone from every zone in the view; the caption still reads
    // correctly, because the name was fixed at record time and travels with the
    // moment. This is the case a board diff cannot render at all.
    render(<PresentationTrail staged={stage({ kind: 'died', object: bear })} view={NAMES} />);
    expect(captionText()).toBe('Grizzly Bears dies');
  });

  it('marks an aggregated run with its count', () => {
    render(
      <PresentationTrail
        staged={stage(
          { kind: 'damage', target: { kind: 'player', player: 'p2' }, amount: 1 },
          {
            count: 4,
          },
        )}
        view={NAMES}
      />,
    );
    expect(screen.getByTestId('presentation-trail-count').textContent).toBe('×4');
    expect(screen.getByTestId('presentation-trail').dataset.count).toBe('4');
  });

  it('states one auto-passed path as one cue, with the reason in plain words', () => {
    render(
      <PresentationTrail
        staged={stage({
          kind: 'phases_skipped',
          steps: [
            { phase: 'upkeep', turn: 8 },
            { phase: 'draw', turn: 8 },
            { phase: 'begin_combat', turn: 8 },
          ],
          reason: 'no_response_available',
        })}
        view={NAMES}
      />,
    );
    // One caption for the whole path — never one per priority window — and the
    // concise reason under it. Nothing here is answerable or dismissible.
    expect(captionText()).toBe('Auto-passed Upkeep → Begin Combat');
    expect(screen.getByTestId('presentation-trail-cue').textContent).toBe('no response available');
  });

  it('names a single skipped step rather than a span', () => {
    render(
      <PresentationTrail
        staged={stage({
          kind: 'phases_skipped',
          steps: [{ phase: 'end', turn: 8 }],
          reason: 'forced_declaration',
        })}
        view={NAMES}
      />,
    );
    expect(captionText()).toBe('Auto-passed End');
    expect(screen.getByTestId('presentation-trail-cue').textContent).toBe('forced declaration');
  });

  it('renders nothing at all when the trail is quiet', () => {
    render(<PresentationTrail staged={null} view={NAMES} />);
    expect(screen.queryByTestId('presentation-trail')).toBeNull();
  });

  it('says nothing about a kind this build cannot read', () => {
    // Forward compatibility: the normalizer keeps an unknown kind so the ORDER is
    // preserved and the beat still costs its dwell, but the surface will not
    // invent words for an event it does not know.
    const unknown: StagedMoment = {
      moment: { id: 9, batch: 1, turn: 5, phase: 'upkeep', count: 1, kindUnknown: true },
      dwellMs: PRESENTATION_DWELL.other,
      travel: false,
    };
    render(<PresentationTrail staged={unknown} view={NAMES} />);
    expect(screen.queryByTestId('presentation-trail')).toBeNull();
  });

  it('is a caption and not a prompt: nothing to answer, nothing to announce twice', () => {
    const { container } = render(
      <PresentationTrail staged={stage({ kind: 'died', object: bear })} view={NAMES} />,
    );
    const surface = screen.getByTestId('presentation-trail');
    expect(surface.getAttribute('aria-hidden')).toBe('true');
    expect(container.querySelectorAll('button, a, input, [tabindex]')).toHaveLength(0);
  });

  it('carries the same words with reduced motion, dropping only travel', () => {
    const travelling = stage({ kind: 'cast', player: 'p2', object: bolt }, {}, true);
    const still = stage({ kind: 'cast', player: 'p2', object: bolt }, {}, false);

    const moving = render(<PresentationTrail staged={travelling} view={NAMES} />);
    const movingText = captionText();
    expect(screen.getByTestId('presentation-trail').dataset.travel).toBe('true');
    moving.unmount();

    render(<PresentationTrail staged={still} view={NAMES} />);
    expect(captionText()).toBe(movingText);
    expect(screen.getByTestId('presentation-trail').dataset.travel).toBeUndefined();
  });
});

describe('PresentationTrail on the live table (issue #594)', () => {
  beforeEach(() => {
    vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  /** The sample frame plus a presentation window, as wire text. */
  function seedWithWindow(presentation: unknown[]): void {
    const payload = { ...JSON.parse(SAMPLE_GAME_VIEW_JSON), presentation };
    seed(JSON.stringify(payload));
  }

  /** A removal chain exactly as the server puts it on the wire — `count` elided
   * at its default of 1, which is the shape the normalizer has to materialize. */
  const removal = [
    {
      id: 412,
      batch: 57,
      turn: 5,
      phase: 'precombat_main',
      kind: { kind: 'cast', player: 'p2', object: { id: 's1', name: 'Lightning Bolt' } },
    },
    {
      id: 413,
      batch: 57,
      turn: 5,
      phase: 'precombat_main',
      cause: 412,
      kind: { kind: 'resolved', player: 'p2', object: { id: 's1', name: 'Lightning Bolt' } },
    },
    {
      id: 414,
      batch: 57,
      turn: 5,
      phase: 'precombat_main',
      cause: 413,
      kind: { kind: 'died', object: { id: 'perm_xyz', name: 'Grizzly Bears' } },
    },
  ];

  it('plays a removal batch as three captions in the order the game passed through them', () => {
    vi.useFakeTimers();
    seedWithWindow(removal);
    render(<LiveMatchTable />);

    expect(captionText()).toBe('p2 casts Lightning Bolt');
    act(() => {
      vi.advanceTimersByTime(PRESENTATION_DWELL.cast);
    });
    expect(captionText()).toBe('Lightning Bolt resolves');
    act(() => {
      vi.advanceTimersByTime(PRESENTATION_DWELL.resolution);
    });
    expect(captionText()).toBe('Grizzly Bears dies');
    act(() => {
      vi.advanceTimersByTime(PRESENTATION_DWELL.zone);
    });
    expect(screen.queryByTestId('presentation-trail')).toBeNull();
  });

  it('never gates the board it is captioning', () => {
    vi.useFakeTimers();
    seedWithWindow(removal);
    render(<LiveMatchTable />);

    // The view is applied on arrival and every control is live from the first
    // frame: the caption paces over an already-authoritative, already-answerable
    // board and holds nothing back.
    expect(screen.getByTestId('presentation-trail')).toBeTruthy();
    expect(screen.getByTestId<HTMLButtonElement>('live-hand-card-c1').disabled).toBe(false);
  });

  it('renders the whole table for a server that sends no window at all', () => {
    // The field is additive and elided by default; an older server simply has no
    // presentation to carry, and the surface is absent rather than empty.
    seed(SAMPLE_GAME_VIEW_JSON);
    render(<LiveMatchTable />);

    expect(screen.getByTestId('live-match-table')).toBeTruthy();
    expect(screen.queryByTestId('presentation-trail')).toBeNull();
    expect(SAMPLE_GAME_VIEW.presentation).toEqual([]);
  });
});

describe('PresentationTrail — a zone move names both endpoints (issue #594)', () => {
  it('distinguishes a commander returning from exile from one returning from a graveyard', () => {
    // The server maintains an origin map purely so CR 903.9a's two cases can be
    // told apart, and refuses to emit the moment at all when it cannot prove which.
    // Rendering only the arrival would hand back the ambiguity the pair removes.
    const commander = { id: 'e9', name: 'Jedit Ojanen' };

    const { unmount } = render(
      <PresentationTrail
        staged={stage({ kind: 'zone_move', object: commander, from: 'exile', to: 'command' })}
        view={NAMES}
      />,
    );
    const fromExile = captionText();
    unmount();

    render(
      <PresentationTrail
        staged={stage({ kind: 'zone_move', object: commander, from: 'graveyard', to: 'command' })}
        view={NAMES}
      />,
    );
    const fromGraveyard = captionText();

    expect(fromExile).toBe('Jedit Ojanen: exile → the command zone');
    expect(fromGraveyard).toBe('Jedit Ojanen: the graveyard → the command zone');
    expect(fromExile).not.toBe(fromGraveyard);
  });
});
