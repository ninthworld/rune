/**
 * The layered SVG placeholder for the battlefield environment
 * (`docs/design/environment-system.md` §10) — issue #530.
 *
 * **Production raster plates do not exist.** They are requested from the
 * maintainer in issue #548. What ships here is the complete environment in the
 * exact slots those plates will occupy: four layers, four DOM nodes, the same
 * manifest keys, the same `0 0 2333 1000` (21:9) authoring canvas, the same crop
 * anchors, the same focal-safe geometry, the same prop anchors, and the same
 * parallax factors — so the swap of §10.5 is a file drop plus a ledger entry
 * rather than a rework.
 *
 * §10.1 decides the placeholder's *form*, and the reasons bind: an `.svg` file
 * cannot import `sceneTokens.ts`, so committing one would put literal hex
 * outside the token layer; a committed file needs an ADR 0031 provenance story
 * for art that is really code; and a component costs **bundle** bytes, not asset
 * bytes, so the per-theme 1.5 MB ceiling keeps reading 0 KB until a real plate
 * lands. Every value below is therefore a `var(--env-*)` published by
 * `environmentScene.ts` from `SCENE_THEMES` — never a literal.
 *
 * These components stay in the tree after the raster swap: they are the T0 and
 * per-layer failure fallback of §8.3 and the Lite L0 treatment, both permanent.
 *
 * Nothing here is interactive. Every node is `aria-hidden` and inherits
 * `pointer-events: none` from the environment root (§7 rule 1).
 */
import { DEFAULT_STROKE, type GlyphElement } from '../../chrome/glyphs/geometry';
import { ENV_MEDALLION_GLYPH } from './medallion';
import { ENV_VIEWBOX } from './manifest';
import { ENV_MEDALLION, propRect, type FractionRect } from './zones';
import type { EnvPropEntry } from './manifest';

const { width: VB_W, height: VB_H } = ENV_VIEWBOX;

/** A canvas-fraction x to a `viewBox` x. */
const vx = (fraction: number): number => fraction * VB_W;
/** A canvas-fraction y to a `viewBox` y. */
const vy = (fraction: number): number => fraction * VB_H;

/** Shared props for one placeholder layer. */
export interface EnvLayerArtProps {
  /** The `viewBox` the aspect crop resolved to (§4.2). */
  viewBox: string;
  /** A gradient-id prefix, so two mounted environments never collide. */
  idPrefix: string;
}

/** One glyph primitive as an SVG element, tinted by `currentColor`. */
function glyphElement(el: GlyphElement, key: number) {
  const common = {
    stroke: 'currentColor',
    fill: 'fill' in el && el.fill === true ? 'currentColor' : 'none',
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  };
  switch (el.kind) {
    case 'circle':
      return <circle key={key} cx={el.cx} cy={el.cy} r={el.r} {...common} />;
    case 'polygon':
      return <polygon key={key} points={el.points.map((p) => p.join(',')).join(' ')} {...common} />;
    case 'polyline':
      return (
        <polyline key={key} points={el.points.map((p) => p.join(',')).join(' ')} {...common} />
      );
  }
}

/**
 * **L0 — far surround.** A radial `surroundTop → surroundBase`, two soft `water`
 * ellipses at the left and right margins, and one `glow` bloom at the horizon
 * (§10.3). Authored soft: the softness is baked into the shapes, never a CSS
 * filter — §1.2 bans runtime blur at every tier, for cost, legibility, and
 * motion sickness.
 */
export function EnvLayerL0({ viewBox, idPrefix }: EnvLayerArtProps) {
  const field = `${idPrefix}-l0-field`;
  const bloom = `${idPrefix}-l0-bloom`;
  const pool = `${idPrefix}-l0-pool`;
  return (
    <svg
      viewBox={viewBox}
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <radialGradient id={field} cx="50%" cy="42%" r="72%">
          <stop offset="0%" stopColor="var(--env-surround-top)" />
          <stop offset="100%" stopColor="var(--env-surround-base)" />
        </radialGradient>
        <radialGradient id={bloom}>
          <stop offset="0%" stopColor="var(--env-glow)" stopOpacity="0.34" />
          <stop offset="100%" stopColor="var(--env-glow)" stopOpacity="0" />
        </radialGradient>
        <radialGradient id={pool}>
          <stop offset="0%" stopColor="var(--env-water)" stopOpacity="0.92" />
          <stop offset="100%" stopColor="var(--env-water)" stopOpacity="0.18" />
        </radialGradient>
      </defs>
      <rect x="0" y="0" width={VB_W} height={VB_H} fill={`url(#${field})`} />
      {/* The two water bodies sit in the seat flanks and prop pockets, never in
          the focal core — streams and small waterfalls on both sides. */}
      <ellipse cx={vx(0.035)} cy={vy(0.52)} rx={vx(0.1)} ry={vy(0.46)} fill={`url(#${pool})`} />
      <ellipse cx={vx(0.965)} cy={vy(0.52)} rx={vx(0.1)} ry={vy(0.46)} fill={`url(#${pool})`} />
      {/* One horizon bloom, centred and low-amplitude: L0's internal local
          contrast is capped at 1.6:1 and its mean luminance sits a step below
          L1, so it can never compete with a card. */}
      <ellipse cx={vx(0.5)} cy={vy(0.06)} rx={vx(0.42)} ry={vy(0.16)} fill={`url(#${bloom})`} />
    </svg>
  );
}

/**
 * **L1 — arena floor.** The plaza ellipse in `plazaCore → plazaEdge`, three
 * concentric `paving` rings, a radial fan, and the medallion at `(50 %, 40 %)`
 * with `r = 5 % W` (§10.3).
 *
 * The two composition rules that make one source serve 2–6 seats (§3.2) are
 * literal here: the paving is **concentric rings plus a soft radial fan, never a
 * fixed N-way split**, and the bright field extends across the whole
 * Zone A ∪ Zone B envelope, so a 2-player table shows no empty painted seats and
 * a 6-player table never runs a board off the paving onto grass.
 *
 * L1 is the theme's identity floor and is the one layer never dropped: Lite
 * renders it at half resolution rather than collapsing to a gradient (§8.1).
 */
export function EnvLayerL1({ viewBox, idPrefix }: EnvLayerArtProps) {
  const plaza = `${idPrefix}-l1-plaza`;
  const rings = [0.9, 0.66, 0.42];
  // The radial fan: evenly spaced strokes from the medallion outward. Thirteen
  // is deliberately coprime with 2, 3, 4, 5, and 6, so no seat count can read
  // the fan as "its" division of the plaza — §3.2's ban on baked seat geometry
  // applied to the one L1 element that could imply it.
  const FAN_SPOKES = 13;
  const fan = Array.from({ length: FAN_SPOKES }, (_, i) => (i * Math.PI * 2) / FAN_SPOKES);
  const cx = vx(ENV_MEDALLION.cx);
  const cy = vy(ENV_MEDALLION.cy);
  return (
    <svg
      viewBox={viewBox}
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <radialGradient id={plaza} cx="50%" cy="40%" r="62%">
          <stop offset="0%" stopColor="var(--env-plaza-core)" />
          <stop offset="100%" stopColor="var(--env-plaza-edge)" />
        </radialGradient>
      </defs>
      {/* The plaza field. Wider than the canvas on purpose: it must still reach
          both edges at 21:9, where the crop reveals the outer 23.8 %. */}
      <ellipse cx={vx(0.5)} cy={vy(0.5)} rx={vx(0.72)} ry={vy(0.62)} fill={`url(#${plaza})`} />
      <g stroke="var(--env-paving)" fill="none" strokeOpacity="0.5">
        {fan.map((angle, i) => (
          <line
            key={`fan-${i}`}
            x1={cx + Math.cos(angle) * vx(0.06)}
            y1={cy + Math.sin(angle) * vx(0.06)}
            x2={cx + Math.cos(angle) * vx(0.7)}
            y2={cy + Math.sin(angle) * vx(0.7)}
            strokeWidth="2"
            strokeOpacity="0.22"
          />
        ))}
        {rings.map((scale, i) => (
          <ellipse
            key={`ring-${i}`}
            cx={vx(0.5)}
            cy={vy(0.5)}
            rx={vx(0.72) * scale}
            ry={vy(0.62) * scale}
            strokeWidth="3"
          />
        ))}
      </g>
      {/* The medallion — the only permitted L1 incident inside Zone A, capped at
          one contrast step above the plaza's own amplitude. It sits in the
          centre corridor, which by construction holds no card. */}
      <g
        transform={`translate(${cx - vx(ENV_MEDALLION.r)} ${cy - vx(ENV_MEDALLION.r)}) scale(${
          (vx(ENV_MEDALLION.r) * 2) / 24
        })`}
        color="var(--env-medallion)"
        strokeWidth={ENV_MEDALLION_GLYPH.strokeWidth ?? DEFAULT_STROKE}
        opacity="0.85"
      >
        {ENV_MEDALLION_GLYPH.elements.map((el, i) => glyphElement(el, i))}
      </g>
    </svg>
  );
}

/**
 * **L2 — arena edge.** The plaza rim stroke in `rim`, the verge fill in `verge`
 * outside it, and the two lip bands (§10.3).
 *
 * The lips are the §2.4 carve-out — the only content permitted to cross the
 * focal core, and only as a broad horizontal band inside `y ∈ [0 %, 8 %]` and
 * `y ∈ [92 %, 100 %]`. They carry the entire depth read, which is why §4.3
 * requires both to span even the tightest crop.
 *
 * `lipsOnly` is the §4.5 phone-portrait recomposition: the rim and verge drop
 * and the two lips re-anchor to canvas top and bottom rather than to source
 * coordinates.
 */
export function EnvLayerL2({
  viewBox,
  idPrefix,
  lipsOnly = false,
}: EnvLayerArtProps & { lipsOnly?: boolean }) {
  const verge = `${idPrefix}-l2-verge`;
  return (
    <svg
      viewBox={viewBox}
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <radialGradient id={verge} cx="50%" cy="50%" r="62%">
          <stop offset="86%" stopColor="var(--env-verge)" stopOpacity="0" />
          <stop offset="100%" stopColor="var(--env-verge)" stopOpacity="0.85" />
        </radialGradient>
      </defs>
      {!lipsOnly && (
        <>
          <rect x="0" y="0" width={VB_W} height={VB_H} fill={`url(#${verge})`} />
          <ellipse
            cx={vx(0.5)}
            cy={vy(0.5)}
            rx={vx(0.72)}
            ry={vy(0.62)}
            fill="none"
            stroke="var(--env-rim)"
            strokeWidth="7"
            strokeOpacity="0.7"
          />
        </>
      )}
      {/* The two raised lips, inside the §2.4 bands. Broad horizontals only: no
          vertical element, no silhouette taller than 8 % of H. */}
      <rect
        x="0"
        y="0"
        width={VB_W}
        height={vy(0.08)}
        fill="var(--env-rim)"
        fillOpacity="0.55"
        data-lip="top"
      />
      <rect
        x="0"
        y={vy(0.92)}
        width={VB_W}
        height={vy(0.08)}
        fill="var(--env-rim)"
        fillOpacity="0.55"
        data-lip="bottom"
      />
    </svg>
  );
}

/**
 * **L3 — props.** Corner- and edge-anchored silhouettes on the six §4.4 anchors:
 * warm lanterns on posts, cool crystal plinths.
 *
 * L3 is deliberately **not** a plate (§4.4). Baked at fixed source coordinates,
 * the 16:9 crop would discard every prop outside source `x ∈ [11.9 %, 88.1 %]` —
 * i.e. exactly the prop pockets — and 16:9 would have no visible scenery at all.
 * Anchoring in **composed-canvas** fractions solves it: a prop sits the same
 * distance from its corner at 16:9 and at 21:9, so ultrawide reveals more
 * surround while the props stay where the eye expects them.
 *
 * Because the anchors are canvas-relative rather than plate-relative, this layer
 * takes no `viewBox` crop: it draws in a `0 0 1000 1000` normalised box stretched
 * to the canvas, and each prop's rect comes straight from the manifest the
 * raster sprites will use.
 */
export function EnvLayerL3({ props: entries }: { props: readonly EnvPropEntry[] }) {
  return (
    <svg viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-hidden="true" focusable="false">
      {entries.map((entry) => (
        <EnvProp key={entry.key} entry={entry} />
      ))}
    </svg>
  );
}

/** One anchored prop silhouette, drawn inside its manifest rect. */
function EnvProp({ entry }: { entry: EnvPropEntry }) {
  const rect: FractionRect = propRect(entry.anchor, entry.offset, entry.size);
  const tone = entry.tone === 'warm' ? 'var(--env-prop-warm)' : 'var(--env-prop-cool)';
  const x = rect.x * 1000;
  const y = rect.y * 1000;
  const w = rect.w * 1000;
  const h = rect.h * 1000;
  const isLantern = entry.tone === 'warm';
  return (
    <g data-prop={entry.key} data-anchor={entry.anchor} data-mass={entry.mass} fill={tone}>
      {isLantern ? (
        <>
          {/* Post and lamp head: a clean silhouette, no light source painted —
              the glow accent belongs to L0 and to the §7.2 priority hook. */}
          <rect x={x + w * 0.42} y={y + h * 0.38} width={w * 0.16} height={h * 0.62} />
          <rect x={x + w * 0.24} y={y + h * 0.92} width={w * 0.52} height={h * 0.08} />
          <polygon
            points={[
              `${x + w * 0.5},${y}`,
              `${x + w * 0.86},${y + h * 0.18}`,
              `${x + w * 0.76},${y + h * 0.42}`,
              `${x + w * 0.24},${y + h * 0.42}`,
              `${x + w * 0.14},${y + h * 0.18}`,
            ].join(' ')}
          />
        </>
      ) : (
        <>
          {/* Crystal plinth: a faceted shard on a low base. */}
          <polygon
            points={[
              `${x + w * 0.5},${y}`,
              `${x + w},${y + h * 0.36}`,
              `${x + w * 0.78},${y + h * 0.86}`,
              `${x + w * 0.22},${y + h * 0.86}`,
              `${x},${y + h * 0.36}`,
            ].join(' ')}
            fillOpacity="0.82"
          />
          <rect x={x + w * 0.1} y={y + h * 0.86} width={w * 0.8} height={h * 0.14} />
        </>
      )}
    </g>
  );
}
