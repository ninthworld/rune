/**
 * The presentation clock (issue #594) — the impure half of the trail: one timer,
 * the discontinuity and decision rules, and the tolerance for a tab that was not
 * running.
 *
 * The pacing itself is proved without a clock in `presentationTrail.test.ts`.
 * What is asserted here is only what a timer and a view stream can get wrong: the
 * order and dwell a real consumer sees, what a reconnect does to a backlog, what a
 * decision does to it, that a redelivered window costs nothing, and that a tab
 * which slept through its own deadlines lands on the present instead of narrating
 * history.
 *
 * Ordinary playback runs on the faked clock, where a timer fires exactly at its
 * own deadline. The throttled tab is staged by offsetting `Date.now` *after* the
 * timer is armed: the wake then lands minutes past a deadline whose delay never
 * changed, which is precisely what a backgrounded tab does and is not something an
 * advancing fake clock can reproduce on its own.
 */
import { StrictMode } from 'react';
import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GameView, MomentKind, PresentationMoment } from '../../protocol';
import { SAMPLE_GAME_VIEW } from '../../game-view.fixture';
import { PRESENTATION_DWELL } from './presentationTrail';
import { usePresentationTrail } from './usePresentationTrail';

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

/** Run the clock forward, letting every wake it reaches land. */
function elapse(ms: number): void {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

/**
 * Put the tab to sleep: the wall clock jumps by `ms` while the armed timer's own
 * delay is untouched, so its next wake runs that far past its deadline.
 */
function throttleBy(ms: number): void {
  const clock = Date.now;
  vi.spyOn(Date, 'now').mockImplementation(() => clock() + ms);
}

let nextId = 400;

/** One moment, at the next id in the stream. */
function moment(kind: MomentKind, overrides: Partial<PresentationMoment> = {}): PresentationMoment {
  return {
    id: (nextId += 1),
    batch: 7,
    turn: 5,
    phase: 'precombat_main',
    count: 1,
    kind,
    ...overrides,
  };
}

/** A named object as the server retained it. */
function object(id: string, name: string) {
  return { id, name };
}

/** The three-beat removal batch: cast → resolved → died. */
function removalBatch(): PresentationMoment[] {
  return [
    moment({ kind: 'cast', player: 'p2', object: object('s1', 'Quickfire Bolt') }),
    moment({ kind: 'resolved', player: 'p2', object: object('s1', 'Quickfire Bolt') }),
    moment({ kind: 'died', object: object('perm_xyz', 'Grizzly Bears') }),
  ];
}

/** A view carrying `presentation`, with the decision state under test control. */
function viewWith(
  presentation: PresentationMoment[],
  decision: { deciding?: boolean; deadline?: number } = {},
): GameView {
  const deciding = decision.deciding ?? false;
  return {
    ...SAMPLE_GAME_VIEW,
    valid_actions: deciding ? SAMPLE_GAME_VIEW.valid_actions : [],
    action_deadline: decision.deadline,
    presentation,
  };
}

/** The hook, with its staged moment published as data attributes. */
function Harness({
  view,
  sessionEpoch = 0,
  reducedMotion = false,
}: {
  view: GameView | null;
  sessionEpoch?: number;
  reducedMotion?: boolean;
}) {
  const staged = usePresentationTrail(view, { sessionEpoch, reducedMotion });
  return (
    <div
      data-testid="staged"
      data-kind={staged?.moment.kind?.kind ?? 'none'}
      data-id={staged?.moment.id ?? ''}
      data-count={staged?.moment.count ?? ''}
      data-travel={staged === null ? '' : String(staged.travel)}
    />
  );
}

/** The kind currently staged, or `'none'`. */
function stagedKind(container: HTMLElement): string {
  return container.querySelector<HTMLElement>('[data-testid="staged"]')?.dataset.kind ?? '';
}

describe('usePresentationTrail — ordered playback (issue #594)', () => {
  it('stages a removal batch as cast, then resolution, then death', () => {
    const { container } = render(<Harness view={viewWith(removalBatch())} />);

    // The three states the board passed through, in the order it passed through
    // them — the whole point of the contract. A board diff can only say the bear
    // is gone; this says a bolt resolved and killed it.
    expect(stagedKind(container)).toBe('cast');
    elapse(PRESENTATION_DWELL.cast);
    expect(stagedKind(container)).toBe('resolved');
    elapse(PRESENTATION_DWELL.resolution);
    expect(stagedKind(container)).toBe('died');
    elapse(PRESENTATION_DWELL.zone);
    expect(stagedKind(container)).toBe('none');
  });

  it('holds each beat for its own dwell and not a millisecond less', () => {
    const { container } = render(<Harness view={viewWith(removalBatch())} />);

    elapse(PRESENTATION_DWELL.cast - 1);
    expect(stagedKind(container)).toBe('cast');
    elapse(1);
    expect(stagedKind(container)).toBe('resolved');
    elapse(PRESENTATION_DWELL.resolution - 1);
    expect(stagedKind(container)).toBe('resolved');
  });

  it('plays a later window after the first one has gone quiet', () => {
    const first = viewWith(removalBatch());
    const { container, rerender } = render(<Harness view={first} />);
    elapse(PRESENTATION_DWELL.cast + PRESENTATION_DWELL.resolution + PRESENTATION_DWELL.zone);
    expect(stagedKind(container)).toBe('none');

    const second = viewWith([moment({ kind: 'turn_change', turn: 6, active_player: 'p2' })], {
      deciding: false,
    });
    rerender(<Harness view={second} />);

    expect(stagedKind(container)).toBe('turn_change');
  });

  it('is a no-op when the same window is delivered a second time', () => {
    const window = removalBatch();
    const { container, rerender } = render(<Harness view={viewWith(window)} />);
    elapse(200);
    expect(stagedKind(container)).toBe('cast');

    // A redelivery — a reconnect resend, or an overlapping bounded window. The
    // watermark makes it free: the caption on screen is NOT restarted, so its
    // remaining dwell is exactly what it was.
    rerender(<Harness view={viewWith([...window])} />);
    expect(stagedKind(container)).toBe('cast');
    elapse(PRESENTATION_DWELL.cast - 200);
    expect(stagedKind(container)).toBe('resolved');
  });
});

describe('usePresentationTrail — a decision outranks cosmetics (issue #594)', () => {
  it('drops a stale backlog when a decision arrives', () => {
    const backlog = [
      moment({ kind: 'drew', player: 'p2', count: 1 }),
      moment({ kind: 'phase_change', phase: 'end' }),
      moment({ kind: 'life', player: 'p2', amount: -3 }),
    ];
    const { container, rerender } = render(<Harness view={viewWith(backlog)} />);
    expect(stagedKind(container)).toBe('drew');

    // The seat is handed something to answer, with a clock on it. Every low-value
    // caption still queued behind describes an earlier frame and goes at once; a
    // decision timer must not run down behind it.
    const decision = viewWith([], { deciding: true, deadline: 20 });
    rerender(<Harness view={decision} />);

    // What was already on screen finishes its own dwell — a caption is never cut
    // off under the reader's eye — and then the surface is quiet, because the two
    // unprotected moments behind it were dropped rather than played.
    expect(stagedKind(container)).toBe('drew');
    elapse(PRESENTATION_DWELL.other);
    expect(stagedKind(container)).toBe('none');
  });

  it('hurries, but never discards, a causal chain the decision arrived one frame behind', () => {
    // The headline acceptance scenario of issue #594, in the shape the server
    // actually delivers it. A settle comes to rest with the *caster* holding
    // priority, so the window carrying cast → resolved → died reaches the helpless
    // seat on a broadcast with no actions on it; that seat's own decision arrives on
    // the NEXT broadcast, which against an AI opponent is milliseconds later. The
    // chain needs longer than that to play, so a fast-forward on the decision edge
    // would throw away the resolution and the death — leaving exactly the board diff
    // this contract exists to replace, with no way to replay them (their ids are
    // already under the watermark).
    const { container, rerender } = render(<Harness view={viewWith(removalBatch())} />);
    expect(stagedKind(container)).toBe('cast');

    elapse(50);
    rerender(
      <Harness
        view={viewWith([moment({ kind: 'phase_change', phase: 'upkeep' })], {
          deciding: true,
          deadline: 20,
        })}
      />,
    );

    // Sampled finely enough to catch the floored beats, which are shorter than any
    // ordinary dwell precisely because a decision is now waiting.
    const seen: string[] = [stagedKind(container)];
    for (let tick = 0; tick < 40; tick += 1) {
      elapse(40);
      const kind = stagedKind(container);
      if (kind !== seen[seen.length - 1]) seen.push(kind);
    }

    // Cause, effect, and consequence all reached the screen, in order, and the
    // window explaining the new decision played after them.
    expect(seen).toEqual(['cast', 'resolved', 'died', 'phase_change', 'none']);
  });

  it('still plays the window that explains the decision it arrived with', () => {
    const { container, rerender } = render(<Harness view={viewWith([])} />);
    const decision = viewWith(removalBatch(), { deciding: true, deadline: 20 });
    rerender(<Harness view={decision} />);

    // The moments arriving WITH the decision are its causal explanation, and they
    // cover nothing and gate nothing while they play.
    expect(stagedKind(container)).toBe('cast');
    elapse(PRESENTATION_DWELL.cast);
    expect(stagedKind(container)).toBe('resolved');
  });

  it('keeps playing across frames where the seat simply still has priority', () => {
    const window = removalBatch();
    const { container, rerender } = render(
      <Harness view={viewWith(window, { deciding: true, deadline: 20 })} />,
    );
    expect(stagedKind(container)).toBe('cast');

    // An ordinary follow-up frame: the seat had priority before and still does,
    // and the deadline is merely counting down. That is not an arrival, and
    // treating it as one would mean nothing ever played at all.
    rerender(<Harness view={viewWith([...window], { deciding: true, deadline: 18 })} />);
    expect(stagedKind(container)).toBe('cast');
    elapse(PRESENTATION_DWELL.cast);
    expect(stagedKind(container)).toBe('resolved');
  });
});

describe('usePresentationTrail — discontinuities (issue #594)', () => {
  it('fast-forwards to the present when the transport generation changes', () => {
    const backlog = removalBatch();
    const { container, rerender } = render(<Harness view={viewWith(backlog)} sessionEpoch={3} />);
    expect(stagedKind(container)).toBe('cast');

    // A reconnect. The view being rendered is already the state those moments led
    // to, and they describe a session that no longer exists, so they are dropped
    // rather than replayed.
    rerender(<Harness view={viewWith([...backlog])} sessionEpoch={4} />);

    expect(stagedKind(container)).toBe('none');
  });

  it('plays the window a reconnect brings with it, once', () => {
    const { container, rerender } = render(
      <Harness view={viewWith(removalBatch())} sessionEpoch={3} />,
    );
    const resync = viewWith([moment({ kind: 'turn_change', turn: 9, active_player: 'p1' })]);
    rerender(<Harness view={resync} sessionEpoch={4} />);

    expect(stagedKind(container)).toBe('turn_change');
  });

  it('lands on the present after a tab that slept through its own deadlines', () => {
    const long = [
      moment({ kind: 'cast', player: 'p2', object: object('s1', 'Quickfire Bolt') }),
      moment({ kind: 'cast', player: 'p2', object: object('s2', 'Stone Rain') }),
      moment({ kind: 'cast', player: 'p2', object: object('s3', 'Shock') }),
      moment({ kind: 'cast', player: 'p2', object: object('s4', 'Giant Growth') }),
    ];
    const { container } = render(<Harness view={viewWith(long)} />);
    expect(stagedKind(container)).toBe('cast');

    // Minutes pass with the tab throttled, and the overdue timer finally runs. It
    // must land on the present rather than narrate the backlog in real time.
    throttleBy(60_000);
    elapse(PRESENTATION_DWELL.cast);

    expect(stagedKind(container)).toBe('none');
  });
});

describe('usePresentationTrail — the timer it owns (issue #594)', () => {
  it('arms nothing once the window has played out', () => {
    render(<Harness view={viewWith([moment({ kind: 'phase_change', phase: 'end' })])} />);
    elapse(PRESENTATION_DWELL.other);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('cancels its timer on unmount', () => {
    const { unmount } = render(<Harness view={viewWith(removalBatch())} />);
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('keeps its clock armed through a strict-effects remount at mount', () => {
    // `main.tsx` wraps the app in StrictMode and `LiveMatchTable` mounts on the
    // first non-null view, so in development the opening settle's window is always
    // ingested by an effect that React immediately tears down and re-runs. The
    // re-run admits nothing — every id is at the watermark already — and must
    // still put back the timer the teardown cancelled, or the first caption of the
    // match freezes and its batch never plays.
    const { container } = render(
      <StrictMode>
        <Harness view={viewWith(removalBatch())} />
      </StrictMode>,
    );

    expect(stagedKind(container)).toBe('cast');
    expect(vi.getTimerCount()).toBe(1);

    // And the recovered timer runs the original dwell, not a restarted one: the
    // re-pump reads the same clock, and `advance` is idempotent for it.
    elapse(PRESENTATION_DWELL.cast - 1);
    expect(stagedKind(container)).toBe('cast');
    elapse(1);
    expect(stagedKind(container)).toBe('resolved');
    elapse(PRESENTATION_DWELL.resolution);
    expect(stagedKind(container)).toBe('died');
  });

  it('stages nothing at all before the first view', () => {
    const { container } = render(<Harness view={null} />);
    expect(stagedKind(container)).toBe('none');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('ignores a view whose window is absent, as an older server sends it', () => {
    const older: GameView = { ...SAMPLE_GAME_VIEW };
    delete older.presentation;
    const { container } = render(<Harness view={older} />);
    expect(stagedKind(container)).toBe('none');
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('usePresentationTrail — reduced motion (issue #594)', () => {
  it('stages the identical sequence with reduced motion on', () => {
    const window = removalBatch();
    const plain = render(<Harness view={viewWith(window)} />);
    const kinds: string[] = [stagedKind(plain.container)];
    elapse(PRESENTATION_DWELL.cast);
    kinds.push(stagedKind(plain.container));
    elapse(PRESENTATION_DWELL.resolution);
    kinds.push(stagedKind(plain.container));
    plain.unmount();

    const reduced = render(<Harness view={viewWith(removalBatch())} reducedMotion />);
    const reducedKinds: string[] = [stagedKind(reduced.container)];
    elapse(PRESENTATION_DWELL.cast);
    reducedKinds.push(stagedKind(reduced.container));
    elapse(PRESENTATION_DWELL.resolution);
    reducedKinds.push(stagedKind(reduced.container));

    // A dwell is not an animation: the reader who asked for less movement did not
    // ask to read faster, so the same beats hold for the same budgets.
    expect(reducedKinds).toEqual(kinds);
  });

  it('suppresses travel only', () => {
    const { container } = render(<Harness view={viewWith(removalBatch())} reducedMotion />);
    const staged = container.querySelector<HTMLElement>('[data-testid="staged"]');
    expect(staged?.dataset.kind).toBe('cast');
    expect(staged?.dataset.travel).toBe('false');
  });
});
