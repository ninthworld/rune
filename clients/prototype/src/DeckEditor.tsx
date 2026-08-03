import { useState } from "react";
import { Card, type CardData } from "./Card";
import { Pip } from "./Pips";

export type Entry = { n: number; card: CardData };

function colorsOf(card: CardData) {
  const pips = card.cost.filter((c) => /^[WUBRG]$/.test(c));
  return [...new Set(pips)];
}

function total(list: Entry[]) {
  return list.reduce((sum, e) => sum + e.n, 0);
}

/* moving one copy across the line, collapsing an entry that runs out */
function move(from: Entry[], to: Entry[], card: CardData): [Entry[], Entry[]] {
  const next = from
    .map((e) => (e.card.name === card.name ? { ...e, n: e.n - 1 } : e))
    .filter((e) => e.n > 0);
  const there = to.find((e) => e.card.name === card.name);
  const grown = there
    ? to.map((e) => (e.card.name === card.name ? { ...e, n: e.n + 1 } : e))
    : [...to, { n: 1, card }];
  return [next, grown];
}

function Pane({
  title,
  list,
  arrow,
  onMove,
  onHover,
}: {
  title: string;
  list: Entry[];
  arrow: string;
  onMove: (card: CardData) => void;
  onHover: (card: CardData) => void;
}) {
  return (
    <div className="edit-pane">
      <div className="edit-head">
        <span className="edit-title">{title}</span>
        <span className="edit-count">{total(list)}</span>
      </div>
      <div className="edit-list">
        {list.map((e) => (
          <button
            key={e.card.name}
            className="edit-row"
            onClick={() => onMove(e.card)}
            onMouseEnter={() => onHover(e.card)}
            title={`Move one to the other list`}
          >
            <span className="edit-n">{e.n}</span>
            <span className="deck-colors">
              {colorsOf(e.card).map((c) => (
                <Pip key={c} symbol={c} />
              ))}
            </span>
            <span className="edit-name">{e.card.name}</span>
            <span className="edit-arrow">{arrow}</span>
          </button>
        ))}
        {list.length === 0 && <div className="edit-empty">Nothing here yet.</div>}
      </div>
    </div>
  );
}

export function DeckEditor({
  deck,
  main,
  side,
  onSave,
  onClose,
}: {
  deck: string;
  main: Entry[];
  side: Entry[];
  onSave: (main: Entry[], side: Entry[]) => void;
  onClose: () => void;
}) {
  const [m, setM] = useState(main);
  const [s, setS] = useState(side);
  const [shown, setShown] = useState<CardData | null>(main[0]?.card ?? null);
  return (
    <div className="zone-view" onClick={onClose}>
      <div className="zone-panel edit-panel" onClick={(e) => e.stopPropagation()}>
        <div className="zone-head">
          <span className="zone-title">{deck}</span>
          <span className="zone-tally">click a card to move it across</span>
          <button className="zone-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="edit-body">
          <Pane
            title="Main deck"
            list={m}
            arrow="→"
            onHover={setShown}
            onMove={(card) => {
              const [a, b] = move(m, s, card);
              setM(a);
              setS(b);
            }}
          />
          <Pane
            title="Sideboard"
            list={s}
            arrow="←"
            onHover={setShown}
            onMove={(card) => {
              const [a, b] = move(s, m, card);
              setS(a);
              setM(b);
            }}
          />
          <div className="edit-pane edit-side">
            <div className="edit-head">
              <span className="edit-title">{shown ? shown.name : "Card"}</span>
            </div>
            <div className="edit-preview">{shown && <Card card={shown} />}</div>
          </div>
        </div>
        <div className="zone-foot">
          <span className="zone-hint">
            {total(m)} in the deck · {total(s)} on the side
          </span>
          <div className="zone-acts">
            <button className="action-done action-alt" onClick={onClose}>
              Cancel
            </button>
            <button
              className="action-done"
              onClick={() => {
                onSave(m, s);
                onClose();
              }}
            >
              Save
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
