/**
 * Gates for the pregame scene-token wiring (issue #506,
 * `docs/design/front-door-and-lobby.md` §8 criteria 2–6, 15, 16).
 *
 * These pin the things a restyle can silently break: that the pregame surfaces
 * read the SCENE tokens rather than inventing values, that the motion classes
 * stay inside their budget windows and collapse to zero under reduced motion,
 * that the quality tier maps to the same ambient levels the match uses, and —
 * the one that matters most for identity — that the accent a seat wears in the
 * room is the accent that seat wears in the match.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_SCENE_THEME,
  SCENE_ELEVATION,
  SCENE_HUES,
  SCENE_MOTION,
  SCENE_NEUTRALS,
  SCENE_SEAT_ACCENTS,
  SCENE_THEMES,
} from '../sceneTokens';
import { pregamePlace, pregameSceneVars, seatAccent, seatAccentVars } from './pregameScene';

/** The match's own seat-accent derivation (`table/live/gameViewPresentation.ts`). */
function matchSeatAccent(seatOrder: readonly string[], player: string): string {
  const index = Math.max(0, seatOrder.indexOf(player));
  return SCENE_SEAT_ACCENTS[index % SCENE_SEAT_ACCENTS.length]!;
}

describe('pregameSceneVars — criterion 3: every value comes from sceneTokens', () => {
  const vars = pregameSceneVars(false);

  it('publishes the foundation neutrals, hues, and elevation ladder verbatim', () => {
    expect(vars['--pregame-raised']).toBe(SCENE_NEUTRALS.raised);
    expect(vars['--pregame-surface']).toBe(SCENE_NEUTRALS.surfaceBase);
    expect(vars['--pregame-surface-top']).toBe(SCENE_NEUTRALS.surfaceTop);
    expect(vars['--pregame-ink']).toBe(SCENE_NEUTRALS.ink);
    expect(vars['--pregame-line']).toBe(SCENE_NEUTRALS.lineFaint);
    expect(vars['--pregame-line-strong']).toBe(SCENE_NEUTRALS.lineStrong);
    expect(vars['--pregame-text']).toBe(SCENE_NEUTRALS.text);
    expect(vars['--pregame-text-muted']).toBe(SCENE_NEUTRALS.textMuted);

    expect(vars['--pregame-gold']).toBe(SCENE_HUES.gold.value);
    expect(vars['--pregame-blue']).toBe(SCENE_HUES.blue.value);
    expect(vars['--pregame-red']).toBe(SCENE_HUES.red.value);
    expect(vars['--pregame-green']).toBe(SCENE_HUES.green.value);

    expect(vars['--pregame-elev-rest']).toBe(SCENE_ELEVATION.rest.shadow);
    expect(vars['--pregame-elev-lifted']).toBe(SCENE_ELEVATION.lifted.shadow);
    expect(vars['--pregame-elev-held']).toBe(SCENE_ELEVATION.held.shadow);
    expect(vars['--pregame-elev-screen']).toBe(SCENE_ELEVATION.screen.shadow);
  });

  it('publishes the default theme’s ambient accent, and no backdrop slots (criterion 2)', () => {
    // The backdrop composition moved to the shared `table/environment` stack
    // (issue #530), which publishes its own `--env-*` properties from the same
    // token set. The pregame keeps only the accent its places read, so the two
    // surfaces cannot drift — there is one environment, not a copy of one.
    const theme = SCENE_THEMES[DEFAULT_SCENE_THEME];
    expect(vars['--pregame-glow']).toBe(theme.glow);
    for (const retired of [
      '--pregame-sky-top',
      '--pregame-sky-horizon',
      '--pregame-sky-base',
      '--pregame-far-ground',
      '--pregame-arena',
    ] as const) {
      expect(vars[retired]).toBeUndefined();
    }
  });

  it('publishes all six seat accents', () => {
    for (const [index, accent] of SCENE_SEAT_ACCENTS.entries()) {
      expect(vars[`--pregame-seat-${index}`]).toBe(accent);
    }
  });
});

describe('pregame motion — criteria 15 and 16', () => {
  it('runs place changes at `staging` and everything else at `micro`, inside the caps', () => {
    const vars = pregameSceneVars(false);
    expect(vars['--pregame-motion-staging']).toBe(`${SCENE_MOTION.staging.ms}ms`);
    expect(vars['--pregame-motion-micro']).toBe(`${SCENE_MOTION.micro.ms}ms`);
    expect(vars['--pregame-motion-reject']).toBe(`${SCENE_MOTION.tapUntap.ms}ms`);
    // The budget windows the classes must stay inside.
    expect(SCENE_MOTION.staging.ms).toBeLessThanOrEqual(500);
    expect(SCENE_MOTION.micro.ms).toBeLessThanOrEqual(150);
    // The rejected-command shake's ≤ 200 ms window (§5.8 last row).
    expect(SCENE_MOTION.tapUntap.ms).toBeLessThanOrEqual(200);
    // No pregame sequence composes past 600 ms: the longest is one place change
    // followed by the destination's own micro settle.
    expect(SCENE_MOTION.staging.ms + SCENE_MOTION.micro.ms).toBeLessThanOrEqual(600);
    // Easings come from the token set, never from a literal curve.
    expect(vars['--pregame-ease-staging']).toBe(SCENE_MOTION.staging.ease);
    expect(vars['--pregame-ease-micro']).toBe(SCENE_MOTION.micro.ease);
    expect(vars['--pregame-ease-reject']).toBe(SCENE_MOTION.tapUntap.ease);
  });

  it('collapses every duration to zero under reduced motion, changing nothing else', () => {
    const full = pregameSceneVars(false);
    const reduced = pregameSceneVars(true);
    for (const key of ['staging', 'micro', 'reject']) {
      expect(reduced[`--pregame-motion-${key}`]).toBe('0ms');
    }
    // Only the durations differ: every color, shadow, theme slot, and easing is
    // identical, so reduced motion is a pure motion collapse with no layout or
    // state difference.
    const nonMotion = (vars: object): Record<string, unknown> =>
      Object.fromEntries(
        Object.entries(vars).filter(([key]) => !key.startsWith('--pregame-motion')),
      );
    expect(nonMotion(reduced)).toEqual(nonMotion(full));
  });
});

// The environment tier that criterion 4 pins is now owned by the ONE shared
// environment system (`table/environment/quality.ts` `ambientLevel`) and gated
// by `environment.test.ts`. There is deliberately no pregame-local copy of that
// rule left to drift from the match's.

describe('seat identity — criterion 6: one accent, taught once', () => {
  it('gives a room seat the accent that seat wears in the match', () => {
    // The server builds `GameView.seat_order` in room-seat order
    // (`crates/rune-server/src/view.rs`), so for one room composition the two
    // index mappings must agree seat for seat. If they ever diverge this fails
    // rather than the mismatch being papered over in CSS.
    const seatOrder = ['p0', 'p1', 'p2', 'p3', 'p4', 'p5', 'p6'];
    for (const [roomSeat, player] of seatOrder.entries()) {
      expect(seatAccent(roomSeat)).toBe(matchSeatAccent(seatOrder, player));
    }
  });

  it('cycles the six accents and exposes the accent as a custom property', () => {
    expect(seatAccent(0)).toBe(SCENE_SEAT_ACCENTS[0]);
    expect(seatAccent(6)).toBe(SCENE_SEAT_ACCENTS[0]);
    expect(seatAccent(7)).toBe(SCENE_SEAT_ACCENTS[1]);
    expect(seatAccentVars(2)['--pregame-accent']).toBe(SCENE_SEAT_ACCENTS[2]);
  });
});

describe('pregamePlace — the flow is derived, never stored', () => {
  it('names the place from the socket status plus the latest view alone', () => {
    expect(pregamePlace('idle', false, false)).toBe('front-door');
    expect(pregamePlace('connecting', false, false)).toBe('front-door');
    expect(pregamePlace('closed', false, false)).toBe('front-door');
    expect(pregamePlace('open', false, false)).toBe('lobby');
    expect(pregamePlace('open', false, true)).toBe('lobby');
    expect(pregamePlace('open', true, true)).toBe('room');
  });
});

describe('pregame CSS — criterion 3: no literal hex or duration', () => {
  // Read from disk: the point of this gate is the CSS *source*, not the
  // compiled module. Vitest runs with `clients/web` as its working directory.
  // Both pregame stylesheets are scanned — the split is for file size only.
  const css = ['pregame.module.css', 'pregamePlaces.module.css']
    .map((file) => readFileSync(resolve(process.cwd(), 'src/pregame', file), 'utf8'))
    .join('\n');

  it('introduces no literal color', () => {
    // Comments carry prose, not values; strip them before scanning.
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(rules.match(/#[0-9a-fA-F]{3,8}\b/g)).toBeNull();
  });

  it('introduces no literal duration — every one resolves through a token', () => {
    const rules = css.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(rules.match(/\b\d+(\.\d+)?m?s\b/g)).toBeNull();
    // …and the durations it does use are the pregame motion tokens.
    expect(rules).toContain('var(--pregame-motion-staging)');
    expect(rules).toContain('var(--pregame-motion-micro)');
  });

  it('retires the carved-panel treatment (criterion 1)', () => {
    // The superseded language was a line-work border with accent corner
    // notches drawn as ::before/::after boxes with `--panel-accent`.
    expect(css).not.toContain('--panel-accent');
    expect(css).not.toContain('border-top-left-radius');
    expect(css).not.toContain('border-bottom-right-radius');
  });
});

/**
 * The ground plate reaches the screen (issue #566).
 *
 * `sceneTokens.test.ts` proves the plate's *value* clears the contrast floor
 * over every slot of every theme. That is only worth anything while the text
 * the issue names is actually drawn on it, which is what this pins — jsdom
 * computes no layout and no `var()`, so structure is the honest thing to check.
 */
describe('pregame CSS — arena text sits on the ground plate', () => {
  const files = ['pregame.module.css', 'pregamePlaces.module.css'].map((file) =>
    readFileSync(resolve(process.cwd(), 'src/pregame', file), 'utf8'),
  );
  const css = files.join('\n');

  it('publishes the plate as a scene token, never as a local value', () => {
    expect(pregameSceneVars(false)['--pregame-ground']).toBe('rgb(13 15 19 / 78.0%)');
    expect(css).toContain('background: var(--pregame-ground);');
  });

  it('draws it as a veil: uniform under the text, dissolving outward, no edge', () => {
    const ground = /\.ground \{([^}]*)\}/.exec(files[0]!)?.[1] ?? '';
    expect(ground, '.ground has no rule').not.toBe('');
    // Uniform across the padded box — which is what makes "every descendant sits
    // on at least the composite" true rather than approximately true.
    expect(ground).toContain('background: var(--pregame-ground);');
    // The halo is the same colour spreading outward, so there is no edge to read
    // as a carved panel. A border here would bring #506's treatment back.
    expect(ground).toContain('box-shadow: 0 0');
    expect(ground).not.toContain('border:');
    // Text may not reach the plate's own corner.
    expect(ground).toMatch(/padding: var\(--rune-space-\d+\) var\(--rune-space-\d+\);/);
  });

  it('puts every block the issue named on it', () => {
    // Point for point against #566's list: the RUNE lockup (both the landing
    // column and the corner mark), "Ready to play" / "Change server" (the door
    // column), the lobby's "Open games" heading and its empty-state sentence.
    for (const selector of ['.lockup', '.arenaTitle', '.kicker']) {
      const block = /\{([^}]*)\}/.exec(css.slice(css.indexOf(`${selector} {`)))?.[1] ?? '';
      expect(block, `${selector} does not compose the ground plate`).toContain('composes: ground');
    }
    for (const selector of ['.doorColumn', '.emptyGames', '.ribbon', '.identityRow']) {
      const block = /\{([^}]*)\}/.exec(css.slice(css.indexOf(`${selector} {`)))?.[1] ?? '';
      expect(block, `${selector} does not compose the ground plate`).toContain(
        "composes: ground from './pregame.module.css'",
      );
    }
  });
});

describe('ready room CSS — the seating ring cannot collide with itself', () => {
  const css = readFileSync(resolve(process.cwd(), 'src/pregame/pregamePlaces.module.css'), 'utf8');

  /** The declaration block of one selector, comments stripped. */
  function rule(selector: string): string {
    const at = css.indexOf(`${selector} {`);
    expect(at, `${selector} has no rule`).toBeGreaterThanOrEqual(0);
    return (/\{([^}]*)\}/.exec(css.slice(at))?.[1] ?? '').replace(/\/\*[\s\S]*?\*\//g, '');
  }

  /**
   * The seat's `transform` (its centring on the ring point) makes it a stacking
   * context, so the popover's `--rune-z-popover` is scoped inside the seat that
   * opened it. Sibling seats and the centre plaque, both untouched, then paint
   * over the popover by document order alone — the shipped bug was an invite
   * panel opening underneath the host's own seat. The rung has to be worn by
   * the seat, which is the element competing with them.
   */
  it('lifts the whole seat when its options are open, not just the panel', () => {
    expect(rule('.ringSeat')).toContain('transform: translate(-50%, -50%);');
    expect(rule('.ringSeat:has(.seatOptions)')).toContain('z-index: var(--rune-z-popover);');
  });

  it('ranks every seat above the room’s own status plaque', () => {
    // The centre is drawn last, so without an explicit rung it wins on document
    // order and paints "everyone's here" across a seat.
    const ladder = readFileSync(resolve(process.cwd(), 'src/chrome/tokens.css'), 'utf8');
    const rung = (name: string): number => {
      const found = new RegExp(`--rune-z-${name}:\\s*(-?\\d+);`).exec(ladder)?.[1];
      expect(found, `--rune-z-${name} is not declared`).toBeDefined();
      return Number(found);
    };
    const seat = /z-index: var\(--rune-z-([a-z-]+)\);/.exec(rule('.ringSeat'))?.[1] ?? '';
    const centre = /z-index: var\(--rune-z-([a-z-]+)\);/.exec(rule('.ringCentre'))?.[1] ?? '';
    expect(rung(seat)).toBeGreaterThan(rung(centre));
  });

  /**
   * Both a seat and the centre claim half their height either side of their
   * point, and `seatRing.RING_RY` sets the distance between those points. The
   * ring's floor is what keeps the two bands apart; at the shipped `* 1.5` they
   * intersected. Whether the drawn spacing is right at 2–8 seats is a browser
   * check — this only holds the floor that makes it possible.
   */
  it('floors the ring high enough for a seat and the centre to clear', () => {
    const ring = rule('.ring');
    const floor = /min-height: calc\(var\(--rune-menu-w-cluster\) \* ([\d.]+)\)/.exec(ring)?.[1];
    expect(floor, '.ring states no min-height in cluster units').toBeDefined();
    expect(Number(floor)).toBeGreaterThanOrEqual(2.4);
    // The floor is stated against the MENU cluster width, not the match's, so it
    // tracks the rung a seat's own controls are drawn at (issue #566). A floor in
    // unscaled units would fall behind the seats the moment the rung grew them.
    expect(ring).toContain('--rune-control-scale: var(--rune-menu-scale-dense);');
  });

  /**
   * Issue #566: "the ready room's ring in particular claims the arena rather
   * than a box in the centre of it". The shipped ring was a fixed 16:9 box
   * capped at four cluster widths, so at the 1440×900 reference it drew
   * 1072 × 643 inside a ~1358 × 690 arena and left the rest of the plaza empty.
   */
  it('claims the whole arena instead of a fixed box centred in it', () => {
    const ring = rule('.ring');
    // Full width of the arena column, and every row the frame does not use.
    expect(ring).toContain('width: 100%;');
    expect(ring).toContain('align-self: stretch;');
    expect(ring).toMatch(/flex: 1 1 auto;/);
    // No fixed cap and no fixed aspect: both are what pinned it to a box.
    expect(ring).not.toContain('max-width:');
    expect(ring).not.toContain('aspect-ratio:');
  });
});
