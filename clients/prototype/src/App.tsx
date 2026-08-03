import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { Card, Pip, type CardData } from "./Card";
import { CardLab } from "./CardLab";
import { Connect } from "./Connect";
import { Lobby } from "./Lobby";
import { Room } from "./Room";
import { FieldRow } from "./FieldRow";
import { Hand } from "./Hand";
import { PeekContext } from "./peek";
import { ZoneView, type ZoneRequest } from "./ZoneView";
import { Arrows, type Arrow } from "./Arrows";
import { GlassContext, ViewContext, type CardView } from "./glass";
import { Settings } from "./Settings";
import { PipDefs } from "./Pips";
import {
  SidePanel,
  type ChatLine,
  type LogLine,
  type StackItem,
} from "./SidePanel";
import type { Perm } from "./Permanent";
import { PlayerBar } from "./PlayerBar";

const forest: CardData = {
  name: "Forest",
  art: "Forest",
  cost: [],
  typeLine: "Basic Land — Forest",
  text: "{T}: Add {G}.",
};
const island: CardData = { ...forest, name: "Island", art: "Island", typeLine: "Basic Land — Island", text: "{T}: Add {U}." };
const mountain: CardData = { ...forest, name: "Mountain", art: "Mountain", typeLine: "Basic Land — Mountain", text: "{T}: Add {R}." };
const plains: CardData = { ...forest, name: "Plains", art: "Plains", typeLine: "Basic Land — Plains", text: "{T}: Add {W}." };
const swamp: CardData = { ...forest, name: "Swamp", art: "Swamp", typeLine: "Basic Land — Swamp", text: "{T}: Add {B}." };
const wastes: CardData = { ...forest, name: "Wastes", art: "Wastes", typeLine: "Basic Land", text: "{T}: Add {C}." };

const blade: CardData = {
  name: "Ancestral Blade",
  art: "Bonesplitter",
  cost: ["1", "W"],
  typeLine: "Artifact — Equipment",
  text: "Equipped creature gets +1/+1 and has vigilance.\nEquip {2}",
};

/* Seven seats to draw from, so the board can be filled out to eight
   players; how many are on screen is the topbar's stepper.

   Every seat carries the same zones, including the ones at zero: which
   zones exist is a property of the game, not of a player. A seat missing
   its command zone would shift every icon after it, and then the same
   glyph would sit in a different place on the bar beside it. */
const opponentPool = [
  {
    name: "Alice",
    life: 20,
    commanderCount: 1,
    mana: ["U", "U", "1"],
    handCount: 5,
    deckCount: 52,
    graveCount: 3,
    exileCount: 0,
    creatures: [
      {
        card: {
          name: "Tidecaller Sprite",
          art: "Cloud Sprite",
          cost: ["1", "U"],
          typeLine: "Creature — Faerie",
          text: "Flying",
          pt: "1/1",
        },
      },
    ],
    lands: [{ card: island, copies: 3 }],
  },
  {
    name: "Bob",
    life: 18,
    commanderCount: 0,
    handCount: 7,
    deckCount: 49,
    graveCount: 1,
    exileCount: 2,
    creatures: [
      {
        card: {
          name: "Emberhoof Ox",
          art: "Canyon Minotaur",
          cost: ["2", "R"],
          typeLine: "Creature — Ox",
          text: "Haste",
          pt: "3/2",
        },
        tapped: true,
      },
      {
        card: {
          name: "Ash Hopper",
          art: "Raging Goblin",
          cost: ["R"],
          typeLine: "Creature — Insect",
          text: "",
          pt: "1/1",
        },
      },
    ],
    lands: [{ card: mountain, copies: 2 }],
  },
  {
    name: "Cora",
    life: 24,
    commanderCount: 0,
    handCount: 4,
    deckCount: 47,
    graveCount: 6,
    exileCount: 1,
    creatures: [
      {
        card: {
          name: "Dawn Herald",
          art: "Ajani's Sunstriker",
          cost: ["2", "W"],
          typeLine: "Creature — Human Cleric",
          text: "Lifelink\nWhen this creature dies, you gain 2 life.",
          pt: "2/2",
        },
      },
    ],
    lands: [{ card: plains }, { card: plains, copies: 2, tapped: true }],
  },
  {
    name: "Dev",
    life: 11,
    commanderCount: 2,
    handCount: 2,
    deckCount: 44,
    graveCount: 12,
    exileCount: 0,
    creatures: [
      {
        card: {
          name: "Gravebound Acolyte",
          art: "Vampire Interloper",
          cost: ["1", "B"],
          typeLine: "Creature — Zombie Cleric",
          text: "Menace",
          pt: "2/1",
        },
      },
      {
        card: {
          name: "Hollow Reaper",
          art: "Vampire Nighthawk",
          cost: ["3", "B", "B"],
          typeLine: "Creature — Skeleton Knight",
          text: "Deathtouch\nWhenever this creature attacks, each opponent loses 1 life.",
          pt: "4/4",
        },
        attached: [blade],
      },
    ],
    lands: [{ card: swamp, copies: 4 }],
  },
  {
    name: "Eli",
    life: 27,
    commanderCount: 0,
    handCount: 6,
    deckCount: 50,
    graveCount: 2,
    exileCount: 0,
    creatures: [
      {
        card: {
          name: "Thornback Boar",
          art: "Charging Rhino",
          cost: ["2", "G"],
          typeLine: "Creature — Boar",
          text: "Trample",
          pt: "3/3",
        },
      },
    ],
    lands: [{ card: forest, copies: 4 }],
  },
  {
    name: "Fay",
    life: 16,
    commanderCount: 0,
    handCount: 3,
    deckCount: 46,
    graveCount: 5,
    exileCount: 3,
    creatures: [
      {
        card: {
          name: "Sunfire Marshal",
          art: "Goblin Legionnaire",
          cost: ["1", "R", "W"],
          typeLine: "Creature — Human Knight",
          text: "First strike, haste",
          pt: "2/2",
        },
      },
    ],
    lands: [{ card: plains }, { card: mountain, copies: 2 }],
  },
  {
    name: "Gus",
    life: 20,
    commanderCount: 0,
    handCount: 7,
    deckCount: 53,
    graveCount: 0,
    exileCount: 0,
    creatures: [
      {
        card: {
          name: "Iron Sentinel",
          art: "Juggernaut",
          cost: ["4"],
          typeLine: "Artifact Creature — Golem",
          text: "Vigilance",
          pt: "3/3",
        },
      },
    ],
    lands: [{ card: wastes, copies: 2 }],
  },
];

/* Zones only carry counts on the wire, so the prototype fills them from
   a pool — enough distinct cards that a graveyard doesn't read as one
   card repeated, offset per seat so no two look alike. */
const zonePool: CardData[] = [
  { name: "Cinder Bolt", art: "Lightning Strike", cost: ["1", "R"], typeLine: "Instant", text: "Deals 3 damage to any target." },
  { name: "Sunlit Ward", art: "Glorious Anthem", cost: ["1", "W"], typeLine: "Enchantment", text: "Creatures you control get +0/+1." },
  { name: "Thornback Boar", art: "Charging Rhino", cost: ["2", "G"], typeLine: "Creature — Boar", text: "Trample", pt: "3/3" },
  { name: "Tidecaller Sprite", art: "Cloud Sprite", cost: ["1", "U"], typeLine: "Creature — Faerie", text: "Flying", pt: "1/1" },
  blade,
  { name: "Gravebound Acolyte", art: "Vampire Interloper", cost: ["1", "B"], typeLine: "Creature — Zombie Cleric", text: "Menace", pt: "2/1" },
  forest,
  { name: "Dawn Herald", art: "Ajani's Sunstriker", cost: ["2", "W"], typeLine: "Creature — Human Cleric", text: "Lifelink\nWhen this creature dies, you gain 2 life.", pt: "2/2" },
  island,
  { name: "Emberhoof Ox", art: "Canyon Minotaur", cost: ["2", "R"], typeLine: "Creature — Ox", text: "Haste", pt: "3/2" },
  { name: "Archivist of the Forgotten Vault", art: "Archivist", cost: ["2", "U", "U"], typeLine: "Legendary Creature — Vedalken Wizard", text: "Flying, vigilance\nWhenever you draw your second card each turn, create a token that is a copy of this creature, except it isn't legendary.", pt: "2/4" },
  mountain,
  { name: "Hollow Reaper", art: "Vampire Nighthawk", cost: ["3", "B", "B"], typeLine: "Creature — Skeleton Knight", text: "Deathtouch", pt: "4/4" },
  swamp,
  { name: "Riftblade Duelist", art: "Boros Swiftblade", cost: ["1", "R", "W"], typeLine: "Creature — Human Soldier", text: "Double strike", pt: "3/1" },
  plains,
];

/* a library is too big to draw; a search shows a workable slice of it */
const MOST = 18;

function pileFor(seat: number, count: number): CardData[] {
  return Array.from({ length: Math.min(count, MOST) }, (_, i) =>
    zonePool[(seat * 5 + i * 3) % zonePool.length]);
}

/* what opening each zone is called, and whether the game wants one back */
function zoneRequest(zone: string, who: string, seat: number, count: number): ZoneRequest {
  const cards = pileFor(seat, count);
  switch (zone) {
    case "deck":
      return {
        title: `${who} library`,
        cards,
        choose: { action: "Put into hand", hint: "Search for a card to take" },
      };
    case "grave":
      return { title: `${who} graveyard`, cards };
    case "exile":
      return { title: `${who} exile`, cards };
    case "commander":
      return { title: `${who} command zone`, cards };
    default:
      return { title: `${who} hand`, cards };
  }
}

/* Seats tile a grid that stays as square as the board allows — at most
   four across on a wide screen, two once the board is a phone. The
   opponents' band then takes one share of the board's height per row it
   uses, so every seat on screen is the same height as mine. */
function seatGrid(count: number, maxCols: number) {
  const rows = Math.ceil(count / Math.min(count, maxCols));
  return { cols: Math.ceil(count / rows), rows };
}

const sampleHand = [
  {
    name: "Bramble Sentinel",
    art: "Grizzly Bears",
    cost: ["1", "G"],
    typeLine: "Creature — Elf Warrior",
    text: "Vigilance\nWhenever this creature attacks, you gain 1 life.",
    pt: "2/3",
  },
  {
    name: "Forest",
  art: "Forest",
    cost: [],
    typeLine: "Basic Land — Forest",
    text: "{T}: Add {G}.",
  },
  {
    name: "Stormwing Drake",
    art: "Wind Drake",
    cost: ["2", "U"],
    typeLine: "Creature — Drake",
    text: "Flying",
    pt: "2/1",
  },
  {
    name: "Cinder Bolt",
    art: "Lightning Strike",
    cost: ["1", "R"],
    typeLine: "Instant",
    text: "Deals 3 damage to any target.",
  },
  {
    name: "Sunlit Ward",
    art: "Glorious Anthem",
    cost: ["1", "W"],
    typeLine: "Enchantment",
    text: "Creatures you control get +0/+1.",
  },
];

const me = {
  name: "You",
  life: 20,
  commanderCount: 1,
  /* my hand is on screen, but the zone is still a zone: every seat's bar
     has to hold the same slots or no icon means the same thing twice */
  handCount: sampleHand.length,
  mana: ["G", "G", "W", "1"],
  deckCount: 51,
  graveCount: 4,
  exileCount: 1,
  creatures: [
    /* tapped — it attacked this turn */
    {
      card: {
        name: "Moss Colossus",
        art: "Craw Wurm",
        cost: ["3", "G", "G"],
        typeLine: "Creature — Elemental",
        text: "Trample",
        pt: "5/5",
      },
      tapped: true,
    },
    /* carrying an Equipment */
    {
      card: {
        name: "Dawn Skirmisher",
        art: "Youthful Knight",
        cost: ["W"],
        typeLine: "Creature — Human Soldier",
        text: "First strike",
        pt: "1/1",
      },
      attached: [blade],
    },
    /* tapped *and* equipped — the whole permanent turns together */
    {
      card: {
        name: "Riftblade Duelist",
        art: "Boros Swiftblade",
        cost: ["1", "R", "W"],
        typeLine: "Creature — Human Soldier",
        text: "Double strike",
        pt: "3/1",
      },
      attached: [blade],
      tapped: true,
    },
    /* just cast, so it can't attack yet */
    {
      card: {
        name: "Stormwing Drake",
        art: "Wind Drake",
        cost: ["2", "U"],
        typeLine: "Creature — Drake",
        text: "Flying",
        pt: "2/1",
      },
      sick: true,
    },
    /* grown by counters: the plaque reads 4/5, not the printed 2/3 */
    {
      card: {
        name: "Bramble Sentinel",
        art: "Grizzly Bears",
        cost: ["1", "G"],
        typeLine: "Creature — Elf Warrior",
        text: "Vigilance\nWhenever this creature attacks, you gain 1 life.",
        pt: "2/3",
      },
      counters: [{ label: "+1/+1", n: 2, pt: 1 }],
    },
    /* suspended: a time counter comes off at each of my upkeeps */
    {
      card: {
        name: "Ash Hopper",
        art: "Raging Goblin",
        cost: ["R"],
        typeLine: "Creature — Insect",
        text: "",
        pt: "1/1",
      },
      counters: [{ label: "time", n: 3 }],
      sick: true,
    },
  ] as Perm[],
  /* four untapped Forests pile into one slot; the two spent for mana are
     tapped, so they stand apart from the pile */
  lands: [
    { card: forest, copies: 4 },
    { card: plains, copies: 2, tapped: true },
    { card: island },
    { card: mountain, tapped: true },
    { card: swamp, copies: 3 },
  ],
};

function FieldArea({
  creatures,
  lands,
  onHover,
  mirrored,
  seat,
}: {
  creatures: Perm[];
  lands: Perm[];
  onHover: (card: CardData | null) => void;
  mirrored?: boolean;
  seat: string;
}) {
  const rows = [
    <FieldRow key="c" perms={creatures} onHover={onHover} anchor={`perm:${seat}:c`} />,
    <FieldRow key="l" perms={lands} onHover={onHover} anchor={`perm:${seat}:l`} />,
  ];
  return (
    <div className={`field-area${mirrored ? " field-area-mirror" : ""}`}>
      {mirrored ? rows.reverse() : rows}
    </div>
  );
}

const phases = [
  "Untap",
  "Upkeep",
  "Draw",
  "Main 1",
  "Begin Combat",
  "Attackers",
  "Blockers",
  "Damage",
  "End Combat",
  "Main 2",
  "End",
];
const turnNumber = 4;
const activePlayer = "You";

/* The stack, topmost first — what resolves next is what you read first.
   Depth follows the action stepper, so both a loaded stack and an empty
   one are reachable without another knob in the top bar. */
const sampleStack: StackItem[] = [
  {
    card: { name: "Cinder Bolt", art: "Lightning Strike", cost: ["1", "R"], typeLine: "Instant", text: "Deals 3 damage to any target." },
    who: "You",
    kind: "Instant",
    targets: ["Tidecaller Sprite"],
  },
  {
    card: { name: "Dawn Herald", art: "Ajani's Sunstriker", cost: ["2", "W"], typeLine: "Creature — Human Cleric", text: "Lifelink\nWhen this creature dies, you gain 2 life.", pt: "2/2" },
    who: "Cora",
    kind: "Triggered ability",
    targets: ["Cora"],
  },
  {
    card: blade,
    who: "You",
    kind: "Equip ability",
    targets: ["Dawn Skirmisher"],
  },
];

const STACK_DEPTH = [0, 0, 0, 3, 2, 2, 1, 0, 0];

const sampleLog: LogLine[] = [
  { turn: "Turn 4 — You" },
  { text: "You played Forest." },
  { text: "You cast Bramble Sentinel." },
  { text: "Bramble Sentinel entered the battlefield." },
  { text: "You cast Cinder Bolt targeting Tidecaller Sprite." },
  { turn: "Turn 3 — Bob" },
  { text: "Bob cast Emberhoof Ox." },
  { text: "Emberhoof Ox attacked You." },
  { text: "You took 3 damage. You are at 20." },
  { turn: "Turn 3 — Alice" },
  { text: "Alice played Island." },
  { text: "Alice cast Tidecaller Sprite." },
];

const sampleChat: ChatLine[] = [
  { who: "Alice", text: "good luck all" },
  { who: "Bob", text: "hf" },
  { who: "You", text: "sorry, misclick — meant to bolt the Ox" },
  { who: "Cora", text: "no worries, take your time" },
];

const helpers = [
  "To next turn",
  "To end step",
  "To main step",
  "To your turn",
  "Skip stack",
  "To prior end",
  "Cancel skip",
];

/* The action bar is the whole of what the client asks of you, so the
   prototype carries one of each shape it can take. Its tone tracks where
   in the turn you are rather than how urgent the ask is: green for the
   turn's bookends, blue while you may cast at will, red once combat is
   live and the choice costs something. */
type ActionState = {
  tone: "green" | "blue" | "red";
  prompt: ReactNode;
  detail?: string;
  phase?: string;
  buttons: { label: string; alt?: boolean }[];
};

const PLAY_ALL = "Play spells and abilities";
const PLAY_INSTANT = "Play instants and activated abilities";
const DONE = [{ label: "Done" }];

const actionStates: ActionState[] = [
  {
    tone: "green",
    prompt: "Mulligan down to 6 cards?",
    buttons: [{ label: "Mulligan", alt: true }, { label: "Keep" }],
  },
  { tone: "green", prompt: PLAY_INSTANT, detail: "Your turn / Upkeep", phase: "Upkeep", buttons: DONE },
  { tone: "blue", prompt: PLAY_ALL, detail: "Your turn / Precombat Main", phase: "Main 1", buttons: DONE },
  {
    tone: "blue",
    prompt: (
      <>
        Pay <Pip symbol="G" />
      </>
    ),
    detail: "Wall of Vines",
    phase: "Main 1",
    buttons: [{ label: "Cancel", alt: true }],
  },
  { tone: "red", prompt: PLAY_INSTANT, detail: "Your turn / Begin Combat", phase: "Begin Combat", buttons: DONE },
  { tone: "red", prompt: PLAY_INSTANT, detail: "Your turn / Declare Attackers", phase: "Attackers", buttons: DONE },
  { tone: "red", prompt: PLAY_INSTANT, detail: "Your turn / End Combat", phase: "End Combat", buttons: DONE },
  { tone: "blue", prompt: PLAY_ALL, detail: "Your turn / Postcombat Main", phase: "Main 2", buttons: DONE },
  { tone: "green", prompt: PLAY_INSTANT, detail: "Your turn / End Turn", phase: "End", buttons: DONE },
];

/* The arrow states the prototype has to look right in: one target, a
   spell held in hand pointing at several at once, a combat declaration
   fanning out of my creatures, blockers answering it from the other side,
   and something aimed back at me. */
function arrowScenes(opps: string[]): { label: string; arrows: Arrow[] }[] {
  const a = opps[0] ?? "You";
  const b = opps[1] ?? a;
  return [
    { label: "no arrows", arrows: [] },
    {
      label: "one target",
      arrows: [{ from: "perm:You:c:3", to: `perm:${a}:c:0`, tone: "target" }],
    },
    {
      label: "three targets",
      arrows: [
        { from: "hand:3", to: `perm:${a}:c:0`, tone: "target" },
        { from: "hand:3", to: `perm:${b}:c:1`, tone: "target" },
        { from: "hand:3", to: `player:${b}`, tone: "target" },
      ],
    },
    {
      label: "attackers",
      arrows: [
        { from: "perm:You:c:1", to: `player:${a}`, tone: "combat" },
        { from: "perm:You:c:4", to: `player:${a}`, tone: "combat" },
        { from: "perm:You:c:5", to: `player:${b}`, tone: "combat" },
      ],
    },
    {
      label: "blocks",
      arrows: [
        { from: "perm:You:c:1", to: `player:${a}`, tone: "combat" },
        { from: "perm:You:c:4", to: `player:${a}`, tone: "combat" },
        { from: `perm:${a}:c:0`, to: "perm:You:c:1", tone: "combat" },
      ],
    },
    {
      label: "aimed at me",
      arrows: [
        { from: `perm:${b}:c:0`, to: "perm:You:c:0", tone: "target" },
        { from: `perm:${b}:c:1`, to: "player:You", tone: "target" },
      ],
    },
  ];
}

function PhaseBar({ className, current }: { className: string; current?: string }) {
  return (
    <div className={`phase-bar ${className}`}>
      <span className="turn-info">
        Turn {turnNumber} — {activePlayer}
      </span>
      <span className="phase-pills">
        {phases.map((phase) => (
          <span
            key={phase}
            className={phase === current ? "phase phase-current" : "phase"}
          >
            {phase}
          </span>
        ))}
      </span>
    </div>
  );
}

export function App() {
  const [hovered, setHovered] = useState<CardData | null>(null);
  const [view, setView] = useState<
    "board" | "lab" | "connect" | "lobby" | "room"
  >("board");
  const [who, setWho] = useState("Chris");
  const [server, setServer] = useState("play.sage.gg");
  const [table, setTable] = useState("Ravnica Nights");
  const [players, setPlayers] = useState(3);
  const [focus, setFocus] = useState<string | null>(null);
  const [act, setAct] = useState(2);
  const [arrowScene, setArrowScene] = useState(1);
  const [glass, setGlass] = useState(true);
  /* how much of a card's face is ours; `view` above is which screen */
  const [face, setFace] = useState<CardView>("frame");
  const [peeked, setPeeked] = useState<CardData | null>(null);
  const [zone, setZone] = useState<ZoneRequest | null>(null);
  const [settings, setSettings] = useState(false);
  const [logOpen, setLogOpen] = useState(() => window.innerWidth > 900);
  useEffect(() => {
    if (!peeked) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setPeeked(null);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [peeked]);
  useEffect(() => {
    const narrow = window.matchMedia("(max-width: 900px)");
    const onChange = (e: MediaQueryListEvent) => setLogOpen(!e.matches);
    narrow.addEventListener("change", onChange);
    return () => narrow.removeEventListener("change", onChange);
  }, []);
  /* settings opens over whatever you are on rather than replacing it */
  const gear = settings ? (
    <Settings face={face} onFace={setFace} onClose={() => setSettings(false)} />
  ) : null;
  if (view === "lab") {
    return (
      <>
        <CardLab onBack={() => setView("board")} />
        {gear}
      </>
    );
  }
  if (view === "connect") {
    return (
      <>
        <Connect
          onBack={() => setView("board")}
          onConnect={(name, host) => {
            setWho(name);
            setServer(host);
            setView("lobby");
          }}
        />
        {gear}
      </>
    );
  }
  if (view === "lobby") {
    return (
      <>
        <Lobby
          who={who}
          server={server}
          onBack={() => setView("board")}
          onConnect={() => setView("connect")}
          onSettings={() => setSettings(true)}
          onJoin={(name) => {
            setTable(name);
            setView("room");
          }}
        />
        {gear}
      </>
    );
  }
  if (view === "room") {
    return (
      <>
        <Room
          who={who}
          table={table}
          onLeave={() => setView("lobby")}
          onSettings={() => setSettings(true)}
          onStart={() => setView("board")}
        />
        {gear}
      </>
    );
  }
  const opponents = opponentPool.slice(0, players - 1);
  /* a seat that has been stepped away stops being the focused one */
  const focused = opponents.some((opp) => opp.name === focus) ? focus : null;
  const wide = seatGrid(opponents.length, 4);
  const narrow = seatGrid(opponents.length, opponents.length <= 2 ? 1 : 2);
  /* focusing collapses the tiling to a single row */
  const action = actionStates[act];
  const scenes = arrowScenes(opponents.map((opp) => opp.name));
  const scene = scenes[arrowScene];
  const band = focused ? "1fr" : `${wide.rows}fr`;
  const bandNarrow = focused ? "1fr" : `${narrow.rows}fr`;
  return (
    <PeekContext.Provider value={setPeeked}>
    <GlassContext.Provider value={glass}>
    <ViewContext.Provider value={face}>
    <div
      className={`layout${logOpen ? "" : " log-hidden"}`}
      style={
        {
          "--opp-cols": wide.cols,
          "--opp-rows": wide.rows,
          "--opp-band": band,
          "--opp-cols-n": narrow.cols,
          "--opp-rows-n": narrow.rows,
          "--opp-band-n": bandNarrow,
        } as CSSProperties
      }
    >
      <div className="topbar">
        <button className="view-btn" onClick={() => setView("lab")}>
          Card lab
        </button>
        <button className="view-btn" onClick={() => setView("connect")}>
          Connect
        </button>
        <button className="view-btn" onClick={() => setView("lobby")}>
          Lobby
        </button>
        <button className="view-btn" onClick={() => setView("room")}>
          Table
        </button>
        <button
          className={`view-btn${glass ? " lab-picked" : ""}`}
          title="Cards wear the chrome's glass"
          onClick={() => setGlass((on) => !on)}
        >
          Glass
        </button>
        <span className="seat-step">
          <button
            className="step-btn"
            title="Remove a player"
            disabled={players <= 2}
            onClick={() => setPlayers((n) => Math.max(2, n - 1))}
          >
            −
          </button>
          <span className="seat-num">{players} players</span>
          <button
            className="step-btn"
            title="Add a player"
            disabled={players >= 8}
            onClick={() => setPlayers((n) => Math.min(8, n + 1))}
          >
            +
          </button>
        </span>
        <span className="seat-step">
          <button
            className="step-btn"
            title="Previous action state"
            disabled={act <= 0}
            onClick={() => setAct((n) => Math.max(0, n - 1))}
          >
            −
          </button>
          <span className="seat-num">
            action {act + 1}/{actionStates.length}
          </span>
          <button
            className="step-btn"
            title="Next action state"
            disabled={act >= actionStates.length - 1}
            onClick={() => setAct((n) => Math.min(actionStates.length - 1, n + 1))}
          >
            +
          </button>
        </span>
        <span className="seat-step">
          <button
            className="step-btn"
            title="Previous arrow state"
            disabled={arrowScene <= 0}
            onClick={() => setArrowScene((n) => Math.max(0, n - 1))}
          >
            −
          </button>
          <span className="seat-num scene-num">{scene.label}</span>
          <button
            className="step-btn"
            title="Next arrow state"
            disabled={arrowScene >= scenes.length - 1}
            onClick={() => setArrowScene((n) => Math.min(scenes.length - 1, n + 1))}
          >
            +
          </button>
        </span>
        <span className="topbar-fill" />
        <button
          className="settings-btn"
          title="Settings"
          onClick={() => setSettings(true)}
        >
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Game log"
          onClick={() => setLogOpen((open) => !open)}
        >
          ☰
        </button>
      </div>
      <PhaseBar className="phase-top" current={action.phase} />
      <div className="battlefield">
        <div className={`field-opponents${focused ? " opp-focused" : ""}`}>
          {opponents.map((opp, i) => {
            const collapsed = focused !== null && focused !== opp.name;
            return (
              <div
                key={opp.name}
                className={
                  `field field-opponent` +
                  `${collapsed ? " field-collapsed" : ""}` +
                  `${opp.name === activePlayer ? " field-active" : ""}`
                }
              >
                <PlayerBar
                  player={opp}
                  anchor={`player:${opp.name}`}
                  focused={focused === opp.name}
                  onFocus={() =>
                    setFocus((f) => (f === opp.name ? null : opp.name))
                  }
                  onZone={(key, count) =>
                    setZone(zoneRequest(key, `${opp.name}'s`, i + 1, count))
                  }
                />
                {!collapsed && (
                  <FieldArea
                    creatures={opp.creatures}
                    lands={opp.lands}
                    onHover={setHovered}
                    seat={opp.name}
                    mirrored
                  />
                )}
              </div>
            );
          })}
        </div>
        <PhaseBar className="phase-mid" current={action.phase} />
        <div
          className={`field field-mine${me.name === activePlayer ? " field-active" : ""}`}
        >
          <PlayerBar
            player={me}
            anchor="player:You"
            onZone={(key, count) => setZone(zoneRequest(key, "Your", 0, count))}
          />
          <FieldArea
            creatures={me.creatures}
            lands={me.lands}
            onHover={setHovered}
            seat="You"
          />
        </div>
      </div>
      <div className={`log${logOpen ? " log-open" : ""}`}>
        <div className="preview-section">
          {hovered && <Card card={hovered} />}
        </div>
        <div className="helper-strip">
          {helpers.map((label) => (
            <button key={label} className="helper-btn">
              {label}
            </button>
          ))}
          <button className="helper-btn helper-concede">Concede</button>
        </div>
        <SidePanel
          stack={sampleStack.slice(0, STACK_DEPTH[act] ?? 0)}
          log={sampleLog}
          chat={sampleChat}
          onHover={setHovered}
        />
      </div>
      <div className={`action-bar action-${action.tone}`}>
        <div className="action-text">
          <span className="action-prompt">{action.prompt}</span>
          {action.detail && <span className="action-phase">{action.detail}</span>}
        </div>
        <div className="action-btns">
          {action.buttons.map((button) => (
            <button
              key={button.label}
              className={`action-done${button.alt ? " action-alt" : ""}`}
            >
              {button.label}
            </button>
          ))}
        </div>
      </div>
      <Hand cards={sampleHand} onHover={setHovered} />
      <Arrows arrows={scene.arrows} />
      <PipDefs />
      {zone && (
        <ZoneView
          request={zone}
          onClose={() => setZone(null)}
          onChoose={() => setZone(null)}
        />
      )}
      {peeked && (
        <div className="peek" onClick={() => setPeeked(null)}>
          <Card card={peeked} />
        </div>
      )}
      {gear}
    </div>
    </ViewContext.Provider>
    </GlassContext.Provider>
    </PeekContext.Provider>
  );
}
