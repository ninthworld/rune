import { useState } from "react";
import { Card, type CardData } from "./Card";

/* The sidebar's lower half: three things that all want the same column,
   so they share it as tabs rather than stacking and starving each other.

   Stack comes first because it is the only one you have to read *now* —
   it is the game asking you to respond. Its tab carries a count, so a
   spell going on the stack is visible from the Log or Chat tab too. */
export type StackItem = {
  card: CardData;
  who: string;
  /* what is actually on the stack: the spell, or an ability the card made */
  kind: string;
  targets?: string[];
};

/* the log is a run of entries broken by the turn that produced them */
export type LogLine = { turn: string } | { text: string };
export type ChatLine = { who: string; text: string };

const TABS = ["Stack", "Log", "Chat"] as const;
type Tab = (typeof TABS)[number];

export function SidePanel({
  stack,
  log,
  chat,
  onHover,
}: {
  stack: StackItem[];
  log: LogLine[];
  chat: ChatLine[];
  onHover?: (card: CardData | null) => void;
}) {
  const [tab, setTab] = useState<Tab>("Stack");
  return (
    <div className="panel">
      <div className="panel-tabs">
        {TABS.map((name) => (
          <button
            key={name}
            className={`panel-tab${tab === name ? " panel-tab-on" : ""}`}
            onClick={() => setTab(name)}
          >
            {name}
            {name === "Stack" && stack.length > 0 && (
              <span className="panel-count">{stack.length}</span>
            )}
          </button>
        ))}
      </div>

      <div className="panel-body">
        {tab === "Stack" &&
          (stack.length === 0 ? (
            <div className="panel-empty">The stack is empty.</div>
          ) : (
            <>
              <div className="stack-head">resolves next</div>
              {stack.map((item, i) => (
                <div
                  key={i}
                  className="stack-item"
                  onMouseEnter={onHover && (() => onHover(item.card))}
                  onMouseLeave={onHover && (() => onHover(null))}
                >
                  <div className="stack-thumb">
                    <Card card={item.card} />
                  </div>
                  <div className="stack-text">
                    <div className="stack-name">{item.card.name}</div>
                    <div className="stack-kind">
                      {item.who} · {item.kind}
                    </div>
                    {item.targets && item.targets.length > 0 && (
                      <div className="stack-target">→ {item.targets.join(", ")}</div>
                    )}
                  </div>
                </div>
              ))}
            </>
          ))}

        {tab === "Log" &&
          log.map((line, i) =>
            "turn" in line ? (
              <div key={i} className="log-turn">
                {line.turn}
              </div>
            ) : (
              <div key={i} className="log-line">
                {line.text}
              </div>
            ),
          )}

        {tab === "Chat" &&
          chat.map((line, i) => (
            <div key={i} className="chat-line">
              <span className="chat-who">{line.who}</span>
              {line.text}
            </div>
          ))}
      </div>

      {tab === "Chat" && (
        <div className="chat-entry">
          <input className="chat-input" placeholder="Say something…" />
        </div>
      )}
    </div>
  );
}
