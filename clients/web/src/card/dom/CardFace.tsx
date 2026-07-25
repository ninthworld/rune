/**
 * The DOM card face — the ONE card renderer for every card surface of the 2.5D
 * client (issue #479, ADR 0030 layer 2), rebuilt to the Rune frame of
 * `docs/design/card-representation.md` (issue #538) for issue #529.
 *
 * **Two silhouettes, one family** (card-representation §2, §3.1, §4):
 *
 * | Surface | Silhouette | Bands |
 * | --- | --- | --- |
 * | battlefield permanent | 1.00 square plaque | title, art, status |
 * | battlefield land | 1.45 resource tile | art (+ name strip, §15.9) |
 * | hand / stack | 0.715 portrait card | title, art, type, rules |
 * | inspect | fixed screen-space reading panel | everything supplied |
 *
 * The square plaque is the portrait card with its rules area structurally
 * removed, and it deliberately **carries no mana cost and no type bar** (§3.3)
 * — transcribed from all three approved baselines and stated there as a
 * normative rule. Type identity on the battlefield is carried by the status
 * band's glyph plates and by inspect.
 *
 * Design rules, carried:
 * - **No game logic.** Every value renders exactly as supplied; the P/T plate
 *   always shows the server-computed current value, never a printed one.
 * - **No literals.** Colors and sizes flow from `src/tokens.ts` through
 *   {@link cardFaceVars} as CSS custom properties (ADR 0019).
 * - **Budget-shaped DOM** (presentation-budgets §Performance: ≤ 12 nodes per
 *   battlefield-tier face — a hard, input-independent ceiling): every state
 *   channel is zero-node — the gold hairline, the actionable/attacking/blocking
 *   edge bars, the latent-ability dot, the summoning-sick / glyph-overflow
 *   plate and the procedural monogram are pseudo-elements; rings, blooms and
 *   the splayed pile are box-shadows; tap, dim and elevation are
 *   transform + opacity — and no element scales with its input: the glyph strip
 *   is one `<svg>` with combined paths, every badge consolidates into one row,
 *   and the art window is ONE node whether it holds an illustration or the
 *   procedural color-identity field.
 * - **Art is contained by the frame, never the file** (issue #527): every
 *   illustration goes through the one {@link CardArt} primitive, whose modes
 *   fix the image box from this face's band geometry.
 * - **Authoritative overlays survive every art mode** (ADR 0024 / §12): in
 *   full-card mode Rune's printed-text elements are suppressed because the
 *   image carries them, but the P/T plate, badges, tab, tap, and every ring and
 *   edge bar still draw on top.
 */
import type { CSSProperties } from 'react';
import { parseManaCost, type CardDisplayData } from '../cardFactory';
import { keywordGlyphName, type GlyphName } from '../../chrome/glyphs';
import { SymbolText } from '../../chrome/symbols';
import { cx } from '../../chrome/cx';
import { ART } from '../../tokens';
import { CardArt, CardArtSlot } from './CardArt';
import { glyphStripGeometry } from './glyphStrip';
import {
  cardFaceVars,
  drawsIdentityStrip,
  splayLayers,
  surfaceKindFor,
  type CardFaceTier,
} from './theme';
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
  /** The shared display description — same data contract as every surface. */
  data: CardDisplayData;
  /** Size tier (defaults to `field`). */
  tier?: CardFaceTier;
  /** Elevation ladder state (defaults to `rest`). */
  elevation?: CardElevation;
  /** Player-side illustration, if the art store has one published (ADR 0024). */
  art?: CardFaceArt;
  /** Server-supplied rules text, rendered verbatim in the rules area of the
   * full-card tiers and in full at `inspect` (card-representation §3.8). */
  rulesText?: string;
  /** Extra class on the root (positioning is the consumer's job). */
  className?: string;
}

/**
 * How many keyword glyph plates a tier's status band shows before the third
 * collapses to `+N` (card-representation §3.9, and the ladder's rung 5 in §10:
 * 3 → 2 → 1 + `+N`). The dense rungs simplify secondary glyphs first, which is
 * exactly why the cap is a tier property and not a measurement.
 */
const GLYPH_CAP: Record<CardFaceTier, number> = {
  chip: 1,
  mini: 2,
  support: 3,
  field: 3,
  hand: 3,
  stack: 3,
  inspect: 6,
};

/** The marker text of the summoning-sickness plate (§6.2 — a plate, not a dim). */
const SICK_MARK = 'zz';

/** One corner badge: label plus its accent classes (colors ride tokens). */
interface BadgeSpec {
  key: string;
  label: string;
  className: string;
}

/**
 * The badge row (card-representation §7.2/§7.3): counter kinds, then marked
 * damage, then `blocked ×N`. The `×N` fold badge is NOT here — it moved to the
 * top-edge tab channel (§7.4/§15.4), and summoning sickness is a status-band
 * plate (§6.2), so neither competes with the art window's badge channels.
 */
function badgeSpecs(data: CardDisplayData): BadgeSpec[] {
  const badges: BadgeSpec[] = (data.counters ?? []).map((c, i) => ({
    key: `counter-${i}`,
    label: c.count === 1 ? c.kind : `${c.kind} ×${c.count}`,
    className: s.badgeCounter,
  }));
  if ((data.markedDamage ?? 0) > 0) {
    badges.push({ key: 'damage', label: `${data.markedDamage} dmg`, className: s.badgeDamage });
  }
  if ((data.blockedBy ?? 0) > 0) {
    badges.push({ key: 'blocked', label: `blocked ×${data.blockedBy}`, className: s.badgeBlocked });
  }
  return badges;
}

/**
 * The badge row. At the battlefield tiers every badge consolidates into ONE
 * bounded node (labels joined with a middle dot) so counters, damage, and
 * `blocked ×N` can never scale the face past the node budget — each badge is
 * text, so the kind stays legible without per-badge color (non-color channel
 * rule). The screen-space tiers keep one shaped, colored badge per kind, which
 * is where §7.3's torn damage silhouette reads.
 */
function Badges({ badges, consolidated }: { badges: BadgeSpec[]; consolidated: boolean }) {
  if (badges.length === 0) return null;
  if (consolidated) {
    return (
      <span className={cx(s.badge, s.badgeRow)} style={{ '--badge-i': 0 } as CSSProperties}>
        {badges.map((b) => b.label).join(' · ')}
      </span>
    );
  }
  // Screen space: one shaped badge per kind, stacking upward from the art
  // window's lower-left (§7.2) through the index — zero wrapper nodes.
  let lane = 0;
  return (
    <>
      {badges.map((b) => (
        <span
          key={b.key}
          className={cx(s.badge, b.className)}
          style={{ '--badge-i': b.className === s.badgeDamage ? 0 : lane++ } as CSSProperties}
        >
          {b.label}
        </span>
      ))}
    </>
  );
}

/** The capped keyword glyph plates plus the count that overflowed them. */
function keywordStrip(
  data: CardDisplayData,
  tier: CardFaceTier,
): { names: GlyphName[]; overflow: number } {
  const names = (data.keywords ?? [])
    .map(keywordGlyphName)
    .filter((n): n is GlyphName => n !== null);
  const capacity = GLYPH_CAP[tier];
  if (names.length <= capacity) return { names, overflow: 0 };
  return { names: names.slice(0, capacity - 1), overflow: names.length - (capacity - 1) };
}

/**
 * The status band's **extra plate** (zero nodes — it is the band's `::after`):
 * the glyph overflow `+N` (§3.9) or, when nothing overflowed, the
 * summoning-sickness marker (§6.2, which replaces the old alpha dim so the
 * state survives at every tier and never competes with tap). Sickness wins a
 * contested plate: a game state outranks a keyword count, and the full keyword
 * list is always one inspect away.
 */
function extraPlate(data: CardDisplayData, overflow: number): string | undefined {
  if (data.summoningSick) return SICK_MARK;
  return overflow > 0 ? `+${overflow}` : undefined;
}

/**
 * The mana cost at the screen-space tiers: one colored disc span per symbol,
 * swatched from the `PIP` tokens via {@link parseManaCost}. Card-representation
 * §3.5 keeps the shipped `PIP` swatches at exactly these tiers — and the
 * battlefield face draws no cost at all, so there is no dense-tier form.
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

/** The one-svg keyword strip: combined stroke and fill paths, no per-glyph node. */
function KeywordStrip({ names, className }: { names: GlyphName[]; className?: string }) {
  if (names.length === 0) return null;
  const geo = glyphStripGeometry(names);
  return (
    <svg
      className={cx(s.keywords, className)}
      viewBox={`0 0 ${geo.width} 24`}
      strokeWidth={geo.strokeWidth}
      aria-label={`keywords: ${names.map((n) => n.slice(3)).join(', ')}`}
      role="img"
      data-keywords={names.length}
    >
      <path d={geo.stroke} fill="none" stroke="currentColor" />
      {geo.fill && <path d={geo.fill} fill="currentColor" stroke="none" />}
    </svg>
  );
}

/** A land's mana/basic glyph, drawn as the tile's own plate (§4). */
function LandGlyph({ name, className }: { name: GlyphName; className?: string }) {
  const geo = glyphStripGeometry([name]);
  return (
    <svg
      className={cx(s.landGlyph, className)}
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
 * The art window — exactly ONE node in every state (card-representation §3.6).
 * With a published illustration it is the {@link CardArt} primitive in `window`
 * mode; with none it is the procedural color-identity field carrying the accent
 * monogram, which is the ADR 0024 default and the only thing that ever ships in
 * the repo. Keeping both forms at one node is what buys the frame its art
 * window inside the ≤ 12-node ceiling.
 */
function ArtWindow({ url, monogram }: { url?: string; monogram: string }) {
  if (url !== undefined) return <CardArt url={url} mode="window" />;
  return <div className={s.artField} data-monogram={monogram} />;
}

/** State data-attributes shared by every tier's root (test and consumer hooks). */
function stateAttrs(
  data: CardDisplayData,
  tier: CardFaceTier,
  elevation: CardElevation,
  kind: string,
) {
  return {
    'data-tier': tier,
    'data-kind': kind,
    'data-elevation': elevation,
    'data-tapped': data.tapped ? true : undefined,
    'data-selected': data.selected ? true : undefined,
    'data-targeting': data.targeting ? true : undefined,
    'data-actionable': data.actionable ? true : undefined,
    'data-attacking': data.attacking ? true : undefined,
    'data-blocking': data.blocking ? true : undefined,
    'data-dimmed': data.dimmed ? true : undefined,
    'data-ability': data.hasActivatedAbility ? true : undefined,
    'data-sick': data.summoningSick ? true : undefined,
    'data-stack': (data.stackCount ?? 1) > 1 ? data.stackCount : undefined,
    // The pile's drawn depth (§6.1 panel 7) — capped, so it never scales with
    // the fold; the ×N tab carries the exact count.
    'data-splay': splayLayers(data.stackCount) || undefined,
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

/** The `×N` top-edge tab (§7.4), or nothing when this face is not a fold. */
function CountTab({ data }: { data: CardDisplayData }) {
  const count = data.stackCount ?? 1;
  if (count <= 1) return null;
  return <span className={s.tab}>{`×${count}`}</span>;
}

/** The P/T plate — the single authoritative characteristic surface (§3.9). */
function PtPlate({ data, className }: { data: CardDisplayData; className?: string }) {
  if (data.power === undefined || data.toughness === undefined) return null;
  return <div className={cx(s.pt, className)}>{`${data.power}/${data.toughness}`}</div>;
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
  const kind = surfaceKindFor(tier, data.landTile);
  // ADR 0024 full-card mode replaces the *drawn* face; chips and land tiles
  // stay procedural in every art mode (§12), so they never take that branch.
  const windowTier = (ART.tiers as readonly string[]).includes(tier);
  const inspect = tier === 'inspect';
  const full =
    !inspect && windowTier && kind !== 'land' && art?.full === true ? art.url : undefined;
  const url = full === undefined && windowTier && !inspect ? art?.url : undefined;
  const body: FaceBodyProps = { data, url, full };
  return (
    <div
      className={cx(s.face, inspect && s.inspect, className, ...stateClasses(data))}
      style={cardFaceVars(data, tier, elevation)}
      role="img"
      aria-label={data.name}
      {...stateAttrs(data, tier, elevation, kind)}
    >
      {inspect ? (
        <InspectFace data={data} art={art} rulesText={rulesText} />
      ) : kind === 'land' ? (
        <LandTileFace {...body} />
      ) : kind === 'card' ? (
        <FullCardFace {...body} tier={tier} rulesText={rulesText} />
      ) : (
        <PermanentFace {...body} tier={tier} />
      )}
    </div>
  );
}

/** Props every silhouette receives from {@link CardFace}. */
interface FaceBodyProps {
  data: CardDisplayData;
  /** Already-resolved window illustration URL, or `undefined` for procedural. */
  url?: string;
  /** Already-resolved ADR 0024 full-card image URL, when that mode is active. */
  full?: string;
}

/**
 * The **battlefield permanent** — the 1.00 square plaque (§3.3).
 *
 * Nodes, worst case: root, inner, art window, title plate, status band, glyph
 * `<svg>` + its two combined paths, P/T plate, consolidated badge, `×N` tab —
 * eleven, one inside the twelve-node ceiling. The **title band** is the one
 * thing the dense rungs spend differently (§8.4), at no change in node count:
 * `mini` swaps the parchment name plate for the colour-identity strip, and
 * `chip` drops the band entirely; both carry identity on the accent, the
 * land/type glyph, and the inspect path instead.
 */
function PermanentFace({ data, tier, url, full }: FaceBodyProps & { tier: CardFaceTier }) {
  const chip = tier === 'chip';
  const identityStrip = drawsIdentityStrip(tier, 'permanent');
  const strip = full !== undefined ? { names: [], overflow: 0 } : keywordStrip(data, tier);
  const glyphs = chip && data.landGlyph !== undefined ? [data.landGlyph] : strip.names;
  return (
    <div className={s.inner}>
      {full !== undefined ? (
        <CardArt url={full} mode="full" />
      ) : (
        <ArtWindow url={url} monogram={data.name.slice(0, 1)} />
      )}
      {/* The title band always exists — empty in full-card mode and absent only
          at `chip` — because its pseudo-elements carry the attacking edge bar
          and the latent-ability marker dot, so those server-computed channels
          survive every art mode unchanged. At `mini` it is the colour-identity
          strip rather than the name plate (§8.4): one node either way, so the
          swap costs nothing and buys back the height the 11 px name floor was
          taking out of the art window. */}
      {identityStrip ? (
        <div className={s.identityStrip} />
      ) : (
        !chip && <div className={s.title}>{full !== undefined ? '' : data.name}</div>
      )}
      <div className={s.status} data-plate-extra={extraPlate(data, strip.overflow)}>
        <KeywordStrip names={glyphs} />
        <PtPlate data={data} />
      </div>
      <Badges badges={badgeSpecs(data)} consolidated />
      <CountTab data={data} />
    </div>
  );
}

/**
 * The **battlefield land** — the 1.45 resource tile (§4): art only, framed, with
 * the mana/basic glyph plate at the bottom-left and no title bar. A **nonbasic
 * or actionable** land gets a bottom name strip and is excluded from the chip
 * rung entirely (§15.9), so it can never collapse to an anonymous glyph.
 */
function LandTileFace({ data, url }: FaceBodyProps) {
  const named = data.landGlyph === undefined || data.actionable === true;
  return (
    <div className={s.inner}>
      <ArtWindow url={url} monogram={data.name.slice(0, 1)} />
      {data.landGlyph !== undefined && (
        <LandGlyph name={data.landGlyph} className={url !== undefined ? s.overArt : undefined} />
      )}
      {named && <div className={cx(s.nameStrip, s.overArt)}>{data.name}</div>}
      <Badges badges={badgeSpecs(data)} consolidated />
      <CountTab data={data} />
    </div>
  );
}

/**
 * The **full card** — the 0.715 portrait silhouette of the hand fan and the
 * stack rail (§3.2): title bar with the cost disc, art window, type bar, rules
 * area, and the P/T plate hanging at the rules area's bottom-right. Screen
 * space, so the node budget does not bind and per-symbol pips and per-badge
 * spans return.
 */
function FullCardFace({
  data,
  tier,
  url,
  full,
  rulesText,
}: FaceBodyProps & { tier: CardFaceTier; rulesText?: string }) {
  const strip = keywordStrip(data, tier);
  return (
    <div className={s.inner}>
      {full !== undefined ? (
        <CardArt url={full} mode="full" />
      ) : (
        <ArtWindow url={url} monogram={data.name.slice(0, 1)} />
      )}
      <div className={s.title}>{full !== undefined ? '' : data.name}</div>
      {full === undefined && (
        <div className={s.cost}>
          <Pips data={data} />
        </div>
      )}
      <div className={s.type}>{full !== undefined ? '' : data.typeLine}</div>
      <div className={s.rules} data-plate-extra={extraPlate(data, strip.overflow)}>
        {/* §3.8 renders the server's rules string verbatim — its `{…}` runs are
            drawn as symbols rather than printed as braces (issue #462). The
            words are untouched; only the notation becomes an icon. */}
        {full !== undefined ? '' : <SymbolText text={rulesText ?? ''} />}
        <KeywordStrip names={full !== undefined ? [] : strip.names} className={s.rulesGlyphs} />
      </div>
      <PtPlate data={data} />
      <Badges badges={badgeSpecs(data)} consolidated={false} />
      <CountTab data={data} />
    </div>
  );
}

/**
 * The fixed screen-space **inspect** tier: everything the supplied data carries
 * — name, cost, type line, keywords, rules text, P/T, counters and state badges
 * — at reading size, independent of battlefield card size (budget rule, and
 * §3.8: inspect always shows the full rules string, so this surface stays a
 * flowing reading panel rather than a fixed 0.715 box that could clip it).
 *
 * Art here goes into a permanently reserved {@link CardArtSlot} (issue #527)
 * rather than being mounted on arrival, and ADR 0024's full-card image is shown
 * *inside* that slot (`panelFull`) instead of replacing the whole face the way
 * it does on a battlefield frame. That is deliberate: this tier is a reading
 * surface, so its text stays present in both art modes, and with the text and
 * the slot both unconditional the panel's height is a function of the card's
 * words alone — no download and no art-style flip can move it.
 */
function InspectFace({
  data,
  art,
  rulesText,
}: {
  data: CardDisplayData;
  art?: CardFaceArt;
  rulesText?: string;
}) {
  const full = art?.full === true;
  const strip = keywordStrip(data, 'inspect');
  return (
    <div className={cx(s.inner, s.inspectInner)}>
      <div className={s.title}>{data.name}</div>
      <div className={cx(s.cost, s.inspectCost)}>
        <Pips data={data} flow />
      </div>
      {/* One reserved slot, always present, one size in both art modes
          (issue #527): the illustration and the whole-card image are both
          contained by it, so neither a late download nor an ADR 0024 art-style
          flip can change this panel's height. */}
      <CardArtSlot
        url={art?.url}
        mode={full ? 'panelFull' : 'panel'}
        monogram={data.name.slice(0, 1)}
      />
      <div className={s.type}>{data.typeLine}</div>
      {rulesText !== undefined && rulesText !== '' && (
        <div className={cx(s.rules, s.inspectRules)}>
          <SymbolText text={rulesText} />
        </div>
      )}
      <div className={s.inspectFooter} data-plate-extra={extraPlate(data, strip.overflow)}>
        <KeywordStrip names={strip.names} />
        <PtPlate data={data} className={s.ptFlow} />
      </div>
      <Badges badges={badgeSpecs(data)} consolidated={false} />
    </div>
  );
}
