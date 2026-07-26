/**
 * The presentation trail's pacing contract (issue #594): what folds, what never
 * folds, when a caption retires, what compression is allowed to cut, and what a
 * fast-forward makes idempotent.
 *
 * Every test drives an **injected clock** — the module takes `now` as a parameter,
 * so nothing here needs fake timers, and a failure names a scheduling decision
 * rather than a timer interaction. The one timer in this feature belongs to
 * `usePresentationTrail`, which is tested where it lives.
 */
import { describe, expect, it } from 'vitest';
import type { MomentKind, MomentObject, PresentationMoment } from '../../protocol';
import { SCENE_MOTION } from '../../sceneTokens';
import {
  accelerate,
  advance,
  backlogMs,
  compress,
  createPresentationTrail,
  dwellMsFor,
  enqueue,
  fastForward,
  isProtectedMoment,
  isTrailIdle,
  needsCompression,
  PRESENTATION_BACKLOG_MS,
  PRESENTATION_DECISION_MS,
  PRESENTATION_DWELL,
  withReducedMotion,
  type PresentationTrail,
  type StagedMoment,
} from './presentationTrail';

const object = (id: string, name: string): MomentObject => ({ id, name });

const BEAR = object('e1', 'Grizzly Bears');
const BOLT = object('e2', 'Quickfire Bolt');

/** One wire moment, normalized as {@link normalizePresentationMoments} would leave it. */
function at(
  id: number,
  kind: MomentKind,
  over: Partial<PresentationMoment> = {},
): PresentationMoment {
  return { id, batch: 1, turn: 3, phase: 'precombat_main', count: 1, kind, ...over };
}

/** The staged tags, in order — the assertion most of these tests make. */
function tags(entries: readonly StagedMoment[]): (string | undefined)[] {
  return entries.map((entry) => entry.moment.kind?.kind);
}

/** Play the whole trail out, returning every moment staged, in order. */
function playOut(trail: PresentationTrail): StagedMoment[] {
  const played: StagedMoment[] = [];
  let current = trail;
  let now = 0;
  // Bounded so a scheduling bug fails as a wrong sequence rather than a hang.
  for (let tick = 0; tick < 200; tick += 1) {
    const step = advance(current, now);
    current = step.trail;
    if (step.staged !== null && step.staged !== played[played.length - 1]) played.push(step.staged);
    if (step.wakeAt === null) break;
    now = step.wakeAt;
  }
  return played;
}

/** The causal chain a board diff cannot recover: cast → resolved → died → zone move. */
function chainFrom(base: number): PresentationMoment[] {
  return [
    at(base, { kind: 'cast', player: 'p1', object: BOLT }),
    at(base + 1, { kind: 'resolved', player: 'p1', object: BOLT }, { cause: base }),
    at(base + 2, { kind: 'died', object: BEAR }, { cause: base + 1 }),
    at(
      base + 3,
      { kind: 'zone_move', object: BEAR, from: 'battlefield', to: 'graveyard' },
      { cause: base + 2 },
    ),
  ];
}

const CHAIN = chainFrom(10);

describe('presentation trail — enqueue (issue #594)', () => {
  it('appends the window in the order the server gave it', () => {
    const trail = enqueue(createPresentationTrail(), CHAIN);
    expect(tags(trail.queue)).toEqual(['cast', 'resolved', 'died', 'zone_move']);
    expect(trail.queue.map((entry) => entry.moment.id)).toEqual([10, 11, 12, 13]);
    expect(trail.watermark).toBe(13);
  });

  it('drops a moment at or below the watermark so an overlapping window costs nothing', () => {
    const first = enqueue(createPresentationTrail(), CHAIN);
    // The server carries the recent suffix on every view: the next frame repeats
    // what this one already showed, plus one new moment.
    const second = enqueue(first, [...CHAIN, at(14, { kind: 'phase_change', phase: 'end' })]);
    expect(second.queue).toHaveLength(5);
    expect(tags(second.queue)).toEqual(['cast', 'resolved', 'died', 'zone_move', 'phase_change']);
    expect(second.watermark).toBe(14);
  });

  it('stages a window whose ids jump without stalling on the gap', () => {
    // Gaps are ordinary: the bounded window rolled, or another seat's per-seat
    // `phases_skipped` was filtered out of this stream. Neither is a lost message.
    const trail = enqueue(createPresentationTrail(), [
      at(400, { kind: 'cast', player: 'p1', object: BOLT }),
      at(9000, { kind: 'died', object: BEAR }),
    ]);
    expect(tags(trail.queue)).toEqual(['cast', 'died']);
    expect(playOut(trail).map((entry) => entry.moment.id)).toEqual([400, 9000]);
  });

  it('aggregates consecutive identical kinds into one entry with a raised count', () => {
    const damage: MomentKind = {
      kind: 'damage',
      target: { kind: 'player', player: 'p1' },
      amount: 1,
    };
    const trail = enqueue(createPresentationTrail(), [at(1, damage), at(2, damage), at(3, damage)]);
    expect(trail.queue).toHaveLength(1);
    expect(trail.queue[0].moment.count).toBe(3);
    // The entry keeps the id it started at, so its React key is stable as the run grows.
    expect(trail.queue[0].moment.id).toBe(1);
    // One dwell for the run, not three.
    expect(backlogMs(trail)).toBe(PRESENTATION_DWELL.other);
  });

  it('keeps a differing payload of the same kind as its own moment', () => {
    const trail = enqueue(createPresentationTrail(), [
      at(1, { kind: 'damage', target: { kind: 'player', player: 'p1' }, amount: 1 }),
      at(2, { kind: 'damage', target: { kind: 'player', player: 'p2' }, amount: 1 }),
    ]);
    expect(trail.queue).toHaveLength(2);
  });

  it('carries the server’s own aggregation count into the fold', () => {
    const trigger: MomentKind = { kind: 'life', player: 'p1', amount: -1 };
    const trail = enqueue(createPresentationTrail(), [
      at(1, trigger, { count: 4 }),
      at(2, trigger, { count: 2 }),
    ]);
    expect(trail.queue).toHaveLength(1);
    expect(trail.queue[0].moment.count).toBe(6);
  });

  it('collapses consecutive phase changes into one readable phase-trail update', () => {
    const trail = enqueue(createPresentationTrail(), [
      at(1, { kind: 'phase_change', phase: 'begin_combat' }),
      at(2, { kind: 'phase_change', phase: 'declare_attackers' }),
      at(3, { kind: 'phase_change', phase: 'declare_blockers' }),
    ]);
    expect(trail.queue).toHaveLength(1);
    // The latest phase is the one worth reading; the count says how far it moved.
    expect(trail.queue[0].moment.kind).toEqual({ kind: 'phase_change', phase: 'declare_blockers' });
    expect(trail.queue[0].moment.count).toBe(3);
  });

  it('never folds a moment whose kind this build does not know', () => {
    const unknown = (id: number): PresentationMoment => ({
      id,
      batch: 1,
      turn: 3,
      phase: 'precombat_main',
      count: 1,
      kindUnknown: true,
    });
    const trail = enqueue(createPresentationTrail(), [unknown(1), unknown(2)]);
    // It cannot say what they were, so it cannot say they were the same thing.
    expect(trail.queue).toHaveLength(2);
    expect(trail.queue.every((entry) => entry.dwellMs === PRESENTATION_DWELL.other)).toBe(true);
  });

  it('folds into the queue tail but never into the caption already on screen', () => {
    const trigger: MomentKind = { kind: 'drew', player: 'p1', count: 1 };
    const staged = advance(enqueue(createPresentationTrail(), [at(1, trigger)]), 0).trail;
    const grown = enqueue(staged, [at(2, trigger)]);
    expect(grown.staged?.moment.count).toBe(1);
    expect(grown.queue).toHaveLength(1);
  });
});

describe('presentation trail — dwell budgets (issue #594)', () => {
  it('reads every kind its documented budget', () => {
    expect(dwellMsFor(at(1, { kind: 'cast', player: 'p1', object: BOLT }))).toBe(380);
    expect(dwellMsFor(at(1, { kind: 'resolved', player: 'p1', object: BOLT }))).toBe(420);
    expect(dwellMsFor(at(1, { kind: 'countered', player: 'p1', object: BOLT }))).toBe(420);
    expect(dwellMsFor(at(1, { kind: 'fizzled', player: 'p1', object: BOLT }))).toBe(420);
    expect(dwellMsFor(at(1, { kind: 'died', object: BEAR }))).toBe(340);
    expect(
      dwellMsFor(at(1, { kind: 'zone_move', object: BEAR, from: 'battlefield', to: 'exile' })),
    ).toBe(340);
    expect(dwellMsFor(at(1, { kind: 'turn_change', turn: 4, active_player: 'p2' }))).toBe(500);
    expect(dwellMsFor(at(1, { kind: 'phase_change', phase: 'end' }))).toBe(260);
    expect(dwellMsFor(at(1, { kind: 'drew', player: 'p1', count: 2 }))).toBe(260);
  });

  it('gives one dwell to a phases_skipped group however many steps it names', () => {
    const one = at(1, {
      kind: 'phases_skipped',
      steps: [{ phase: 'upkeep', turn: 3 }],
      reason: 'no_response_available',
    });
    const many = at(2, {
      kind: 'phases_skipped',
      steps: [
        { phase: 'upkeep', turn: 3 },
        { phase: 'draw', turn: 3 },
        { phase: 'precombat_main', turn: 3 },
        { phase: 'begin_combat', turn: 3 },
        { phase: 'declare_attackers', turn: 3 },
      ],
      reason: 'forced_declaration',
    });
    expect(dwellMsFor(one)).toBe(PRESENTATION_DWELL.skipped);
    expect(dwellMsFor(many)).toBe(PRESENTATION_DWELL.skipped);
    // …and the group is one beat on the surface, not one per step.
    const trail = enqueue(createPresentationTrail(), [many]);
    expect(trail.queue).toHaveLength(1);
    expect(backlogMs(trail)).toBe(PRESENTATION_DWELL.skipped);
  });

  it('holds every travelling moment on screen at least as long as its travel takes', () => {
    // A caption that retires before its own zone travel lands would describe an
    // animation the reader is still watching (`presentation-budgets.md` §Animation).
    for (const dwell of [PRESENTATION_DWELL.cast, PRESENTATION_DWELL.zone]) {
      expect(dwell).toBeGreaterThanOrEqual(SCENE_MOTION.zoneTravel.ms);
    }
    expect(PRESENTATION_DWELL.resolution).toBeLessThanOrEqual(SCENE_MOTION.resolution.cap);
    expect(PRESENTATION_DWELL.turnChange).toBe(SCENE_MOTION.turnFlow.cap);
    // The compression floor still reads as its own beat, not a micro flash.
    expect(PRESENTATION_DWELL.floor).toBeGreaterThan(SCENE_MOTION.micro.cap);
    expect(PRESENTATION_DWELL.floor).toBeLessThan(PRESENTATION_DWELL.repeat);
  });
});

describe('presentation trail — advance (issue #594)', () => {
  it('stages the first moment immediately and reports when to wake', () => {
    const trail = enqueue(createPresentationTrail(), CHAIN);
    const step = advance(trail, 1000);
    expect(step.staged?.moment.id).toBe(10);
    expect(step.wakeAt).toBe(1000 + PRESENTATION_DWELL.cast);
    expect(step.trail.queue).toHaveLength(3);
  });

  it('is idempotent for a given now', () => {
    const trail = advance(enqueue(createPresentationTrail(), CHAIN), 1000).trail;
    const again = advance(trail, 1000);
    expect(again.trail).toBe(trail);
    expect(again.staged?.moment.id).toBe(10);
    expect(again.wakeAt).toBe(1000 + PRESENTATION_DWELL.cast);
  });

  it('retires a moment only once its dwell has elapsed', () => {
    const staged = advance(enqueue(createPresentationTrail(), CHAIN), 0).trail;
    const early = advance(staged, PRESENTATION_DWELL.cast - 1);
    expect(early.staged?.moment.id).toBe(10);
    const due = advance(staged, PRESENTATION_DWELL.cast);
    expect(due.staged?.moment.id).toBe(11);
    expect(due.wakeAt).toBe(PRESENTATION_DWELL.cast + PRESENTATION_DWELL.resolution);
  });

  it('runs late rather than skipping a beat when the caller wakes past the dwell', () => {
    // A backgrounded tab or a busy frame must not silently swallow moments: the
    // next one gets its full dwell from now. Catching up is compress/fastForward's job.
    const staged = advance(enqueue(createPresentationTrail(), CHAIN), 0).trail;
    const late = advance(staged, 10_000);
    expect(late.staged?.moment.id).toBe(11);
    expect(late.wakeAt).toBe(10_000 + PRESENTATION_DWELL.resolution);
  });

  it('goes quiet when the last moment retires', () => {
    const trail = enqueue(createPresentationTrail(), [
      at(1, { kind: 'drew', player: 'p1', count: 1 }),
    ]);
    const staged = advance(trail, 0);
    expect(staged.wakeAt).toBe(PRESENTATION_DWELL.other);
    const retired = advance(staged.trail, PRESENTATION_DWELL.other);
    expect(retired.staged).toBeNull();
    expect(retired.wakeAt).toBeNull();
    expect(isTrailIdle(retired.trail)).toBe(true);
    // Still idempotent once quiet.
    expect(advance(retired.trail, 99_999).trail).toBe(retired.trail);
  });
});

describe('presentation trail — the causal chain is never collapsed (issue #594)', () => {
  it('plays cast → resolved → died → zone move as four separate beats', () => {
    const played = playOut(enqueue(createPresentationTrail(), CHAIN));
    expect(tags(played)).toEqual(['cast', 'resolved', 'died', 'zone_move']);
    expect(played.every((entry) => entry.dwellMs > 0)).toBe(true);
  });

  it('keeps the chain whole and ordered under compression', () => {
    const noise: PresentationMoment[] = [];
    for (let i = 0; i < 24; i += 1) {
      noise.push(at(100 + i * 2, { kind: 'phase_change', phase: 'upkeep' }));
      noise.push(at(101 + i * 2, { kind: 'drew', player: 'p1', count: 1 }));
    }
    const trail = compress(enqueue(createPresentationTrail(), [...noise, ...chainFrom(900)]));
    expect(tags(trail.queue).filter((tag) => tag !== 'drew')).toEqual([
      'cast',
      'resolved',
      'died',
      'zone_move',
    ]);
    expect(trail.queue.every((entry) => entry.dwellMs > 0)).toBe(true);
  });

  it('keeps the chain whole with reduced motion on', () => {
    const trail = enqueue(createPresentationTrail({ reducedMotion: true }), CHAIN);
    expect(tags(playOut(trail))).toEqual(['cast', 'resolved', 'died', 'zone_move']);
  });
});

describe('presentation trail — compression (issue #594)', () => {
  const noisy = (): PresentationTrail => {
    const moments: PresentationMoment[] = [];
    for (let i = 0; i < 14; i += 1) {
      // Alternated so the phase captions cannot fold into one another on the way in.
      moments.push(at(200 + i * 2, { kind: 'phase_change', phase: 'upkeep' }));
      moments.push(at(201 + i * 2, { kind: 'drew', player: `p${i}`, count: 1 }));
    }
    return enqueue(createPresentationTrail(), [...moments, ...chainFrom(900)]);
  };

  it('leaves a backlog under the cap exactly as it is', () => {
    const trail = enqueue(createPresentationTrail(), CHAIN);
    expect(needsCompression(trail)).toBe(false);
    expect(compress(trail)).toBe(trail);
  });

  it('brings a compressible backlog back under the cap', () => {
    const trail = noisy();
    expect(backlogMs(trail)).toBeGreaterThan(PRESENTATION_BACKLOG_MS);
    const compressed = compress(trail);
    expect(backlogMs(compressed)).toBeLessThanOrEqual(PRESENTATION_BACKLOG_MS);
  });

  it('takes its cuts from phase changes and draws first', () => {
    const compressed = compress(noisy());
    expect(tags(compressed.queue)).not.toContain('phase_change');
    // Whatever survives of the chain kept its full dwell: nothing was hurried
    // that did not have to be.
    const cast = compressed.queue.find((entry) => entry.moment.kind?.kind === 'cast');
    expect(cast?.dwellMs).toBe(PRESENTATION_DWELL.cast);
  });

  it('never drops a cast, resolution, death, zone move, or turn change', () => {
    const chainOnly: PresentationMoment[] = [];
    for (let i = 0; i < 12; i += 1) {
      chainOnly.push(
        at(300 + i, { kind: 'cast', player: 'p1', object: object(`c${i}`, `Spell ${i}`) }),
      );
    }
    chainOnly.push(at(400, { kind: 'turn_change', turn: 4, active_player: 'p2' }));
    const compressed = compress(enqueue(createPresentationTrail(), chainOnly));
    // Every protected beat survives — even though the backlog stays over the cap.
    // A trail that runs long is a pacing problem; a trail that swallows a beat is
    // a correctness problem, because nothing else states the order these happened in.
    expect(compressed.queue).toHaveLength(13);
    expect(compressed.queue.every((entry) => isProtectedMoment(entry.moment))).toBe(true);
    expect(compressed.queue.every((entry) => entry.dwellMs >= PRESENTATION_DWELL.floor)).toBe(true);
  });

  it('shortens a repeat before it shortens anything unique', () => {
    // Eight aggregated damage runs (each already a single caption saying "×3")
    // ahead of one cast. A repeat has said what it has to say, so its dwell is
    // the first thing cut — and nothing is dropped.
    const moments: PresentationMoment[] = [];
    for (let i = 0; i < 8; i += 1) {
      moments.push(
        at(
          i,
          { kind: 'damage', target: { kind: 'player', player: `p${i}` }, amount: 1 },
          {
            count: 3,
          },
        ),
      );
    }
    moments.push(at(20, { kind: 'cast', player: 'p1', object: BOLT }));
    const compressed = compress(enqueue(createPresentationTrail(), moments));
    expect(compressed.queue).toHaveLength(9);
    // Every repeat is down to at most the repeat budget…
    expect(
      compressed.queue.every(
        (entry) => entry.moment.count === 1 || entry.dwellMs <= PRESENTATION_DWELL.repeat,
      ),
    ).toBe(true);
    // …the cutting stopped as soon as the cap was met, so later repeats still
    // hold the repeat budget rather than the floor…
    expect(compressed.queue.some((entry) => entry.dwellMs === PRESENTATION_DWELL.repeat)).toBe(
      true,
    );
    // …and the unique cast was never touched.
    expect(compressed.queue[8].dwellMs).toBe(PRESENTATION_DWELL.cast);
    expect(backlogMs(compressed)).toBeLessThanOrEqual(PRESENTATION_BACKLOG_MS);
  });
});

describe('presentation trail — fast forward (issue #594)', () => {
  it('empties the queue and parks the watermark at the newest id seen', () => {
    const trail = fastForward(enqueue(createPresentationTrail(), CHAIN));
    expect(isTrailIdle(trail)).toBe(true);
    expect(trail.watermark).toBe(13);
    expect(advance(trail, 0).staged).toBeNull();
  });

  it('makes a duplicate delivery of the same window a no-op', () => {
    // The reconnect path: the server re-sends the recent suffix, which is already
    // reflected in the view being rendered. Replaying it would narrate history the
    // player has stopped caring about.
    const parked = fastForward(enqueue(createPresentationTrail(), CHAIN));
    const redelivered = enqueue(parked, CHAIN);
    expect(redelivered).toBe(parked);
    expect(isTrailIdle(redelivered)).toBe(true);
  });

  it('still plays a later batch after the jump', () => {
    const parked = fastForward(enqueue(createPresentationTrail(), CHAIN));
    const next = enqueue(parked, [
      at(20, { kind: 'turn_change', turn: 4, active_player: 'p2' }, { batch: 2 }),
      at(21, { kind: 'cast', player: 'p2', object: BOLT }, { batch: 2 }),
    ]);
    expect(tags(playOut(next))).toEqual(['turn_change', 'cast']);
    expect(next.watermark).toBe(21);
  });

  it('drops the caption that was on screen when the decision arrived', () => {
    const staged = advance(enqueue(createPresentationTrail(), CHAIN), 0).trail;
    expect(staged.staged).not.toBeNull();
    const parked = fastForward(staged);
    expect(parked.staged).toBeNull();
    expect(advance(parked, 0).wakeAt).toBeNull();
  });
});

describe('presentation trail — reduced motion (issue #594)', () => {
  const window: PresentationMoment[] = [
    ...CHAIN,
    at(14, { kind: 'phase_change', phase: 'end' }),
    at(15, { kind: 'turn_change', turn: 4, active_player: 'p2' }),
  ];

  it('stages the identical sequence with reduced motion on and off', () => {
    // Reduced motion is not a request to read faster: same moments, same order,
    // same dwell. Only travel is removed (the ACTIVITY.dwellMs argument).
    const plain = playOut(enqueue(createPresentationTrail({ reducedMotion: false }), window));
    const reduced = playOut(enqueue(createPresentationTrail({ reducedMotion: true }), window));
    expect(tags(reduced)).toEqual(tags(plain));
    expect(reduced.map((entry) => entry.moment.id)).toEqual(plain.map((entry) => entry.moment.id));
    expect(reduced.map((entry) => entry.dwellMs)).toEqual(plain.map((entry) => entry.dwellMs));
  });

  it('suppresses travel only', () => {
    const plain = enqueue(createPresentationTrail(), window);
    const reduced = enqueue(createPresentationTrail({ reducedMotion: true }), window);
    // The travelling kinds travel at standard motion…
    expect(
      plain.queue.filter((entry) => entry.travel).map((entry) => entry.moment.kind?.kind),
    ).toEqual(['cast', 'died', 'zone_move']);
    // …and none of them do under reduced motion, while the captions are unchanged.
    expect(reduced.queue.some((entry) => entry.travel)).toBe(false);
    expect(tags(reduced.queue)).toEqual(tags(plain.queue));
  });

  it('applies a mid-match preference change to what has not played yet', () => {
    const plain = enqueue(createPresentationTrail(), window);
    const reduced = withReducedMotion(plain, true);
    expect(reduced.queue.some((entry) => entry.travel)).toBe(false);
    expect(tags(reduced.queue)).toEqual(tags(plain.queue));
    expect(withReducedMotion(reduced, true)).toBe(reduced);
    // …and back again, without disturbing the order or the dwell.
    const restored = withReducedMotion(reduced, false);
    expect(restored.queue.map((entry) => entry.travel)).toEqual(
      plain.queue.map((entry) => entry.travel),
    );
  });
});

describe('presentation trail — acceleration on a decision (issue #594)', () => {
  it('keeps the causal chain a decision arrived behind, floored rather than dropped', () => {
    // The headline failure this exists to prevent: the server settles a removal to
    // rest with the *caster* holding priority, so cast → resolved → died reaches the
    // helpless seat one broadcast before that seat's own decision. Discarding here
    // leaves the bare board diff — the creature gone, nothing saying what killed it.
    const trail = enqueue(createPresentationTrail(), chainFrom(10));
    const hurried = accelerate(trail);

    expect(tags(hurried.queue)).toEqual(['cast', 'resolved', 'died', 'zone_move']);
    expect(hurried.queue.every((entry) => entry.dwellMs === PRESENTATION_DWELL.floor)).toBe(true);
  });

  it('drops the low-value captions a decision has no time for', () => {
    const trail = enqueue(createPresentationTrail(), [
      at(20, { kind: 'phase_change', phase: 'upkeep' }),
      at(21, { kind: 'drew', player: 'p1', count: 1 }),
      ...chainFrom(22),
    ]);

    // A phase caption names a position `GameView.phase` already states, and a draw
    // is not worth a decision's clock. The chain is what survives.
    expect(tags(accelerate(trail).queue)).toEqual(['cast', 'resolved', 'died', 'zone_move']);
  });

  it('bounds even an all-chain backlog, dropping the oldest beats first', () => {
    // Five removals in one settle: twenty protected beats, floored, still over
    // budget. The newest are the ones explaining the decision now on the table.
    const many = [0, 1, 2, 3, 4].flatMap((n) => chainFrom(100 + n * 10));
    const hurried = accelerate(enqueue(createPresentationTrail(), many));

    expect(backlogMs(hurried)).toBeLessThanOrEqual(PRESENTATION_DECISION_MS);
    expect(hurried.queue.length).toBeGreaterThan(0);
    // The tail survived; the head is what was sacrificed.
    expect(tags(hurried.queue).at(-1)).toBe('zone_move');
    expect(hurried.queue.at(-1)?.moment.id).toBe(143);
  });

  it('leaves what is already on screen alone', () => {
    // Cutting a caption off under the reader's eye is the one thing the module
    // never does; the staged beat is bounded by a single dwell anyway.
    const played = advance(enqueue(createPresentationTrail(), chainFrom(30)), 0);
    const hurried = accelerate(played.trail);

    expect(hurried.staged).toBe(played.staged);
    expect(hurried.stagedUntil).toBe(played.trail.stagedUntil);
  });

  it('is a no-op on an idle trail', () => {
    const idle = createPresentationTrail();
    expect(accelerate(idle)).toBe(idle);
  });

  it('holds the watermark, so accelerating never re-admits what it cut', () => {
    const trail = enqueue(createPresentationTrail(), chainFrom(40));
    const hurried = accelerate(trail);

    expect(hurried.watermark).toBe(trail.watermark);
    expect(enqueue(hurried, chainFrom(40)).queue).toEqual(hurried.queue);
  });
});
