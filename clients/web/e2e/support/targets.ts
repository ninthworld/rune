/**
 * The two client targets the suite serves, shared by `playwright.config.ts` and
 * the specs so a port lives in exactly one place (ADR 0011).
 *
 * **Why two.** ADR 0011 specifies the *production preview* build as what the
 * suite runs against — the artifact that actually ships. The smoke canary is the
 * one deliberate exception: the regression it exists for is React StrictMode's
 * development-only double-invoke of effects, which a production bundle never
 * performs, so a preview-only canary could not see its own bug. Everything else
 * belongs on the preview build, and the four-player slice is emphatically
 * everything else: on the dev server its four browser contexts each pay for the
 * unbundled module graph and a development React, which is minutes of wall clock
 * spent on nothing the spec asserts.
 *
 * Both targets are built/served with `VITE_RUNE_TEST_HOOKS` set, which is what
 * compiles in the read-only `window.__RUNE_TEST__` surface.
 *
 * Neither port is a Vite default, so a developer's own `npm run dev` is never
 * mistaken for the suite's server (and vice versa).
 */

/** Port for the Vite **dev** server the smoke canary drives. */
export const DEV_PORT = Number(process.env.RUNE_E2E_PORT ?? 5179);

/** Port for the **preview** server (a real production build) the slice drives. */
export const PREVIEW_PORT = Number(process.env.RUNE_E2E_PREVIEW_PORT ?? 5180);

/** Origin of the dev-server target. */
export const DEV_URL = `http://127.0.0.1:${DEV_PORT}`;

/** Origin of the preview-build target. */
export const PREVIEW_URL = `http://127.0.0.1:${PREVIEW_PORT}`;

/**
 * Where the preview build is emitted. Deliberately **not** `dist/`: `npm run
 * budget` measures `dist/` against the shipped load ceilings, and a hooks-enabled
 * bundle is not the artifact that budget is about.
 */
export const PREVIEW_OUT_DIR = 'dist-e2e';
