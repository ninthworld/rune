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
import {
  pregameEnvironmentMotion,
  pregamePlace,
  pregameSceneVars,
  seatAccent,
  seatAccentVars,
} from './pregameScene';

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

  it('publishes the default theme’s six environment slots (criterion 2)', () => {
    const theme = SCENE_THEMES[DEFAULT_SCENE_THEME];
    expect(vars['--pregame-sky-top']).toBe(theme.skyTop);
    expect(vars['--pregame-sky-horizon']).toBe(theme.skyHorizon);
    expect(vars['--pregame-sky-base']).toBe(theme.skyBase);
    expect(vars['--pregame-far-ground']).toBe(theme.ground);
    expect(vars['--pregame-arena']).toBe(theme.arena);
    expect(vars['--pregame-glow']).toBe(theme.glow);
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

describe('pregame environment tier — criterion 4', () => {
  it('steps on → reduced → off across quality levels, exactly like the match', () => {
    expect(pregameEnvironmentMotion('high', false)).toBe('on');
    expect(pregameEnvironmentMotion('standard', false)).toBe('reduced');
    expect(pregameEnvironmentMotion('lite', false)).toBe('off');
  });

  it('turns ambient drift off at any level under reduced motion', () => {
    for (const quality of ['high', 'standard', 'lite'] as const) {
      expect(pregameEnvironmentMotion(quality, true)).toBe('off');
    }
  });
});

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
