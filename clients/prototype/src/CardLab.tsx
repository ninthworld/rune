import { useState, type CSSProperties } from "react";
import { Card, type CardData, type Counter } from "./Card";
import { ALL_PIPS, Pip, PipDefs } from "./Pips";

const samples: CardData[] = [
  {
    name: "Angel of Sanctions",
    art: "Angel of Sanctions",
    cost: ["3", "W", "W"],
    typeLine: "Creature - Angel",
    text: "Flying\nWhen Angel of Sanctions enters the battlefield, you may exile target nonland permanent an opponent controls until Angel of Sanctions leaves the battlefield.\nEmbalm {5}{W}",
    pt: "3/4",
    set: "s1",
  },
  {
    name: "Bramble Sentinel",
    art: "Grizzly Bears",
    cost: ["1", "G"],
    typeLine: "Creature — Elf Warrior",
    text: "Vigilance\nWhenever this creature attacks, you gain 1 life.",
    pt: "2/3",
  },
  {
    name: "Archivist of the Forgotten Vault",
    art: "Archivist",
    cost: ["2", "U", "U"],
    typeLine: "Legendary Creature — Vedalken Wizard",
    text: "Flying, vigilance\nWhenever you draw your second card each turn, create a token that is a copy of this creature, except it isn't legendary.\nAt the beginning of your end step, return a card from your graveyard to your hand.",
    pt: "2/4",
  },
  {
    name: "Riftblade Duelist",
    art: "Boros Swiftblade",
    cost: ["1", "R", "W"],
    typeLine: "Creature — Human Soldier",
    text: "Double strike\nWhen this creature enters, it deals 1 damage to each opponent.",
    pt: "3/1",
  },
  {
    name: "Cinder Bolt",
    art: "Lightning Strike",
    cost: ["1", "R"],
    typeLine: "Instant",
    text: "Deals 3 damage to any target.",
  },
  {
    name: "Forest",
    art: "Forest",
    cost: [],
    typeLine: "Basic Land — Forest",
    text: "{T}: Add {G}.",
  },
  /* one card carrying every kind of pip at once, so the set can be judged
     at the size a cost line actually gets rather than on the sheet */
  {
    name: "Rimewrought Zealot",
    art: "Rimewind Cryomancer",
    cost: ["2", "W/U", "B/P"],
    typeLine: "Snow Creature — Human Cleric",
    text:
      "{T}: Add {W/U}.\n{2/R}: This creature gets +1/+0 until end of turn.\n" +
      "{S}{Q}: Untap another target creature. Pay {½} or {∞}.",
    pt: "2/3",
  },
];

const SIZES = [600, 420, 300, 220, 160, 120, 90];

/* the modifier states, at a preview size and at the size a permanent
   actually gets on a crowded board */
const MOD_SIZES = [300, 120];

const PLUS = { label: "+1/+1", pt: 1 };
const MINUS = { label: "−1/−1", pt: -1 };

const MODS: { label: string; counters?: Counter[]; sick?: boolean }[] = [
  { label: "printed" },
  { label: "one +1/+1", counters: [{ ...PLUS, n: 1 }] },
  { label: "three +1/+1", counters: [{ ...PLUS, n: 3 }] },
  { label: "two −1/−1", counters: [{ ...MINUS, n: 2 }] },
  { label: "time", counters: [{ label: "time", n: 4 }] },
  { label: "two kinds", counters: [{ ...PLUS, n: 2 }, { label: "stun", n: 1 }] },
  { label: "sick", sick: true },
  { label: "sick, one +1/+1", counters: [{ ...PLUS, n: 1 }], sick: true },
  /* the digits the plaque has to survive when a game gets out of hand */
  { label: "×12", counters: [{ ...PLUS, n: 12 }] },
  { label: "×100", counters: [{ ...PLUS, n: 100 }] },
];

function ModSheet({ card }: { card: CardData }) {
  return (
    <>
      {MOD_SIZES.map((height) => (
        <section key={height}>
          <div className="lab-label">
            {height}px · {(height / 600).toFixed(2)}×
          </div>
          <div
            className="lab-row"
            style={{ "--lab-s": `${height / 600}px` } as CSSProperties}
          >
            {MODS.map((mod) => (
              <div key={mod.label} className="mod-cell">
                <Card card={card} counters={mod.counters} sick={mod.sick} />
                <div className="lab-label">{mod.label}</div>
              </div>
            ))}
          </div>
        </section>
      ))}
    </>
  );
}

/* the sheet: every pip at the sizes it actually gets — the lab, a cost
   line on a preview, a cost line on a board card, and rules text */
const PIP_SIZES = [44, 22, 14, 10];

function PipSheet() {
  return (
    <div className="pip-sheet">
      {Object.entries(ALL_PIPS).map(([group, symbols]) => (
        <section key={group}>
          <div className="lab-label">{group}</div>
          {PIP_SIZES.map((size) => (
            <div
              key={size}
              className="pip-line"
              style={{ "--pip": `${size}px` } as CSSProperties}
            >
              <span className="pip-size">{size}px</span>
              {symbols.map((symbol) => (
                <Pip key={symbol} symbol={symbol} />
              ))}
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

/* the lab shows what the board is showing, so the same switch reaches it */
export function CardLab({ onBack }: { onBack: () => void }) {
  const [selected, setSelected] = useState(0);
  const [sheet, setSheet] = useState(false);
  const [mods, setMods] = useState(false);
  const card = samples[selected];
  return (
    <div className="lab">
      <PipDefs />
      <div className="topbar lab-topbar">
        <button className="view-btn" onClick={onBack}>
          ← Board
        </button>
        <button
          className={`view-btn${sheet ? " lab-picked" : ""}`}
          onClick={() => setSheet((on) => !on)}
        >
          Pips
        </button>
        <button
          className={`view-btn${mods ? " lab-picked" : ""}`}
          onClick={() => setMods((on) => !on)}
        >
          Modifiers
        </button>
        <div className="lab-picker">
          {samples.map((sample, i) => (
            <button
              key={sample.name}
              className={`helper-btn${i === selected ? " lab-picked" : ""}`}
              onClick={() => setSelected(i)}
            >
              {sample.name}
            </button>
          ))}
        </div>
      </div>
      <div className="lab-content">
        {sheet && <PipSheet />}
        {!sheet && mods && <ModSheet card={card} />}
        {!sheet && !mods && SIZES.map((height) => (
          <section key={height}>
            <div className="lab-label">
              {height}px · {(height / 600).toFixed(2)}× · flat / glass
            </div>
            <div
              className="lab-row"
              style={{ "--lab-s": `${height / 600}px` } as CSSProperties}
            >
              <Card card={card} glass={false} />
              <Card card={card} glass />
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
