import { useState } from "react";

type Table = {
  name: string;
  host: string;
  format: string;
  seats: number;
  taken: number;
  locked?: boolean;
};

const tables: Table[] = [
  { name: "Ravnica Nights", host: "Alice", format: "Commander", seats: 4, taken: 3 },
  { name: "Bolt the Bird", host: "Bob", format: "Modern", seats: 2, taken: 1 },
  { name: "Casual Cube", host: "Cora", format: "Draft", seats: 8, taken: 6 },
  { name: "Testing Room", host: "Dev", format: "Commander", seats: 4, taken: 1 },
  { name: "No Rares Allowed", host: "Eli", format: "Pauper", seats: 2, taken: 2 },
  { name: "Friday Pod", host: "Fay", format: "Commander", seats: 4, taken: 2, locked: true },
  { name: "Legacy Grinder", host: "Gus", format: "Legacy", seats: 2, taken: 1 },
  { name: "Eight-Player Chaos", host: "Hana", format: "Commander", seats: 8, taken: 7 },
  { name: "Deck Testing", host: "Ivo", format: "Modern", seats: 2, taken: 2 },
  { name: "Sunday Cube", host: "Jae", format: "Draft", seats: 8, taken: 2, locked: true },
];

const formats = ["All", "Commander", "Modern", "Draft", "Pauper", "Legacy"];
const NEW_FORMATS = ["Commander", "Modern", "Draft", "Pauper", "Legacy"];

const roster = [
  { name: "Alice", where: "Ravnica Nights" },
  { name: "Bob", where: "Bolt the Bird" },
  { name: "Cora", where: "Casual Cube" },
  { name: "Dev", where: "Testing Room" },
  { name: "Eli", where: "No Rares Allowed" },
  { name: "Fay", where: "Friday Pod" },
  { name: "Gus", where: "Legacy Grinder" },
  { name: "Hana", where: null },
  { name: "Ivo", where: null },
  { name: "Jae", where: "Sunday Cube" },
  { name: "Kit", where: null },
  { name: "Lux", where: null },
];

const chat = [
  { who: "Alice", text: "one seat left in Ravnica Nights" },
  { who: "Gus", text: "anyone up for legacy? no combo pls" },
  { who: "Hana", text: "chaos pod filling fast, 7/8" },
  { who: "Kit", text: "just here to watch" },
  { who: "Cora", text: "cube starts in 5, grab a seat" },
];

function Seats({ taken, seats }: { taken: number; seats: number }) {
  return (
    <span className="seats">
      <span className="seat-dots">
        {Array.from({ length: seats }, (_, i) => (
          <span key={i} className={`seat-dot${i < taken ? " seat-taken" : ""}`} />
        ))}
      </span>
      <span className="seat-count">
        {taken}/{seats}
      </span>
    </span>
  );
}

function CreateTable({
  who,
  onClose,
  onCreate,
}: {
  who: string;
  onClose: () => void;
  onCreate: (name: string) => void;
}) {
  const [name, setName] = useState(`${who}'s table`);
  const [format, setFormat] = useState("Commander");
  const [seats, setSeats] = useState(4);
  const [open, setOpen] = useState(true);
  const [undo, setUndo] = useState(true);
  return (
    <div className="zone-view" onClick={onClose}>
      <form
        className="zone-panel new-table"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          onCreate(name.trim());
        }}
      >
        <div className="zone-head">
          <span className="zone-title">New table</span>
          <span className="zone-tally" />
          <button type="button" className="zone-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="new-body">
          <label className="connect-field">
            <span className="connect-label">Name</span>
            <input
              className="connect-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={32}
              autoFocus
            />
          </label>
          <div className="connect-field">
            <span className="connect-label">Format</span>
            <span className="seg new-seg">
              {NEW_FORMATS.map((f) => (
                <button
                  key={f}
                  type="button"
                  className={`seg-btn${format === f ? " seg-on" : ""}`}
                  onClick={() => setFormat(f)}
                >
                  {f}
                </button>
              ))}
            </span>
          </div>
          <div className="connect-field">
            <span className="connect-label">Seats</span>
            <span className="seat-step new-step">
              <button
                type="button"
                className="step-btn"
                disabled={seats <= 2}
                onClick={() => setSeats((n) => Math.max(2, n - 1))}
              >
                −
              </button>
              <span className="seat-num">{seats} players</span>
              <button
                type="button"
                className="step-btn"
                disabled={seats >= 8}
                onClick={() => setSeats((n) => Math.min(8, n + 1))}
              >
                +
              </button>
            </span>
          </div>
          <div className="connect-field">
            <span className="connect-label">Access</span>
            <span className="seg new-seg">
              <button
                type="button"
                className={`seg-btn${open ? " seg-on" : ""}`}
                onClick={() => setOpen(true)}
              >
                Open
              </button>
              <button
                type="button"
                className={`seg-btn${open ? "" : " seg-on"}`}
                onClick={() => setOpen(false)}
              >
                Invite only
              </button>
            </span>
          </div>
          <div className="connect-field">
            <span className="connect-label">Undo</span>
            <span className="seg new-seg">
              <button
                type="button"
                className={`seg-btn${undo ? " seg-on" : ""}`}
                onClick={() => setUndo(true)}
              >
                Allowed
              </button>
              <button
                type="button"
                className={`seg-btn${undo ? "" : " seg-on"}`}
                onClick={() => setUndo(false)}
              >
                Not allowed
              </button>
            </span>
            <span className="new-note">
              {undo
                ? "A player may take back an action the game has not answered yet."
                : "Every action stands once it is taken."}
            </span>
          </div>
        </div>
        <div className="zone-foot">
          <span className="zone-hint">
            {format} · {seats} seats · {open ? "anyone may join" : "invite only"} ·{" "}
            {undo ? "undo allowed" : "no undo"}
          </span>
          <div className="zone-acts">
            <button className="action-done" disabled={name.trim() === ""}>
              Create
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}

export function Lobby({
  who,
  server,
  onBack,
  onConnect,
  onJoin,
  onSettings,
}: {
  who: string;
  server: string;
  onBack: () => void;
  onConnect: () => void;
  onJoin: (table: string) => void;
  onSettings: () => void;
}) {
  const [query, setQuery] = useState("");
  const [format, setFormat] = useState("All");
  const [openOnly, setOpenOnly] = useState(false);
  const [tab, setTab] = useState<"chat" | "players">("chat");
  const [creating, setCreating] = useState(false);
  const [panelOpen, setPanelOpen] = useState(() => window.innerWidth > 900);

  const shown = tables.filter(
    (t) =>
      (format === "All" || t.format === format) &&
      (!openOnly || t.taken < t.seats) &&
      (t.name.toLowerCase().includes(query.toLowerCase()) ||
        t.host.toLowerCase().includes(query.toLowerCase())),
  );

  return (
    <div className={`lobby${panelOpen ? "" : " side-hidden"}`}>
      <div className="topbar lobby-topbar">
        <button className="view-btn" onClick={onBack}>
          ← Board
        </button>
        <button className="view-btn" onClick={onConnect}>
          Connect
        </button>
        <span className="topbar-fill" />
        <span className="lobby-who">
          <b>{who}</b>
          <span className="lobby-server">{server}</span>
        </span>
        <button className="settings-btn" title="Settings" onClick={onSettings}>
          ⚙
        </button>
        <button
          className="menu-btn"
          title="Chat and players"
          onClick={() => setPanelOpen((on) => !on)}
        >
          ☰
        </button>
      </div>

      <div className="lobby-main">
        <div className="lobby-head">
          <span className="lobby-title">Open tables</span>
          <span className="lobby-tally">
            {shown.length} of {tables.length}
          </span>
          <button className="action-done lobby-new" onClick={() => setCreating(true)}>
            + Create table
          </button>
        </div>

        <div className="filter-strip">
          <input
            className="connect-input filter-search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search tables or hosts"
          />
          <span className="seg">
            {formats.map((f) => (
              <button
                key={f}
                className={`seg-btn${format === f ? " seg-on" : ""}`}
                onClick={() => setFormat(f)}
              >
                {f}
              </button>
            ))}
          </span>
          <button
            className={`view-btn${openOnly ? " lab-picked" : ""}`}
            onClick={() => setOpenOnly((on) => !on)}
          >
            Open seats only
          </button>
        </div>

        <div className="table-list">
          {shown.map((t) => {
            const full = t.taken >= t.seats;
            return (
              <div key={t.name} className={`table-row${full ? " table-full" : ""}`}>
                <span className={`table-dot${full ? " dot-full" : ""}`} />
                <span className="table-id">
                  <span className="table-name">
                    {t.name}
                    {t.locked && <span className="table-lock" title="Invite only">🔒</span>}
                  </span>
                  <span className="table-host">hosted by {t.host}</span>
                </span>
                <span className="table-format">{t.format}</span>
                <Seats taken={t.taken} seats={t.seats} />
                <button
                  className="action-done table-join"
                  disabled={full}
                  onClick={() => onJoin(t.name)}
                >
                  {full ? "Full" : "Join"}
                </button>
              </div>
            );
          })}
          {shown.length === 0 && (
            <div className="zone-empty">No table matches those filters.</div>
          )}
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
              className={`panel-tab${tab === "players" ? " panel-tab-on" : ""}`}
              onClick={() => setTab("players")}
            >
              Players <span className="panel-count">{roster.length}</span>
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
            {tab === "players" &&
              roster.map((p) => (
                <div key={p.name} className="roster-line">
                  <span className={`table-dot${p.where ? " dot-full" : ""}`} />
                  <span className="roster-name">{p.name}</span>
                  <span className="roster-where">{p.where ?? "in lobby"}</span>
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

      {creating && (
        <CreateTable
          who={who}
          onClose={() => setCreating(false)}
          onCreate={onJoin}
        />
      )}
    </div>
  );
}
