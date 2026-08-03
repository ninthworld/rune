import { useState, type CSSProperties } from "react";
import { Card, type CardData } from "./Card";
import { Pip, PipDefs } from "./Pips";
import { DeckEditor, type Entry } from "./DeckEditor";

type Deck = {
  name: string;
  colors: string[];
  cards: number;
  commander: CardData;
};

function cmdr(
  name: string,
  art: string,
  cost: string[],
  typeLine: string,
  text: string,
  pt: string,
): CardData {
  return { name, art, cost, typeLine: `Legendary Creature — ${typeLine}`, text, pt };
}

const decks: Deck[] = [
  {
    name: "Selesnya Angels",
    colors: ["G", "W"],
    cards: 100,
    commander: cmdr("Isolde, Sword of Dawn", "Angel of Sanctions", ["2", "G", "W"],
      "Angel Knight", "Flying, vigilance\nOther creatures you control get +1/+1.", "4/4"),
  },
  {
    name: "Dimir Mill",
    colors: ["U", "B"],
    cards: 100,
    commander: cmdr("Vess, the Quiet Tide", "Archivist", ["1", "U", "B"],
      "Vedalken Rogue", "Whenever this creature deals combat damage, each opponent mills three cards.", "2/3"),
  },
  {
    name: "Mono-Red Aggro",
    colors: ["R"],
    cards: 60,
    commander: cmdr("Kagan, Ember Chief", "Raging Goblin", ["1", "R"],
      "Goblin Warrior", "Haste\nWhenever this creature attacks, it gets +1/+0 until end of turn.", "2/2"),
  },
  {
    name: "Bant Blink",
    colors: ["G", "W", "U"],
    cards: 100,
    commander: cmdr("Ferro of the Long Watch", "Wind Drake", ["1", "G", "W", "U"],
      "Bird Wizard", "Flying\nWhen this creature enters, exile another target creature you control, then return it.", "3/4"),
  },
  {
    name: "Colorless Eldrazi",
    colors: ["C"],
    cards: 60,
    commander: cmdr("Ulmoth, the Hollow", "Juggernaut", ["6"],
      "Eldrazi", "Trample\nWhen this creature enters, exile the top two cards of each library.", "6/6"),
  },
  {
    name: "Jund Sacrifice",
    colors: ["B", "R", "G"],
    cards: 100,
    commander: cmdr("Grisla, Bone Broker", "Vampire Nighthawk", ["1", "B", "R", "G"],
      "Human Shaman", "Sacrifice another creature: Each opponent loses 1 life and you draw a card.", "3/3"),
  },
];

/* one deck list stands in for every deck — the editor is about moving
   cards across the line, not about which cards these are */
const sentinel: CardData = { name: "Bramble Sentinel", art: "Grizzly Bears", cost: ["1", "G"], typeLine: "Creature — Elf Warrior", text: "Vigilance\nWhenever this creature attacks, you gain 1 life.", pt: "2/3" };
const herald: CardData = { name: "Dawn Herald", art: "Ajani's Sunstriker", cost: ["2", "W"], typeLine: "Creature — Human Cleric", text: "Lifelink\nWhen this creature dies, you gain 2 life.", pt: "2/2" };
const colossus: CardData = { name: "Moss Colossus", art: "Craw Wurm", cost: ["3", "G", "G"], typeLine: "Creature — Elemental", text: "Trample", pt: "5/5" };
const skirmisher: CardData = { name: "Dawn Skirmisher", art: "Youthful Knight", cost: ["W"], typeLine: "Creature — Human Soldier", text: "First strike", pt: "1/1" };
const blade: CardData = { name: "Ancestral Blade", art: "Bonesplitter", cost: ["1", "W"], typeLine: "Artifact — Equipment", text: "Equipped creature gets +1/+1 and has vigilance.\nEquip {2}" };
const ward: CardData = { name: "Sunlit Ward", art: "Glorious Anthem", cost: ["1", "W"], typeLine: "Enchantment", text: "Creatures you control get +0/+1." };
const vines: CardData = { name: "Wall of Vines", art: "Wall of Vines", cost: ["G"], typeLine: "Creature — Plant Wall", text: "Defender, reach", pt: "0/3" };
const angel: CardData = { name: "Angel of Sanctions", art: "Angel of Sanctions", cost: ["3", "W", "W"], typeLine: "Creature — Angel", text: "Flying\nWhen this creature enters, you may exile target nonland permanent an opponent controls until this creature leaves the battlefield.", pt: "3/4" };
const boar: CardData = { name: "Thornback Boar", art: "Charging Rhino", cost: ["2", "G"], typeLine: "Creature — Boar", text: "Trample", pt: "3/3" };
const sentry: CardData = { name: "Iron Sentinel", art: "Juggernaut", cost: ["4"], typeLine: "Artifact Creature — Golem", text: "Vigilance", pt: "3/3" };
const forest: CardData = { name: "Forest", art: "Forest", cost: [], typeLine: "Basic Land — Forest", text: "{T}: Add {G}." };
const plains: CardData = { name: "Plains", art: "Plains", cost: [], typeLine: "Basic Land — Plains", text: "{T}: Add {W}." };

const mainDeck: Entry[] = [
  { n: 4, card: sentinel },
  { n: 4, card: herald },
  { n: 4, card: skirmisher },
  { n: 3, card: colossus },
  { n: 3, card: blade },
  { n: 3, card: ward },
  { n: 2, card: vines },
  { n: 1, card: angel },
  { n: 20, card: forest },
  { n: 16, card: plains },
];

const sideDeck: Entry[] = [
  { n: 4, card: boar },
  { n: 3, card: sentry },
  { n: 3, card: ward },
  { n: 3, card: vines },
  { n: 2, card: angel },
];

type Guest = { name: string; deck?: Deck; ready: boolean };

const guests: Guest[] = [
  { name: "Alice", deck: decks[1], ready: true },
  { name: "Bob", ready: false },
  { name: "Cora", deck: decks[3], ready: true },
  { name: "Dev", deck: decks[2], ready: false },
  { name: "Eli", deck: decks[5], ready: true },
  { name: "Fay", deck: decks[4], ready: true },
  { name: "Gus", deck: decks[0], ready: true },
];

const spectators = ["Kit", "Lux", "Mo"];

const chat = [
  { who: "Alice", text: "welcome in" },
  { who: "Chris", text: "one sec, swapping decks" },
  { who: "Cora", text: "no rush" },
  { who: "Bob", text: "what's the mulligan rule here?" },
  { who: "Alice", text: "london, same as always" },
];

function Colors({ colors }: { colors: string[] }) {
  return (
    <span className="deck-colors">
      {colors.map((c) => (
        <Pip key={c} symbol={c} />
      ))}
    </span>
  );
}

function DeckPicker({
  current,
  onPick,
  onClose,
}: {
  current: Deck;
  onPick: (deck: Deck) => void;
  onClose: () => void;
}) {
  const [pick, setPick] = useState(current);
  return (
    <div className="zone-view" onClick={onClose}>
      <div className="zone-panel deck-panel" onClick={(e) => e.stopPropagation()}>
        <div className="zone-head">
          <span className="zone-title">Choose a deck</span>
          <span className="zone-tally">{decks.length} decks</span>
          <button className="zone-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="deck-list">
          {decks.map((deck) => (
            <button
              key={deck.name}
              className={`deck-row${deck.name === pick.name ? " deck-on" : ""}`}
              onClick={() => setPick(deck)}
            >
              <Colors colors={deck.colors} />
              <span className="deck-name">{deck.name}</span>
              <span className="deck-count">{deck.cards} cards</span>
            </button>
          ))}
        </div>
        <div className="zone-foot">
          <span className="zone-hint">{pick.name}</span>
          <div className="zone-acts">
            <button className="action-done action-alt" onClick={onClose}>
              Cancel
            </button>
            <button
              className="action-done"
              onClick={() => {
                onPick(pick);
                onClose();
              }}
            >
              Choose
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Seat({
  name,
  deck,
  ready,
  host,
  mine,
  canKick,
  commander,
  onDeck,
  onEdit,
}: {
  name: string;
  deck?: Deck;
  ready: boolean;
  host: boolean;
  mine: boolean;
  canKick: boolean;
  commander: boolean;
  onDeck: () => void;
  onEdit: () => void;
}) {
  return (
    <div className={`seat${mine ? " seat-mine" : ""}${ready ? " seat-ready" : ""}`}>
      <div className="seat-head">
        <span className={`table-dot ${ready ? "dot-ready" : "dot-full"}`} />
        <span className="seat-name">{name}</span>
        {host && <span className="seat-badge">Host</span>}
        {canKick && (
          <button className="seat-kick" title={`Remove ${name}`}>
            ✕
          </button>
        )}
      </div>
      {/* the deck's colors are what a seat reads as from across the room;
          in a commander game the card it is built around says it better */}
      <div className="seat-body">
        <div className="seat-ident">
          {deck && commander && (
            <span className="seat-cmdr">
              <Card card={deck.commander} />
            </span>
          )}
          {deck ? (
            <span className="seat-colors">
              {deck.colors.map((c) => (
                <Pip key={c} symbol={c} />
              ))}
            </span>
          ) : (
            <span className="seat-nodeck" />
          )}
        </div>
        {deck && commander && (
          <span className="seat-cmdr-name">{deck.commander.name}</span>
        )}
        <span className={`seat-deck-name${deck ? "" : " seat-deck-none"}`}>
          {deck ? deck.name : "No deck chosen"}
        </span>
      </div>
      <div className="seat-foot">
        <span className={`seat-state${ready ? " state-ready" : ""}`}>
          {ready ? "✓ Ready" : deck ? "Not ready" : "Picking a deck"}
        </span>
        {mine && (
          <span className="seat-acts">
            <button className="view-btn" onClick={onEdit}>
              Edit
            </button>
            <button className="view-btn" onClick={onDeck}>
              Change
            </button>
          </span>
        )}
      </div>
    </div>
  );
}

export function Room({
  who,
  table,
  onLeave,
  onStart,
  onSettings,
}: {
  who: string;
  table: string;
  onLeave: () => void;
  onStart: () => void;
  onSettings: () => void;
}) {
  const [seats, setSeats] = useState(4);
  const [host, setHost] = useState(true);
  const [ready, setReady] = useState(false);
  const [deck, setDeck] = useState(decks[0]);
  const [picking, setPicking] = useState(false);
  const [editing, setEditing] = useState(false);
  const [main, setMain] = useState(mainDeck);
  const [side, setSide] = useState(sideDeck);
  const [commander, setCommander] = useState(true);
  const [undo, setUndo] = useState(true);
  const [tab, setTab] = useState<"chat" | "watch">("chat");
  const [panelOpen, setPanelOpen] = useState(() => window.innerWidth > 900);

  /* one seat is left open above two players, so the empty state is on
     screen at every size the stepper reaches */
  const taken = Math.max(2, seats - 1);
  const others = guests.slice(0, taken - 1);
  const readyCount = 1 + others.filter((g) => g.ready).length - (ready ? 0 : 1);
  const allReady = ready && others.every((g) => g.ready);

  return (
    <div className={`lobby room${panelOpen ? "" : " side-hidden"}`}>
      <PipDefs />
      <div className="topbar lobby-topbar">
        <button className="view-btn" onClick={onLeave}>
          ← Leave table
        </button>
        <span className="seat-step">
          <button
            className="step-btn"
            title="Fewer seats"
            disabled={seats <= 2}
            onClick={() => setSeats((n) => Math.max(2, n - 1))}
          >
            −
          </button>
          <span className="seat-num">{seats} seats</span>
          <button
            className="step-btn"
            title="More seats"
            disabled={seats >= 8}
            onClick={() => setSeats((n) => Math.min(8, n + 1))}
          >
            +
          </button>
        </span>
        <button
          className={`view-btn${host ? " lab-picked" : ""}`}
          title="See the room as its host"
          onClick={() => setHost((on) => !on)}
        >
          Host
        </button>
        <button
          className={`view-btn${commander ? " lab-picked" : ""}`}
          title="A commander game names the card each deck is built around"
          onClick={() => setCommander((on) => !on)}
        >
          Commander
        </button>
        <button
          className={`view-btn${undo ? " lab-picked" : ""}`}
          title="Whether this table allows taking an action back"
          onClick={() => setUndo((on) => !on)}
        >
          Undo
        </button>
        <span className="topbar-fill" />
        <span className="lobby-who">
          <b>{who}</b>
          <span className="lobby-server">seated</span>
        </span>
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Chat and spectators"
          onClick={() => setPanelOpen((on) => !on)}
        >
          ☰
        </button>
      </div>

      <div className="lobby-main">
        <div className="lobby-head">
          <span className="lobby-title">{table}</span>
          <span className="lobby-tally">
            {taken} of {seats} seated
          </span>
        </div>

        {/* the table's own rules, on show rather than remembered */}
        <div className="filter-strip room-facts">
          <span className="fact">
            <b>{commander ? "Commander" : "Modern"}</b>
          </span>
          <span className="fact">{seats} seats</span>
          <span className="fact">Open to anyone</span>
          <span className="fact">London mulligan</span>
          <span className="fact">90s per turn</span>
          <span className={`fact${undo ? " fact-on" : " fact-off"}`}>
            {undo ? "Undo allowed" : "No undo"}
          </span>
        </div>

        <div
          className={`seat-grid${commander ? " cmdr-grid" : ""}`}
          style={{ "--seat-cols": Math.min(4, seats) } as CSSProperties}
        >
          <Seat
            name={who}
            deck={deck}
            ready={ready}
            host={host}
            mine
            canKick={false}
            commander={commander}
            onDeck={() => setPicking(true)}
            onEdit={() => setEditing(true)}
          />
          {others.map((g) => (
            <Seat
              key={g.name}
              name={g.name}
              deck={g.deck}
              ready={g.ready}
              host={false}
              mine={false}
              canKick={host}
              commander={commander}
              onDeck={() => {}}
              onEdit={() => {}}
            />
          ))}
          {Array.from({ length: seats - taken }, (_, i) => (
            <div key={i} className="seat seat-open">
              <span className="seat-open-label">Open seat</span>
              <button className="view-btn">Invite</button>
            </div>
          ))}
        </div>
      </div>

      <div className={`lobby-side${panelOpen ? " side-open" : ""}`}>
        <div className="panel">
          <div className="panel-tabs">
            <button
              className={`panel-tab${tab === "chat" ? " panel-tab-on" : ""}`}
              onClick={() => setTab("chat")}
            >
              Chat
            </button>
            <button
              className={`panel-tab${tab === "watch" ? " panel-tab-on" : ""}`}
              onClick={() => setTab("watch")}
            >
              Watching <span className="panel-count">{spectators.length}</span>
            </button>
          </div>
          <div className="panel-body">
            {tab === "chat" &&
              chat.map((line, i) => (
                <div key={i} className="chat-line">
                  <span className="chat-who">{line.who}</span>
                  {line.text}
                </div>
              ))}
            {tab === "watch" &&
              spectators.map((s) => (
                <div key={s} className="roster-line">
                  <span className="table-dot dot-full" />
                  <span className="roster-name">{s}</span>
                  <span className="roster-where">watching</span>
                </div>
              ))}
          </div>
        </div>
        {tab === "chat" && (
          <div className="chat-entry">
            <input className="chat-input" placeholder="Say something" />
          </div>
        )}
      </div>

      {/* the board's action bar, doing the same job before the game starts */}
      <div className={`action-bar room-foot ${allReady ? "action-green" : ""}`}>
        <div className="action-text">
          <span className="action-prompt">
            {allReady
              ? host
                ? "Everyone is ready"
                : "Waiting for the host to start"
              : `${readyCount} of ${taken} players ready`}
          </span>
          <span className="action-phase">
            {taken < seats
              ? `${seats - taken} seat${seats - taken > 1 ? "s" : ""} still open`
              : "Every seat is taken"}
          </span>
        </div>
        <div className="action-btns">
          <button
            className={`action-done${ready ? " action-alt" : ""}`}
            onClick={() => setReady((on) => !on)}
          >
            {ready ? "Not ready" : "Ready"}
          </button>
          {host && (
            <button className="action-done" disabled={!allReady} onClick={onStart}>
              Start game
            </button>
          )}
        </div>
      </div>

      {picking && (
        <DeckPicker current={deck} onPick={setDeck} onClose={() => setPicking(false)} />
      )}
      {editing && (
        <DeckEditor
          deck={deck.name}
          main={main}
          side={side}
          onSave={(m, s) => {
            setMain(m);
            setSide(s);
          }}
          onClose={() => setEditing(false)}
        />
      )}
    </div>
  );
}
