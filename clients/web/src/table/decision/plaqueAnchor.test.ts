/**
 * The anchoring algorithm's contract (`control-language.md` §10.1/D17 and §10.2,
 * issue #534).
 *
 * These tests exist because the plaque is the one surface that *moves*, and a
 * surface that follows its subject is one arithmetic slip away from covering it.
 * §10 opens with the constraint — the plaque must not occlude the subject or any
 * candidate — and #528 is the shipped proof that "it obviously won't" is not a
 * guarantee: chrome painted over a forced mulligan and left it unanswerable.
 *
 * Each `it` below names the numbered step or the §10.2 row it pins. The sweep at
 * the foot is the one that would catch a regression nobody thought to write a
 * case for: it walks the subject across the whole board and asserts the two
 * invariants at every position.
 *
 * jsdom proves the arithmetic, not the result. Whether the placement *looks*
 * right, whether the measured rects the shell feeds in are the rects the browser
 * actually lays out, and whether a click really passes through the plate to the
 * candidate underneath are all browser questions — see the note at the foot.
 */
import { describe, expect, it } from 'vitest';
import { rectsOverlap, type ShellRect } from '../live/shellLayout';
import {
  PLAQUE,
  clampToViewport,
  estimatePlaqueSize,
  placeDecisionPlaque,
  plaqueForm,
  type PlaqueAnchorInput,
} from './plaqueAnchor';

/** A 1440 × 900 desktop: the geometry §10.2's first four rows describe. */
const VIEWPORT: ShellRect = { x: 0, y: 0, w: 1440, h: 900 };
/** The scene band between the top bar and the hand. Its midline drives step 1. */
const BOARD: ShellRect = { x: 0, y: 56, w: 1440, h: 600 };
/** The lower-right cluster column: 268 wide, 28 from the edge (§3.3). */
const CLUSTER: ShellRect = { x: 1144, y: 640, w: 268, h: 232 };

const SIZE = estimatePlaqueSize(2);

function input(overrides: Partial<PlaqueAnchorInput> = {}): PlaqueAnchorInput {
  return {
    viewport: VIEWPORT,
    board: BOARD,
    cluster: CLUSTER,
    size: SIZE,
    seatCount: 2,
    ...overrides,
  };
}

/** Whether a rect keeps §10.1 step 5's ≥ 16 px gutter on all four sides. */
function insideGutter(rect: ShellRect, viewport: ShellRect = VIEWPORT): boolean {
  const g = PLAQUE.gutter;
  return (
    rect.x >= viewport.x + g &&
    rect.y >= viewport.y + g &&
    rect.x + rect.w <= viewport.x + viewport.w - g &&
    rect.y + rect.h <= viewport.y + viewport.h - g
  );
}

describe('estimatePlaqueSize', () => {
  it('never draws narrower than panel 7 (§3.1: ≥ 251)', () => {
    // One control does not shrink the plate below its drawn width — the title is
    // what sets the minimum, not the button row.
    expect(estimatePlaqueSize(1).w).toBe(PLAQUE.minW);
    expect(estimatePlaqueSize(0).w).toBe(PLAQUE.minW);
  });

  it('grows with the control row once it outruns the minimum', () => {
    expect(estimatePlaqueSize(3).w).toBeGreaterThan(estimatePlaqueSize(2).w);
    expect(estimatePlaqueSize(4).w).toBe(2 * PLAQUE.pad + 4 * 118 + 3 * PLAQUE.rowGap);
  });

  it('reserves the 44 px hit box, not the 36 px drawn plate (D2)', () => {
    // The transparent correction padding is real box. Sizing to the plate would
    // clip the very targets the correction exists to guarantee.
    expect(estimatePlaqueSize(2).h).toBe(2 * PLAQUE.pad + PLAQUE.titleH + PLAQUE.rowGap + 44);
  });
});

describe('plaqueForm (§10.2, last two rows)', () => {
  it.each([
    ['desktop', { w: 1440, h: 900 }, 'anchored'],
    ['tablet landscape at the floor', { w: 1180, h: 820 }, 'anchored'],
    ['landscape below the tablet floor', { w: 1024, h: 700 }, 'sheet'],
    ['tablet portrait', { w: 820, h: 1180 }, 'sheet'],
    ['phone portrait', { w: 390, h: 844 }, 'sheet'],
  ] as const)('%s takes the %s form', (_name, viewport, expected) => {
    expect(plaqueForm(viewport)).toBe(expected);
  });
});

describe('clampToViewport (§10.1 step 5)', () => {
  it('shrinks a plaque larger than the gutter-inset viewport before moving it', () => {
    const rect = clampToViewport(
      { x: -50, y: -50, w: 400, h: 400 },
      { x: 0, y: 0, w: 200, h: 200 },
    );
    expect(rect).toEqual({ x: 16, y: 16, w: 168, h: 168 });
  });

  it('honours a non-zero viewport origin (a safe-area inset)', () => {
    const rect = clampToViewport({ x: 0, y: 0, w: 100, h: 50 }, { x: 40, y: 30, w: 400, h: 300 });
    expect(rect.x).toBe(56);
    expect(rect.y).toBe(46);
  });
});

describe('placeDecisionPlaque — §10.1 step 1 (which side)', () => {
  it('places BELOW a subject in the board’s top half', () => {
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    const placement = placeDecisionPlaque(input({ subject }));
    expect(placement.side).toBe('below');
    expect(placement.form).toBe('anchored');
    expect(placement.rect.y).toBe(subject.y + subject.h + PLAQUE.subjectGap);
    // Centred on the subject until something pushes it off centre.
    expect(placement.rect.x).toBe(subject.x + subject.w / 2 - SIZE.w / 2);
    expect(placement.slide).toBe(0);
  });

  it('places ABOVE a subject in the board’s bottom half', () => {
    const subject: ShellRect = { x: 600, y: 500, w: 100, h: 140 };
    const placement = placeDecisionPlaque(input({ subject }));
    expect(placement.side).toBe('above');
    expect(placement.rect.y).toBe(subject.y - PLAQUE.subjectGap - SIZE.h);
  });

  it('takes the halves off the BOARD, not the viewport', () => {
    // The discriminating case: this subject's centre (400) is above the board's
    // midline (356 + …) but below the viewport's (450). Measuring the halves
    // against the viewport would flip the side, and the top bar and hand band —
    // which are not board — would decide where a board decision opens.
    const subject: ShellRect = { x: 600, y: 380, w: 100, h: 40 };
    expect(subject.y + subject.h / 2).toBeGreaterThan(BOARD.y + BOARD.h / 2);
    expect(subject.y + subject.h / 2).toBeLessThan(VIEWPORT.y + VIEWPORT.h / 2);
    expect(placeDecisionPlaque(input({ subject })).side).toBe('above');
  });
});

describe('placeDecisionPlaque — §10.1 steps 2 and 3 (reject, then slide)', () => {
  it('slides along the perpendicular axis until the rect is clear', () => {
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    // A candidate parked exactly where the centred position would go.
    const candidate: ShellRect = { x: 500, y: 250, w: 300, h: 120 };
    const placement = placeDecisionPlaque(input({ subject, candidates: [candidate] }));

    expect(placement.form).toBe('anchored');
    expect(placement.slide).not.toBe(0);
    expect(rectsOverlap(placement.rect, candidate)).toBe(false);
    expect(rectsOverlap(placement.rect, subject)).toBe(false);
  });

  it('slides in whole 16 px steps, right before left', () => {
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    const candidate: ShellRect = { x: 500, y: 250, w: 300, h: 120 };
    const placement = placeDecisionPlaque(input({ subject, candidates: [candidate] }));

    // Against the literal 16 the spec names, not against PLAQUE.slideStep —
    // asserting a constant against itself proves only that it equals itself.
    expect(PLAQUE.slideStep).toBe(16);
    expect(placement.slide % 16).toBe(0);
    // Right first: the first clear position at each step is taken, and the
    // rightward one is offered first, so the plaque lands to the right of the
    // blocking candidate rather than the left.
    expect(placement.slide).toBeGreaterThan(0);
    expect(placement.rect.x).toBeGreaterThanOrEqual(candidate.x + candidate.w);
  });

  it('never lets the viewport clamp push the plaque back onto the subject', () => {
    // The case the ordinary above/below gap hides: a subject near the top of a
    // shallow board takes the ABOVE side, the preferred y lands off-screen, and
    // step 5 clamps it back down — straight over the subject. Step 2 has to be
    // tested against the CLAMPED rect for this to be caught, which is why the
    // implementation clamps before it rejects.
    const board: ShellRect = { x: 0, y: 0, w: 1440, h: 200 };
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    const placement = placeDecisionPlaque(input({ board, subject }));

    expect(placement.side).toBe('above');
    expect(placement.rect.y).toBe(PLAQUE.gutter);
    expect(placement.slide).not.toBe(0);
    expect(rectsOverlap(placement.rect, subject)).toBe(false);
  });

  it('rejects every candidate rect, not just the first', () => {
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    const candidates: ShellRect[] = [
      { x: 500, y: 250, w: 300, h: 120 },
      { x: 800, y: 250, w: 300, h: 120 },
    ];
    const placement = placeDecisionPlaque(input({ subject, candidates }));
    for (const candidate of candidates) {
      expect(rectsOverlap(placement.rect, candidate)).toBe(false);
    }
  });
});

describe('placeDecisionPlaque — §10.1 step 4 (the dock)', () => {
  it('docks at the cluster when no slid position is ever clear', () => {
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    // A candidate band across the whole row: nothing at that y can be clear.
    const candidates: ShellRect[] = [{ x: -2000, y: 0, w: 6000, h: 900 }];
    const placement = placeDecisionPlaque(input({ subject, candidates }));

    expect(placement.form).toBe('docked');
    expect(placement.dockReason).toBe('no-clear-position');
    // Right-aligned in the cluster column, so the cluster's 28 px margin holds
    // whatever the plaque's width.
    expect(placement.rect.x + placement.rect.w).toBe(CLUSTER.x + CLUSTER.w);
    expect(placement.rect.y).toBe(CLUSTER.y);
  });

  it('docks with no subject to anchor to (a bare option prompt)', () => {
    const placement = placeDecisionPlaque(input({ subject: undefined }));
    expect(placement.form).toBe('docked');
    expect(placement.dockReason).toBe('no-subject');
  });

  it('clamps the docked rect into the viewport too', () => {
    const placement = placeDecisionPlaque(input({ cluster: { x: 1380, y: 880, w: 268, h: 232 } }));
    expect(insideGutter(placement.rect)).toBe(true);
  });
});

describe('placeDecisionPlaque — §10.2 placement per geometry', () => {
  it('anchors in the corridor at 2 and 3–4 seats', () => {
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    for (const seatCount of [2, 3, 4]) {
      expect(placeDecisionPlaque(input({ subject, seatCount })).form).toBe('anchored');
    }
  });

  it('ALWAYS docks at 5 and 6 seats, however clear the subject is', () => {
    // §10.2: the corridor is dense with combat paths at that count and the wings
    // are digested, so there is no near position worth trying. This is a rule
    // about the seat count, not a failure to find room — the subject below has
    // acres of it.
    const subject: ShellRect = { x: 600, y: 100, w: 100, h: 140 };
    for (const seatCount of [5, 6]) {
      const placement = placeDecisionPlaque(input({ subject, seatCount }));
      expect(placement.form).toBe('docked');
      expect(placement.dockReason).toBe('seat-count');
    }
  });

  it('docks when the subject sits on a wing board', () => {
    const wings: ShellRect[] = [{ x: 0, y: 120, w: 300, h: 400 }];
    const subject: ShellRect = { x: 40, y: 200, w: 120, h: 160 };
    const placement = placeDecisionPlaque(input({ subject, seatCount: 4, wings }));
    expect(placement.form).toBe('docked');
    expect(placement.dockReason).toBe('wing-subject');
  });

  it('never places the plaque inside a wing slot', () => {
    // The plain reading of §10.2's "never inside a wing slot": a wing carries an
    // opponent's crest cluster, which layout-model.md says can never degrade
    // away, so covering it during a combat declaration hides who is attacked.
    const wings: ShellRect[] = [{ x: 300, y: 240, w: 500, h: 300 }];
    const subject: ShellRect = { x: 480, y: 100, w: 100, h: 100 };
    const placement = placeDecisionPlaque(input({ subject, seatCount: 4, wings }));
    if (placement.form === 'anchored') {
      expect(rectsOverlap(placement.rect, wings[0]!)).toBe(false);
    }
  });
});

describe('placeDecisionPlaque — §10.2 the bottom sheet', () => {
  const PHONE: ShellRect = { x: 0, y: 0, w: 390, h: 844 };
  const RECEIVER: ShellRect = { x: 0, y: 600, w: 390, h: 244 };

  function phone(overrides: Partial<PlaqueAnchorInput> = {}): PlaqueAnchorInput {
    return input({
      viewport: PHONE,
      board: { x: 0, y: 54, w: 390, h: 546 },
      cluster: { x: 100, y: 700, w: 268, h: 120 },
      receiverBand: RECEIVER,
      ...overrides,
    });
  }

  it('is a full-width sheet at phone portrait, whatever the subject', () => {
    const placement = placeDecisionPlaque(phone({ subject: { x: 20, y: 80, w: 90, h: 130 } }));
    expect(placement.form).toBe('sheet');
    expect(placement.rect.x).toBe(PLAQUE.gutter);
    expect(placement.rect.w).toBe(PHONE.w - 2 * PLAQUE.gutter);
  });

  it('caps at 40 % of the viewport height', () => {
    const placement = placeDecisionPlaque(phone({ size: { w: 300, h: 900 } }));
    expect(placement.rect.h).toBeLessThanOrEqual(PHONE.h * PLAQUE.sheetMaxFraction);
    expect(placement.rect.h).toBe(Math.floor(PHONE.h * PLAQUE.sheetMaxFraction));
  });

  it('never covers the receiver’s band', () => {
    // The one thing §10.2 states outright about this form. The receiver's band
    // holds their own cards and crest; a sheet over it would hide the hand the
    // decision is being made from.
    for (const h of [60, 102, 400, 900]) {
      const placement = placeDecisionPlaque(phone({ size: { w: 300, h } }));
      expect(rectsOverlap(placement.rect, RECEIVER)).toBe(false);
      expect(placement.rect.y + placement.rect.h).toBeLessThanOrEqual(RECEIVER.y);
    }
  });

  it('still docks nothing and slides nothing — the sheet has one position', () => {
    const placement = placeDecisionPlaque(phone({ seatCount: 6 }));
    // Even the 5–6 seat dock rule yields: at this geometry there is no cluster,
    // the compact change-of-kind has engaged, and the sheet IS the home.
    expect(placement.form).toBe('sheet');
    expect(placement.slide).toBe(0);
  });
});

describe('placeDecisionPlaque — determinism and the standing invariants', () => {
  it('returns an identical rect for identical inputs', () => {
    // ui-requirements.md §Performance and determinism. A placement that drifts
    // is a placement that was measured rather than derived.
    const subject: ShellRect = { x: 617, y: 143, w: 116, h: 162 };
    const candidates: ShellRect[] = [
      { x: 500, y: 300, w: 260, h: 140 },
      { x: 790, y: 290, w: 200, h: 150 },
    ];
    const first = placeDecisionPlaque(input({ subject, candidates }));
    for (let i = 0; i < 5; i += 1) {
      expect(placeDecisionPlaque(input({ subject, candidates }))).toEqual(first);
    }
  });

  it('keeps the ≥ 16 px gutter and clears every blocker across the whole board', () => {
    // The sweep. Every step above pins one rule; this walks the subject across
    // the board and asserts the two that may never fail at any position.
    const candidates: ShellRect[] = [
      { x: 400, y: 260, w: 300, h: 130 },
      { x: 760, y: 420, w: 260, h: 130 },
    ];
    const wings: ShellRect[] = [{ x: 1180, y: 100, w: 260, h: 380 }];
    for (let x = 0; x <= 1340; x += 100) {
      for (let y = 60; y <= 600; y += 60) {
        const subject: ShellRect = { x, y, w: 100, h: 140 };
        const placement = placeDecisionPlaque(input({ subject, candidates, wings, seatCount: 4 }));
        const where = `subject ${x},${y} → ${placement.form}`;
        expect(insideGutter(placement.rect), where).toBe(true);
        if (placement.form !== 'anchored') continue;
        expect(rectsOverlap(placement.rect, subject), where).toBe(false);
        for (const candidate of candidates) {
          expect(rectsOverlap(placement.rect, candidate), where).toBe(false);
        }
        for (const wing of wings) {
          expect(rectsOverlap(placement.rect, wing), where).toBe(false);
        }
      }
    }
  });

  it('takes an explicit form override ahead of the derived one', () => {
    const placement = placeDecisionPlaque(
      input({ subject: { x: 600, y: 100, w: 100, h: 140 }, form: 'sheet' }),
    );
    expect(placement.form).toBe('sheet');
  });
});

/*
 * What jsdom cannot prove, and what belongs to the maintainer:
 *
 * - Whether the placement READS as attached to its subject — the arithmetic can
 *   only show the rects do not intersect, not that a player sees the plaque and
 *   the card as one thing.
 * - Whether the rects the shell measures and feeds in are the rects the browser
 *   lays out. Every rect here is synthetic; the real ones come from
 *   `getBoundingClientRect` through `LiveMatchTable.notePlaneGeometry`, and
 *   jsdom reports every element as 0 × 0.
 * - Whether a pointer really passes through the plate to a candidate underneath.
 *   jsdom does not hit-test, so `pointer-events` is asserted as a class and an
 *   attribute, never as behaviour.
 * - Whether the drawn plaque fits the estimated size at a real font and a real
 *   label length.
 */
