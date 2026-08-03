import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import { PEEK_MS, PEEK_SLOP, usePeek } from "./peek";
import { useGlass, useCardView } from "./glass";
import { useArt } from "./art";
import { Pip } from "./Pips";

export type CardData = {
  name: string;
  cost: string[];
  typeLine: string;
  text: string;
  pt?: string;
  set?: string;
  /* the real card this one's pictures are fetched from, for the views
     that show any. Nothing else about the card comes from there. */
  art?: string;
};

/* the pips live in their own file now; re-exported so every caller still
   reaches them through the card */
export { Pip };

const COLOR_PIPS = ["W", "U", "B", "R", "G"];

/* a cost's colours, counting both halves of a hybrid and the colour side
   of a phyrexian pip */
function colorClass(cost: string[]): string {
  const colors = cost
    .flatMap((pip) => pip.split("/"))
    .filter((part) => COLOR_PIPS.includes(part.toUpperCase()))
    .map((part) => part.toUpperCase());
  if (colors.length === 0) return "c";
  if (new Set(colors).size > 1) return "gold";
  return colors[0].toLowerCase();
}

/* rules text: paragraphs split on \n, {X} tokens become mana pips,
   a lone keyword line and a leading keyword-cost word render bold */
function RulesText({ text }: { text: string }) {
  return (
    <>
      {text.split("\n").map((para, pi) => {
        const parts = para.split(/(\{[^}]+\})/);
        const barePara = para.replace(/\{[^}]+\}/g, "").trim();
        const keyword = !barePara.includes(" ");
        return (
          <p key={pi} className={keyword ? "c-kw" : undefined}>
            {parts.map((part, i) => {
              const token = part.match(/^\{(.+)\}$/);
              if (token) return <Pip key={i} symbol={token[1]} inline />;
              return <span key={i}>{part}</span>;
            })}
          </p>
        );
      })}
    </>
  );
}

/* Type is set in the card's own 207x291 grid, so the same "9px" line is
   18 device pixels in the preview and 3 on a board card. Each run of text
   therefore picks its own size: the largest that fits its box, found by
   bisection between a floor and a ceiling.

   The title and type line only ever shrink — they must clear the mana
   pips and their bars are a fixed height. Rules text has no design size
   at all: it is simply set as large as its box will take, so two words of
   reminder text fill the same space a paragraph needs and no card carries
   a half-empty text field. Because the box and the type are both in the
   card's own grid, that lands on the same size whether the card is a
   preview or a permanent on the board — the card is one drawing, scaled.

   RULES_MAX is the only dial: the point past which body text stops being
   body text, for the card that has two words to say. */
const RULES_MAX = 22;
/* the P/T is the number the game is played on, so it is set as large as
   the plaque will hold and only gives ground once a creature has grown
   enough digits to need it */
const PT_SIZE = 20;
const STEPS = 7;

const tooWide = (el: HTMLElement) => el.scrollWidth > el.clientWidth;
const tooBig = (el: HTMLElement) =>
  el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth;

function fit(
  ref: RefObject<HTMLElement | null>,
  hi: number,
  lo: number,
  overflows: (el: HTMLElement) => boolean,
) {
  const el = ref.current;
  if (!el) return;
  el.style.fontSize = `${hi}px`;
  if (!overflows(el)) return;
  let fits = lo;
  let over = hi;
  for (let i = 0; i < STEPS; i++) {
    const mid = (fits + over) / 2;
    el.style.fontSize = `${mid}px`;
    if (overflows(el)) over = mid;
    else fits = mid;
  }
  el.style.fontSize = `${fits}px`;
}

/* Nothing here needs to know how big the card is on screen any more: every
   run of text is fitted to a box measured in the card's own grid, so a
   card renders identically at any size and the per-card ResizeObserver
   that used to watch for it is gone. */

/* a bar is a sharp-edged rectangle whose short ends are circular arcs
   bulging outward by `b`; the arc meets the straight edges at hard
   corners — this is not a border-radius */
function barPath(w: number, h: number, b: number): string {
  const x0 = b;
  const x1 = w - b;
  const chord = h - 1;
  const r = (chord * chord) / (8 * b) + b / 2;
  return (
    `M ${x0} 0.5 L ${x1} 0.5 A ${r} ${r} 0 0 1 ${x1} ${h - 0.5} ` +
    `L ${x0} ${h - 0.5} A ${r} ${r} 0 0 1 ${x0} 0.5 Z`
  );
}

/* the ivory slab: gently rounded on top, rounded bottom corners ending
   high above the card bottom; the text box overhangs it into the dark */
const SLAB =
  "M 9 7 H 197 A 2 2 0 0 1 199 9 V 250 A 8 8 0 0 1 191 258 " +
  "H 15 A 8 8 0 0 1 7 250 V 9 A 2 2 0 0 1 9 7 Z";

const TITLE = barPath(183, 16.5, 2.5);
const TYPE = barPath(183, 17, 2.5);
const PT_OUTER = barPath(63, 30, 3);
const PT_INNER = barPath(58, 25, 2.5);

/* What is on a permanent that isn't printed on the card.

   There are a great many kinds of counter and no chance of drawing a mark
   for each, so a counter is a label and a count in a plain dark pill —
   one shape that any kind fits, named the way the rules name it. The
   pills sit in the dark well along the card's foot, left of the P/T.

   The ones that move power and toughness carry `pt`, the change each
   counter makes; the P/T plaque then prints the number you actually act
   on, coloured to say it isn't the printed one. */
export type Counter = { label: string; n: number; pt?: number };

/* A counter that moves power and toughness names its total rather than
   its kind and a multiplier: two +1/+1 counters are what the player
   thinks of as +2/+2. Every other kind keeps its name and a count. */
function chipLabel(counter: Counter) {
  if (!counter.pt) {
    return { text: counter.label, count: counter.n > 1 ? `×${counter.n}` : null };
  }
  const total = counter.n * counter.pt;
  const sign = total > 0 ? "+" : "−";
  const size = Math.abs(total);
  return { text: `${sign}${size}/${sign}${size}`, count: null };
}

function modified(pt: string, counters: Counter[]) {
  const delta = counters.reduce((sum, c) => sum + c.n * (c.pt ?? 0), 0);
  const parts = pt.split("/").map((half) => Number(half.trim()));
  if (!delta || parts.length !== 2 || parts.some(Number.isNaN)) {
    return { text: pt, delta: 0 };
  }
  return { text: `${parts[0] + delta}/${parts[1] + delta}`, delta };
}

export function Card({
  card,
  onHover,
  style,
  anchor,
  glass: glassProp,
  counters,
  sick,
}: {
  card: CardData;
  onHover?: (card: CardData | null) => void;
  style?: CSSProperties;
  /* what a targeting arrow aims at, when this card is one of its ends */
  anchor?: string;
  /* overrides the board's setting, so the lab can show both at once */
  glass?: boolean;
  /* permanent state, not card state: only the top card of a slot wears it */
  counters?: Counter[];
  sick?: boolean;
}) {
  const uid = useId();
  const peek = usePeek();
  const glass = glassProp ?? useGlass();
  const view = useCardView();
  const art = useArt(view === "frame" ? undefined : card.art);
  const chips = (counters ?? []).filter((c) => c.n > 0);
  const pt = modified(card.pt ?? "", counters ?? []);
  const held = useRef<{ timer: number; x: number; y: number } | null>(null);
  const startHold = (e: ReactPointerEvent) => {
    if (!peek || e.button !== 0) return;
    held.current = {
      timer: window.setTimeout(() => peek(card), PEEK_MS),
      x: e.clientX,
      y: e.clientY,
    };
  };
  const endHold = () => {
    if (held.current) window.clearTimeout(held.current.timer);
    held.current = null;
  };
  const moveHold = (e: ReactPointerEvent) => {
    const at = held.current;
    if (!at) return;
    if (Math.hypot(e.clientX - at.x, e.clientY - at.y) > PEEK_SLOP) endHold();
  };
  const svgRef = useRef<SVGSVGElement>(null);
  const nameRef = useRef<HTMLSpanElement>(null);
  const typeRef = useRef<HTMLSpanElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const ptRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    fit(nameRef, 10, 6, tooWide);
    fit(typeRef, 8.5, 5.5, tooWide);
    fit(textRef, RULES_MAX, 4.5, tooBig);
    fit(ptRef, PT_SIZE, 7, tooWide);
  });
  /* the same card, however much of its face the view spends on a picture */
  const shell = {
    ref: svgRef,
    className: `card card-${colorClass(card.cost)}`,
    viewBox: "0 0 207 291",
    style,
    "data-anchor": anchor,
    onMouseEnter: onHover && (() => onHover(card)),
    onMouseLeave: onHover && (() => onHover(null)),
    onPointerDown: startHold,
    onPointerMove: moveHold,
    onPointerUp: endHold,
    onPointerCancel: endHold,
    onPointerLeave: endHold,
    onContextMenu: peek ? (e: ReactMouseEvent) => e.preventDefault() : undefined,
  };

  /* what is true of the permanent rather than of the card, so it is drawn
     over whichever face is underneath */
  const overlays = (
    <>
      {chips.length > 0 && (
        <foreignObject x="18" y="133" width="170" height="24">
          <div className="c-counters">
            {chips.map((counter) => {
              const chip = chipLabel(counter);
              return (
                <span key={counter.label} className="c-ct">
                  {chip.text}
                  {chip.count && <b>{chip.count}</b>}
                </span>
              );
            })}
          </div>
        </foreignObject>
      )}
      {sick && (
        <>
          <rect x="0" y="0" width="207" height="291" rx="7" fill="rgba(18, 30, 50, 0.46)" />
          <rect
            x="1"
            y="1"
            width="205"
            height="289"
            rx="6.5"
            fill="none"
            stroke="rgba(150, 178, 214, 0.42)"
            strokeWidth="2"
            strokeDasharray="9 7"
          />
        </>
      )}
    </>
  );

  /* the printed card, whole: none of the frame below is drawn, because
     none of it is ours in this view */
  if (view === "full" && art) {
    return (
      <svg {...shell}>
        <defs>
          <clipPath id={`${uid}-round`}>
            <rect x="0" y="0" width="207" height="291" rx="7" />
          </clipPath>
        </defs>
        <image
          href={art.full}
          x="0"
          y="0"
          width="207"
          height="291"
          preserveAspectRatio="xMidYMid slice"
          clipPath={`url(#${uid}-round)`}
        />
        {overlays}
      </svg>
    );
  }

  return (
    <svg {...shell}>
      <defs>
        <linearGradient id={`${uid}-slab`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" style={{ stopColor: "var(--panel)" }} />
          <stop offset="60%" style={{ stopColor: "var(--panel)" }} />
          <stop offset="100%" style={{ stopColor: "var(--panel-deep)" }} />
        </linearGradient>
        <clipPath id={`${uid}-tb`}>
          <path d={TITLE} />
        </clipPath>
        <clipPath id={`${uid}-ty`}>
          <path d={TYPE} />
        </clipPath>
        <filter id={`${uid}-sh`} x="-30%" y="-30%" width="160%" height="160%">
          <feDropShadow dx="0" dy="1.5" stdDeviation="1.5" floodOpacity="0.55" />
        </filter>

        {/* The card under the same light as the chrome: what is raised off
            the slab — the title bar, the type bar, the P/T chip — is lit
            along its top edge, and what is sunk into it — the art window,
            the text field — is shadowed along the same one. */}
        {glass && (
          <>
            <linearGradient id={`${uid}-bar`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" style={{ stopColor: "color-mix(in srgb, var(--title-b) 84%, #ffffff)" }} />
              <stop offset="55%" style={{ stopColor: "var(--title-b)" }} />
              <stop offset="100%" style={{ stopColor: "color-mix(in srgb, var(--title-b) 91%, #000000)" }} />
            </linearGradient>
            <linearGradient id={`${uid}-chip`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" style={{ stopColor: "color-mix(in srgb, var(--field) 86%, #ffffff)" }} />
              <stop offset="100%" style={{ stopColor: "color-mix(in srgb, var(--field) 93%, #000000)" }} />
            </linearGradient>
            <linearGradient id={`${uid}-rim`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="rgba(255, 255, 255, 0.30)" />
              <stop offset="34%" stopColor="rgba(255, 255, 255, 0.04)" />
              <stop offset="100%" stopColor="rgba(255, 255, 255, 0)" />
            </linearGradient>
            <linearGradient id={`${uid}-recess`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="rgba(0, 0, 0, 0.5)" />
              <stop offset="20%" stopColor="rgba(0, 0, 0, 0)" />
            </linearGradient>
            {/* one raking highlight across the whole face, so the parts
                read as one sheet of glass rather than five lit pieces */}
            <linearGradient id={`${uid}-sheen`} x1="0" y1="0" x2="0.85" y2="1">
              <stop offset="0%" stopColor="rgba(255, 255, 255, 0.10)" />
              <stop offset="40%" stopColor="rgba(255, 255, 255, 0.02)" />
              <stop offset="100%" stopColor="rgba(255, 255, 255, 0)" />
            </linearGradient>
          </>
        )}
      </defs>

      {/* black card and slab; the area below the slab is the plain
         border black — no separate tone */}
      <rect
        x="0"
        y="0"
        width="207"
        height="291"
        rx="7"
        style={{ fill: "var(--bg)" }}
      />
      <path d={SLAB} fill={`url(#${uid}-slab)`} />

      {/* art window: flush under the title bar, type bar sits on its foot */}
      <rect
        x="14"
        y="28"
        width="179"
        height="132.5"
        style={{ fill: "var(--key)" }}
      />
      <rect
        x="15"
        y="28"
        width="177"
        height="131.5"
        style={{ fill: "color-mix(in srgb, var(--f2) 30%, #23262b)" }}
      />
      {/* the picture, when this view spends one — cropped to fill the
          window rather than letterboxed inside it */}
      {view !== "frame" && art && (
        <image
          href={art.art}
          x="15"
          y="28"
          width="177"
          height="131.5"
          preserveAspectRatio="xMidYMid slice"
        />
      )}

      {/* the art is sunk into the card, so it takes the shadow of the
          title bar sitting above it */}
      {glass && (
        <rect x="15" y="28" width="177" height="131.5" fill={`url(#${uid}-recess)`} />
      )}

      {/* rules text field: sharp box, tucked under the type bar at the
         same remove the type bar keeps from the art, its lower third
         overhanging the slab into the dark well. On a creature the text
         stops short of the P/T plaque rather than running under it. */}
      <rect
        x="13"
        y="179"
        width="181"
        height="92"
        style={{ fill: "var(--accent)" }}
      />
      <rect
        x="14.5"
        y="180.5"
        width="178"
        height="89"
        strokeWidth="1"
        style={{ fill: "var(--field)", stroke: "var(--key)" }}
      />
      {/* the same shadow on the text field, at a third the strength — any
          more and it dirties the ivory the rules text has to read off */}
      {glass && (
        <rect
          x="14.5"
          y="180.5"
          width="178"
          height="89"
          fill={`url(#${uid}-recess)`}
          opacity="0.34"
        />
      )}
      <foreignObject x="15" y="182" width="177" height={card.pt ? 72 : 86}>
        <div className="c-text" ref={textRef}>
          <RulesText text={card.text} />
        </div>
      </foreignObject>

      {/* type bar: black top edge overlapping the art's foot */}
      <g transform="translate(12 162)">
        <path
          d={TYPE}
          strokeWidth="3.5"
          style={{ fill: "var(--accent)", stroke: "var(--accent)" }}
        />
        <path d={TYPE} style={{ fill: glass ? `url(#${uid}-bar)` : "var(--title-b)" }} />
        <g clipPath={`url(#${uid}-ty)`}>
          <rect x="0" y="16" width="183" height="1" style={{ fill: "var(--key-soft)" }} />
        </g>
        <path d={TYPE} fill="none" strokeWidth="1" style={{ stroke: "var(--key)" }} />
        <foreignObject x="6" y="2" width="171" height="13.5">
          <div className="c-type-row">
            <span className="c-typeline" ref={typeRef}>
              {card.typeLine}
            </span>
            {card.set && <span className="c-set">◆</span>}
          </div>
        </foreignObject>
      </g>

      {/* title bar: sits on the slab with a sliver of it showing above */}
      <g transform="translate(12 9.5)">
        <path
          d={TITLE}
          strokeWidth="3.5"
          style={{ fill: "var(--accent)", stroke: "var(--accent)" }}
        />
        <path d={TITLE} style={{ fill: glass ? `url(#${uid}-bar)` : "var(--title-b)" }} />
        <g clipPath={`url(#${uid}-tb)`}>
          <rect x="0" y="15.5" width="183" height="1" style={{ fill: "var(--key-soft)" }} />
        </g>
        <path d={TITLE} fill="none" strokeWidth="1" style={{ stroke: "var(--key)" }} />
        <foreignObject x="5" y="1.5" width="173" height="13.5">
          <div className="c-title-row">
            <span className="c-name" ref={nameRef}>
              {card.name}
            </span>
            <span className="c-cost">
              {card.cost.map((pip, i) => (
                <Pip key={i} symbol={pip} />
              ))}
            </span>
          </div>
        </foreignObject>
      </g>

      {/* P/T plaque: same bulged-end construction — white outer bar,
         dark keyline, ivory fill */}
      {card.pt && (
        <g transform="translate(142 255)" filter={`url(#${uid}-sh)`}>
          <path d={PT_OUTER} style={{ fill: "var(--accent)" }} />
          <g transform="translate(2.5 2.5)">
            <path
              d={PT_INNER}
              strokeWidth="1.2"
              style={{
                fill: glass ? `url(#${uid}-chip)` : "var(--field)",
                stroke: "var(--key)",
              }}
            />
            {/* inset from the plaque, so a long P/T shrinks to a margin
                rather than to the keyline */}
            <foreignObject x="3" y="0" width="52" height="25">
              {/* the numeral, not the plaque, says the P/T has moved */}
              <div
                ref={ptRef}
                className={`c-pt-num${pt.delta > 0 ? " c-pt-up" : pt.delta < 0 ? " c-pt-down" : ""}`}
              >
                {pt.text}
              </div>
            </foreignObject>
          </g>
        </g>
      )}


      {/* last, so they lie over every part: one sheet of glass across the
          face, and the lit edge the chrome's panes all carry */}
      {glass && (
        <>
          <rect x="0" y="0" width="207" height="291" rx="7" fill={`url(#${uid}-sheen)`} />
          <rect
            x="0.6"
            y="0.6"
            width="205.8"
            height="289.8"
            rx="6.6"
            fill="none"
            strokeWidth="1.2"
            stroke={`url(#${uid}-rim)`}
          />
        </>
      )}

      {/* Counters sit in the art's bottom-left corner and summoning
          sickness washes the whole face — both are true of the permanent
          rather than of the card, so both are drawn over the frame. */}
      {overlays}
    </svg>
  );
}
