/**
 * The side column of a pre-game screen: one tabbed panel, in the same place and the same
 * vocabulary before and during a game (`docs/client-design.md` §9.4, §9.5).
 *
 * The lobby carries **Chat** and **Players**; the table room carries **Chat** and **Watching**.
 *
 * **Two of those have nothing behind them yet.** The protocol carries no chat and no roster of
 * who is where — `LobbyView` states the directory, the room, and your own seat, and nothing about
 * anybody else's presence. The panel is drawn anyway, with the count it *can* state and an empty
 * state saying plainly what is missing, because the alternative is a screen that changes shape
 * the day the command is added. Nothing here is faked: an empty list says it is empty, and the
 * chat field cannot be typed into, because a message this client could send has no command to
 * carry it.
 */
import { useState } from 'react'

export interface SideTab {
  id: string
  label: string
  /** A number the server stated. Absent where there is nothing to count. */
  count?: number
  /** What to say when the tab has nothing in it — including because nothing carries it yet. */
  empty: string
  /** Whether this tab is the chat, which is the one with a field under it. */
  chat?: boolean
}

export function SidePanel({ tabs, open }: { tabs: readonly SideTab[]; open: boolean }) {
  const [current, setCurrent] = useState(tabs[0]?.id)
  const tab = tabs.find((entry) => entry.id === current) ?? tabs[0]

  return (
    <div className={`lobby-side${open ? ' side-open' : ''}`}>
      <div className="panel">
        <div className="panel-tabs">
          {tabs.map((entry) => (
            <button
              key={entry.id}
              className={`panel-tab${entry.id === tab?.id ? ' panel-tab-on' : ''}`}
              onClick={() => setCurrent(entry.id)}
            >
              {entry.label}
              {entry.count !== undefined && <span className="panel-count">{entry.count}</span>}
            </button>
          ))}
        </div>
        <div className="panel-body">
          <div className="panel-empty">{tab?.empty}</div>
        </div>
      </div>
      {tab?.chat && (
        <div className="chat-entry">
          <input
            className="chat-input"
            aria-label="Say something"
            placeholder="Say something"
            disabled
          />
        </div>
      )}
    </div>
  )
}
