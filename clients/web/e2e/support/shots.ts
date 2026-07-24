/**
 * The screenshot leg of the browser suite (ADR 0011 §Canvas assertion strategy).
 *
 * ADR 0011 is explicit that pixels are **secondary**: structured assertions on
 * the derived scene are the workhorse, and baselines are "opt-in and secondary …
 * never the sole assertion for a behavior that scene-data assertions can
 * express". This module honors that, and answers the one thing a structural
 * assertion cannot: *is the rendered composition stable, or does it churn?*
 *
 * **Why no committed baseline images.** A committed PNG is a comparison against
 * whatever machine produced it — a different font stack, freetype build, or GPU
 * on the runner re-renders every glyph, and the suite then fails for reasons that
 * have nothing to do with RUNE. So stability is proved *within the run*, on two
 * axes, both of which can genuinely fail:
 *
 * - {@link captureStable} — the same state, captured twice with real frames in
 *   between, must be byte-identical. An animation still in flight, a caret, a
 *   clock, or anything else non-deterministic in the composition fails here.
 * - {@link expectSameComposition} — the *same pinned state of two independent
 *   passes* (fresh server, fresh browser contexts, same seed) must be
 *   byte-identical. That is the "stable across two consecutive runs" property,
 *   asserted every run instead of hoped for across two of them.
 *
 * **What is in frame.** The staged scene plane — the board composition — with
 * the passive Pixi effects overlay masked out: that overlay is decorative, is
 * never load-bearing (`clients/web/AGENTS.md`), carries its own animation clock,
 * and its health is already asserted structurally by `render.ts`. The plane is
 * also the surface whose resting appearance is *motion-independent* by design
 * (`live-plane.module.css` reduces only transitions and the sky's ambient
 * animation, both of which a settled capture has already resolved), which is
 * what lets a full-motion pass and a reduced-motion pass be compared at all —
 * the shell's hand fan, by contrast, is deliberately flattened under reduced
 * motion and is therefore not in frame.
 */
import { expect, type Page, type TestInfo } from '@playwright/test';

/** How many painted frames to let pass between the two stability captures. */
const SETTLE_FRAMES = 12;

/** Wait for `SETTLE_FRAMES` real animation frames — a frame condition, not a sleep. */
async function letFramesPass(page: Page): Promise<void> {
  await page.evaluate(
    (frames) =>
      new Promise<void>((resolve) => {
        let left = frames;
        const step = (): void => {
          left -= 1;
          if (left <= 0) resolve();
          else requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      }),
    SETTLE_FRAMES,
  );
}

/** Screenshot the staged scene plane, with the decorative effects layer masked. */
async function shoot(page: Page): Promise<Buffer> {
  return page.getByTestId('live-2-5d-plane').screenshot({
    animations: 'disabled',
    caret: 'hide',
    mask: [page.getByTestId('effects-surface')],
  });
}

/**
 * Capture one pinned state twice, `SETTLE_FRAMES` apart, and require the two
 * captures to be identical — the composition is not still moving, and nothing in
 * it is time-dependent. Attaches the image to the report under `name`.
 */
export async function captureStable(page: Page, name: string, info: TestInfo): Promise<Buffer> {
  const first = await shoot(page);
  await letFramesPass(page);
  const second = await shoot(page);
  await info.attach(`${name}.png`, { body: second, contentType: 'image/png' });
  expect(
    first.equals(second),
    `${name}: the composition should be settled — two captures ${SETTLE_FRAMES} frames apart differ`,
  ).toBe(true);
  return second;
}

/**
 * Require the same pinned state of two independent passes to render identically.
 * Attaches both on failure so the churn is inspectable rather than a bare
 * boolean.
 */
export async function expectSameComposition(
  name: string,
  first: Buffer,
  second: Buffer,
  info: TestInfo,
): Promise<void> {
  if (!first.equals(second)) {
    await info.attach(`${name}-pass-1.png`, { body: first, contentType: 'image/png' });
    await info.attach(`${name}-pass-2.png`, { body: second, contentType: 'image/png' });
  }
  expect(
    first.equals(second),
    `${name}: two consecutive runs of the same seeded scenario should render identically`,
  ).toBe(true);
}
