/**
 * The DOM card face (issue #479, ADR 0030 layer 2) — the ONE card renderer for
 * every card surface of the 2.5D client, dissolving ADR 0003's two-renderer
 * split. It renders every tier of the carried information budget (chip → mini →
 * support → field → hand → inspect; `docs/design/ui-design-notes.md` §Card
 * render stays authoritative) from the same {@link CardDisplayData} the Pixi
 * factory consumes, so the GameView → display mapping is shared unchanged.
 *
 * Design rules, carried from the factory:
 * - **No game logic.** Every value renders exactly as supplied.
 * - **No literals.** Colors and sizes flow from `src/tokens.ts` through
 *   {@link cardFaceVars} as CSS custom properties (ADR 0019); the provisional
 *   elevation/motion seeds live in `theme.ts` as the #480 swap point.
 * - **Budget-shaped DOM** (presentation-budgets §Performance: ≤ 12 nodes per
 *   battlefield-tier face — a hard, input-independent ceiling): every state
 *   channel is zero-node — rings are box-shadows, edge bars and the monogram
 *   and the ability marker are pseudo-elements, tap/dim/elevation are
 *   transform + opacity, the ×N splay is layered box-shadow — and no element
 *   scales with its input: the keyword strip is one `<svg>` with combined
 *   paths and a capped `+N`, the mana cost is one bounded pill, and every
 *   badge consolidates into one row (the colored per-symbol/per-badge
 *   rendering stays at the budget-exempt hand/inspect tiers).
 * - **Transform readiness** (ADR 0030): the face renders correctly flat (chrome
 *   surfaces) and on the perspective plane; all state transitions are
 *   transform/opacity-only and `prefers-reduced-motion` snaps them.
 *
 * The Pixi factory stays untouched and shipping until Phase 2 retires it; no
 * production surface swaps renderers here (the fixture battlefield, #483, is
 * the visual consumer).
 */
import type { CSSProperties } from 'react';
import { parseManaCost, type CardDisplayData } from '../cardFactory';
import { keywordGlyphName, type GlyphName } from '../../chrome/glyphs';
import { cx } from '../../chrome/cx';
import { glyphStripGeometry } from './glyphStrip';
import { cardFaceVars, faceMetrics, type CardFaceTier } from './theme';
import s from './card-face.module.css';

/** The elevation ladder of visual-system §3: resting on the plane, lifted by
 * hover/keyboard focus, held while selected/dragged/cast. */
export type CardElevation = 'rest' | 'lifted' | 'held';

/** An illustration already published by the player-side art store (ADR 0024).
 * The face never fetches: the consumer resolves `artKey` → object URL and mode.
 * `full` replaces the whole face (Scryfall full-card style); otherwise the
 * image fills the art window at the window tiers, budget unchanged. */
export interface CardFaceArt {
  /** Object/asset URL of the already-loaded image. */
  url: string;
  /** Whether the image is a full-card face rather than a window illustration. */
  full?: boolean;
}

/** Props for {@link CardFace}. */
export interface CardFaceProps {
  /** The shared display description — same data contract as the Pixi factory. */
  data: CardDisplayData;
  /** Size tier (defaults to `field`). */
  tier?: CardFaceTier;
  /** Elevation ladder state (defaults to `rest`). */
  elevation?: CardElevation;
  /** Player-side illustration, if the art store has one published (ADR 0024). */
  art?: CardFaceArt;
  /** Server-supplied rules text — rendered at the `inspect` tier only (its
   * budget is "everything the server supplies"). */
  rulesText?: string;
  /** Extra class on the root (positioning is the consumer's job). */
  className?: string;
}

/** One corner badge: label plus its accent classes (colors ride tokens). */
interface BadgeSpec {
  key: string;
  label: string;
  className: string;
}

/** The badge row, carried from the factory in the same order: counters,
 * summoning sickness, marked damage, blocked ×N, then the ×N stack badge. */
function badgeSpecs(data: CardDisplayData): BadgeSpec[] {
  const badges: BadgeSpec[] = (data.counters ?? []).map((c, i) => ({
    key: `counter-${i}`,
    label: c.count === 1 ? c.kind : `${c.kind} ×${c.count}`,
    className: s.badgeCounter,
  }));
  if (data.summoningSick) badges.push({ key: 'sick', label: 'zz', className: s.badgePlain });
  if ((data.markedDamage ?? 0) > 0) {
    badges.push({ key: 'damage', label: `${data.markedDamage} dmg`, className: s.badgeDamage });
  }
  if ((data.blockedBy ?? 0) > 0) {
    badges.push({ key: 'blocked', label: `blocked ×${data.blockedBy}`, className: s.badgeBlocked });
  }
  if ((data.stackCount ?? 1) > 1) {
    badges.push({ key: 'stack', label: `×${data.stackCount}`, className: s.badgePlain });
  }
  return badges;
}

/**
 * The badge row. At the battlefield tiers every badge consolidates into ONE
 * bounded node (labels joined with a middle dot) so counters, damage, blocked,
 * and ×N can never scale the face past the node budget — each badge is text,
 * so the kind stays legible without per-badge color (non-color channel rule).
 * The screen-space tiers keep one colored span per badge.
 */
function Badges({ badges, consolidated }: { badges: BadgeSpec[]; consolidated: boolean }) {
  if (badges.length === 0) return null;
  if (consolidated) {
    return (
      <span className={cx(s.badge, s.badgeRow)}>{badges.map((b) => b.label).join(' · ')}</span>
    );
  }
  return (
    <>
      {badges.map((b) => (
        <span key={b.key} className={cx(s.badge, b.className)}>
          {b.label}
        </span>
      ))}
    </>
  );
}

/** The capped keyword strip: shown glyph names plus the `+N` overflow count. */
function keywordStrip(
  data: CardDisplayData,
  tier: CardFaceTier,
): { names: GlyphName[]; overflow: number } {
  const names = (data.keywords ?? [])
    .map(keywordGlyphName)
    .filter((n): n is GlyphName => n !== null);
  const m = faceMetrics(tier);
  const capacity = Math.max(1, Math.floor((m.w - 14) / (m.pip + 3)));
  if (names.length <= capacity) return { names, overflow: 0 };
  return { names: names.slice(0, capacity - 1), overflow: names.length - (capacity - 1) };
}

/**
 * The mana cost at the screen-space tiers (hand, inspect): one colored disc
 * span per symbol, swatched from the `PIP` tokens via {@link parseManaCost}.
 * Budget-exempt — the battlefield tiers render {@link CostPill} instead.
 */
function Pips({ data, flow }: { data: CardDisplayData; flow?: boolean }) {
  if (!data.manaCost) return null;
  return (
    <>
      {parseManaCost(data.manaCost).map((pip, i) => (
        <span
          key={i}
          className={cx(s.pip, flow && s.pipFlow)}
          style={
            {
              '--pip-i': i,
              '--pip-bg': pip.bg,
              '--pip-fg': pip.fg,
            } as CSSProperties
          }
        >
          {pip.symbol}
        </span>
      ))}
    </>
  );
}

/**
 * The mana cost at the battlefield tiers: ONE bounded node regardless of the
 * number of symbols (the ≤ 12-node budget is a hard per-face ceiling, so no
 * face element may scale with its input). The symbols read as text separated
 * by a middle dot — the symbol letters themselves are the information channel
 * (never color-only, ui-requirements §10); the per-symbol disc swatches remain
 * at the screen-space hand/inspect tiers where costs are read closely.
 */
function CostPill({ data }: { data: CardDisplayData }) {
  if (!data.manaCost) return null;
  const label = parseManaCost(data.manaCost)
    .map((pip) => pip.symbol)
    .join('·');
  return <div className={s.cost}>{label}</div>;
}

/** The one-svg keyword strip (combined paths; `+N` overflow tag). */
function KeywordStrip({ names, overflow }: { names: GlyphName[]; overflow: number }) {
  if (names.length === 0) return null;
  const geo = glyphStripGeometry(names);
  const overflowUnits = overflow > 0 ? 30 : 0;
  return (
    <svg
      className={s.keywords}
      viewBox={`0 0 ${geo.width + overflowUnits} 24`}
      strokeWidth={geo.strokeWidth}
      aria-label={`keywords: ${names.map((n) => n.slice(3)).join(', ')}`}
      role="img"
      data-keywords={names.length}
      data-overflow={overflow > 0 ? overflow : undefined}
    >
      <path d={geo.stroke} fill="none" stroke="currentColor" />
      {geo.fill && <path d={geo.fill} fill="currentColor" stroke="none" />}
      {overflow > 0 && (
        <text x={geo.width + 4} y={17} className={s.keywordOverflow}>
          +{overflow}
        </text>
      )}
    </svg>
  );
}

/** State data-attributes shared by every tier's root (test and consumer hooks). */
function stateAttrs(data: CardDisplayData, tier: CardFaceTier, elevation: CardElevation) {
  return {
    'data-tier': tier,
    'data-elevation': elevation,
    'data-tapped': data.tapped ? true : undefined,
    'data-selected': data.selected ? true : undefined,
    'data-targeting': data.targeting ? true : undefined,
    'data-actionable': data.actionable ? true : undefined,
    'data-attacking': data.attacking ? true : undefined,
    'data-blocking': data.blocking ? true : undefined,
    'data-dimmed': data.dimmed ? true : undefined,
    'data-ability': data.hasActivatedAbility ? true : undefined,
    'data-stack': (data.stackCount ?? 1) > 1 ? data.stackCount : undefined,
  };
}

/** Root state classes: every one maps to a zero-node CSS channel. */
function stateClasses(data: CardDisplayData): (string | false | undefined)[] {
  return [
    data.selected && s.selected,
    data.targeting && s.targeting,
    data.actionable && s.actionable,
    data.attacking && s.attacking,
    data.blocking && s.blocking,
    data.hasActivatedAbility && s.hasAbility,
    (data.stackCount ?? 1) > 1 && s.stacked,
  ];
}

/**
 * The single DOM card face. See the module doc for the contract; the root is a
 * presentational `role="img"` — interactivity (hotspots, focus, activation)
 * stays with the consuming surface, which positions the root inside the rect
 * the scene reserved (the tapped footprint is pre-reserved, carried rule).
 */
export function CardFace({
  data,
  tier = 'field',
  elevation = 'rest',
  art,
  rulesText,
  className,
}: CardFaceProps) {
  const vars = cardFaceVars(data, tier, elevation);
  if (tier === 'inspect') {
    return (
      <InspectFace
        data={data}
        art={art}
        rulesText={rulesText}
        className={className}
        vars={vars}
        elevation={elevation}
      />
    );
  }

  const full = art?.full === true;
  const windowArt = art && !full && (tier === 'field' || tier === 'hand') ? art : undefined;
  const strip = full || tier === 'chip' ? { names: [], overflow: 0 } : keywordStrip(data, tier);
  const badges = badgeSpecs(data);
  const rootProps = {
    className: cx(s.face, className, ...stateClasses(data)),
    style: vars,
    role: 'img' as const,
    'aria-label': data.name,
    ...stateAttrs(data, tier, elevation),
  };

  if (tier === 'chip') {
    // The chip budget (carried): frame color, name or basic-land glyph, tap
    // state — plus the shared zero-node channels (bar, rings, ×N splay).
    return (
      <div {...rootProps}>
        <div className={s.inner} data-monogram="">
          {art && <img className={s.artFull} src={art.url} alt="" />}
          {!art?.full &&
            (data.landGlyph ? (
              <ChipGlyph name={data.landGlyph} scrim={art !== undefined} />
            ) : (
              <div className={cx(s.chipName, art && s.overArt)}>{data.name}</div>
            ))}
          <Badges badges={badges} consolidated />
        </div>
      </div>
    );
  }

  return (
    <div {...rootProps}>
      <div className={s.inner} data-monogram={full || windowArt ? '' : data.name.slice(0, 1)}>
        {full && <img className={s.artFull} src={art!.url} alt="" />}
        {/* The name and type nodes always exist (empty in full-card mode):
            their pseudo-elements carry the combat edge bars and the latent
            ability marker, and the art image stacks below every overlay — so
            each server-computed channel survives every face mode unchanged. */}
        <div className={s.name}>{full ? '' : data.name}</div>
        {!full && (tier === 'hand' ? <Pips data={data} /> : <CostPill data={data} />)}
        {windowArt && <img className={s.artWindow} src={windowArt.url} alt="" />}
        <div className={cx(s.type, windowArt && s.overArt)}>{full ? '' : data.typeLine}</div>
        <KeywordStrip names={strip.names} overflow={strip.overflow} />
        {data.power !== undefined && data.toughness !== undefined && (
          <div className={s.pt}>{`${data.power}/${data.toughness}`}</div>
        )}
        <Badges badges={badges} consolidated={tier !== 'hand'} />
      </div>
    </div>
  );
}

/** A chip's basic-land glyph as one svg + combined path (+ scrim over art). */
function ChipGlyph({ name, scrim }: { name: GlyphName; scrim: boolean }) {
  const geo = glyphStripGeometry([name]);
  return (
    <svg
      className={cx(s.chipGlyph, scrim && s.overArt)}
      viewBox="0 0 24 24"
      strokeWidth={geo.strokeWidth}
      role="img"
      aria-label={name.replace('land-', '')}
    >
      <path d={geo.stroke} fill="none" stroke="currentColor" />
      {geo.fill && <path d={geo.fill} fill="currentColor" stroke="none" />}
    </svg>
  );
}

/**
 * The fixed screen-space inspect tier: everything the supplied data carries —
 * name, cost, type line, keywords, rules text, P/T, counters and state badges —
 * at reading size, independent of battlefield card size (budget rule). Screen
 * space is exempt from the battlefield node budget. The production inspect
 * popover (`CardInspect`) remains the shipped surface until Phase 2 unifies it
 * onto this tier.
 */
function InspectFace({
  data,
  art,
  rulesText,
  className,
  vars,
  elevation,
}: {
  data: CardDisplayData;
  art?: CardFaceArt;
  rulesText?: string;
  className?: string;
  vars: CSSProperties;
  elevation: CardElevation;
}) {
  const full = art?.full === true;
  const strip = keywordStrip(data, 'inspect');
  const badges = badgeSpecs(data);
  return (
    <div
      className={cx(s.face, s.inspect, className, ...stateClasses(data))}
      style={vars}
      role="img"
      aria-label={data.name}
      {...stateAttrs(data, 'inspect', elevation)}
    >
      <div className={cx(s.inner, s.inspectInner)} data-monogram={art ? '' : data.name.slice(0, 1)}>
        {full && <img className={s.artFull} src={art!.url} alt="" />}
        <div className={s.name}>{full ? '' : data.name}</div>
        {!full && (
          <div className={s.inspectCost}>
            <Pips data={data} flow />
          </div>
        )}
        {art && !full && <img className={s.inspectArt} src={art.url} alt="" />}
        {!full && <div className={cx(s.type, s.inspectType)}>{data.typeLine}</div>}
        {!full && rulesText !== undefined && rulesText !== '' && (
          <div className={s.rules}>{rulesText}</div>
        )}
        <div className={s.inspectFooter}>
          <KeywordStrip names={strip.names} overflow={strip.overflow} />
          {data.power !== undefined && data.toughness !== undefined && (
            <div className={cx(s.pt, s.ptFlow)}>{`${data.power}/${data.toughness}`}</div>
          )}
        </div>
        <Badges badges={badges} consolidated={false} />
      </div>
    </div>
  );
}
