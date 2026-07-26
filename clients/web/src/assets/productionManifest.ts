/**
 * The typed reader for the **shipped** production presentation manifest
 * (`clients/web/public/assets/manifest.json`, issue #555 / request set #548).
 *
 * The manifest is the single source of truth for every content-hashed path the
 * client fetches. It is read here at **build time** (a JSON module import),
 * never duplicated into TypeScript, for three reasons that all bind:
 *
 * 1. **Hashes change on every regeneration.** A path transcribed into a `.ts`
 *    file is a future breakage the type system cannot catch; importing the file
 *    the generator writes means a re-run can never leave the client pointing at
 *    a deleted object.
 * 2. **The consumers are pure and synchronous.** `table/environment/manifest.ts`
 *    is a module-level constant table that `quality.ts` resolves against with no
 *    `await` anywhere on the path; the environment must also compose its first
 *    frame (T0) with zero fetches (`environment-system.md` §8.2). Fetching the
 *    manifest at runtime would make both asynchronous and would put a network
 *    round trip in front of the theme's identity.
 * 3. **It stays measurable.** The document budgets the per-theme manifest at
 *    4 KB of first-match *asset* bytes (§9.1); as a bundled module it is
 *    gzipped code bytes instead, measured against the stricter ≤ 1 MB
 *    interactive-code ceiling. The file still ships in `public/` because it is
 *    the generator's output and `src/assets/manifest.test.ts` reads it from
 *    disk — the duplication is ~2 KB and is the price of not hardcoding a hash.
 *
 * Nothing here fetches, caches, or remembers: it is a pure projection of a
 * committed file. The `portraits` section is deliberately **not** surfaced —
 * seat identity (issue #532) owns it.
 */
import manifestJson from '../../public/assets/manifest.json';

/** One rectangle inside a sprite atlas: `[x, y, width, height]` in atlas px. */
export type AtlasRect = readonly [number, number, number, number];

/** One frame of the L3 prop atlas, as the shipped manifest records it. */
export interface ProductionAtlasFrame {
  /** The frame's rect inside the atlas image. */
  rect: AtlasRect;
}

/** One shipped environment layer plate. */
export interface ProductionLayer {
  /** Absolute, content-hashed URL under the built root. */
  src: string;
  /** Intrinsic width in px. */
  width: number;
  /** Intrinsic height in px. */
  height: number;
  /** The §1.1 parallax factor the generator recorded for the layer. */
  parallax: number;
  /** Load class: `first-match` ships at the root, `lazy` under `lazy/`. */
  load: string;
  /** Present only on the L3 atlas. */
  frames?: Record<string, ProductionAtlasFrame>;
}

/** One shipped production environment: a complete, separable layer set. */
export interface ProductionEnvironment {
  /** Display label, mirrored from the token set. */
  label: string;
  /** Whether the theme ships production layers rather than a study. */
  production: boolean;
  /** The authoring aspect the plates were composed at. */
  authoringAspect: string;
  /** The §2.2 focal-safe rect, as the generator recorded it. */
  focalSafe: { x: number; y: number; width: number; height: number };
  /** The five layer plates, keyed as the shipped manifest keys them. */
  layers: Record<string, ProductionLayer>;
}

/**
 * One shipped **theme study**: a single flattened key-art plate, not a layer
 * set. What a study can and cannot do is stated where it is consumed
 * (`table/environment/manifest.ts`).
 */
export interface ProductionStudy {
  /** Display label. */
  label: string;
  /** Absolute, content-hashed URL. */
  src: string;
  /** Intrinsic width in px. */
  width: number;
  /** Intrinsic height in px. */
  height: number;
}

/** One card-back skin (`card-representation.md` §13.2). */
export interface ProductionCardBack {
  /** Display label. */
  label: string;
  /** Absolute, content-hashed URL. */
  src: string;
  /** Intrinsic width in px. */
  width: number;
  /** Intrinsic height in px. */
  height: number;
}

/**
 * One shipped card-frame plate (issue #570) — the material the Rune frame is
 * drawn from, as `card-representation.md` §3.12 specifies it.
 *
 * A plate is a nine-sliceable **alpha light map**: it carries bevel, shadow,
 * grain, and the structural gold hairline, and nothing else, so every body
 * colour still arrives from `src/tokens.ts` underneath it. That is what lets
 * one set serve every environment theme and every colour identity.
 */
export interface ProductionFramePlate {
  /** Absolute, content-hashed URL under the built root. */
  src: string;
  /** Intrinsic width in px. */
  width: number;
  /** Intrinsic height in px. */
  height: number;
  /** The nine-slice inset in authored px; `0` marks a plate that tiles. */
  slice: number;
  /**
   * The fraction of the card width `W` the drawn band occupies — resolved per
   * tier into `border-image-width` by `card/dom/plates.ts`. This is the whole
   * of the "one asset for every tier" contract: the band is a `calc()` on `W`,
   * so the drawn bevel is the authored ratio at the hand fan and at the chip.
   */
  band: number;
  /** Whether the middle patch paints (a printed surface) or drops out (a ring). */
  fill: boolean;
  /** Load class; every plate is `first-match` — the frame is on every card. */
  load: string;
  /** Tile size ÷ `W`, on the tiling plate only. */
  tile?: number;
}

/** The sections of the shipped manifest this module surfaces. */
interface ProductionManifest {
  version: number;
  environments: Record<string, ProductionEnvironment>;
  environmentStudies: Record<string, ProductionStudy>;
  cardBacks: { default: string; skins: Record<string, ProductionCardBack> };
  cardFrames: { plates: Record<string, ProductionFramePlate> };
}

/**
 * The manifest as data. The cast is the one unchecked step, and it is covered
 * by `src/assets/manifest.test.ts` (shape + every `src` exists on disk) and by
 * `productionManifest.test.ts` (every accessor below agrees with the file).
 */
const MANIFEST = manifestJson as unknown as ProductionManifest;

/** The manifest's schema version. */
export const PRODUCTION_MANIFEST_VERSION: number = MANIFEST.version;

/** The production environments, keyed by theme name. */
export const PRODUCTION_ENVIRONMENTS: Readonly<Record<string, ProductionEnvironment>> =
  MANIFEST.environments;

/** The theme studies, keyed by theme name. */
export const PRODUCTION_ENVIRONMENT_STUDIES: Readonly<Record<string, ProductionStudy>> =
  MANIFEST.environmentStudies;

/** Every shipped card-back skin, keyed by skin id. */
export const PRODUCTION_CARD_BACKS: Readonly<Record<string, ProductionCardBack>> =
  MANIFEST.cardBacks.skins;

/**
 * The id of the default card back — the one every hidden card falls back to
 * (`card-representation.md` §13.2, "a missing, malformed, or failed skin falls
 * back to the default **with no layout change**").
 */
export const PRODUCTION_CARD_BACK_DEFAULT: string = MANIFEST.cardBacks.default;

/** The shipped production layer set for a theme, or `undefined` for a study. */
export function productionEnvironment(theme: string): ProductionEnvironment | undefined {
  return PRODUCTION_ENVIRONMENTS[theme];
}

/** The shipped study plate for a theme, or `undefined` for a production theme. */
export function productionStudy(theme: string): ProductionStudy | undefined {
  return PRODUCTION_ENVIRONMENT_STUDIES[theme];
}

/** Every shipped card-frame plate, keyed as the generator keys it. */
export const PRODUCTION_FRAME_PLATES: Readonly<Record<string, ProductionFramePlate>> =
  MANIFEST.cardFrames.plates;

/** One shipped frame plate by key, or `undefined` when the key is unknown. */
export function productionFramePlate(key: string): ProductionFramePlate | undefined {
  return PRODUCTION_FRAME_PLATES[key];
}

/** One shipped card-back skin by id, or `undefined` when the id is unknown. */
export function productionCardBack(id: string): ProductionCardBack | undefined {
  return PRODUCTION_CARD_BACKS[id];
}
