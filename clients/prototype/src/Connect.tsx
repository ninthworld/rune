import { useState } from "react";

type Server = { id: string; name: string; host: string; ping?: string };

const servers: Server[] = [
  { id: "na", name: "SAGE Official", host: "play.sage.gg", ping: "24 ms" },
  { id: "eu", name: "SAGE Europe", host: "eu.sage.gg", ping: "98 ms" },
  { id: "local", name: "Localhost", host: "ws://localhost:8080", ping: "1 ms" },
  { id: "custom", name: "Custom server", host: "" },
];

export function Connect({
  onBack,
  onConnect,
}: {
  onBack: () => void;
  onConnect: (who: string, server: string) => void;
}) {
  const [name, setName] = useState("");
  const [pick, setPick] = useState("na");
  const [custom, setCustom] = useState("");
  const ready = name.trim() !== "" && (pick !== "custom" || custom.trim() !== "");
  const host =
    pick === "custom" ? custom : servers.find((s) => s.id === pick)?.host ?? "";
  return (
    <div className="connect">
      <div className="topbar lab-topbar">
        <button className="view-btn" onClick={onBack}>
          ← Board
        </button>
      </div>
      <div className="connect-stage">
        <form
          className="connect-panel"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready) onConnect(name.trim(), host);
          }}
        >
          <div className="connect-mark">
            <div className="connect-title">SAGE</div>
            <div className="connect-sub">
              <b>S</b>erver <b>A</b>uthoritative <b>G</b>ame <b>E</b>ngine
            </div>
          </div>

          <label className="connect-field">
            <span className="connect-label">Display name</span>
            <input
              className="connect-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="How the table sees you"
              maxLength={24}
              autoFocus
            />
          </label>

          <div className="connect-field">
            <span className="connect-label">Server</span>
            <div className="server-list" role="radiogroup">
              {servers.map((server) => (
                <button
                  key={server.id}
                  type="button"
                  role="radio"
                  aria-checked={pick === server.id}
                  className={`server-row${pick === server.id ? " server-on" : ""}`}
                  onClick={() => setPick(server.id)}
                >
                  <span className="server-dot" />
                  <span className="server-name">{server.name}</span>
                  <span className="server-host">
                    {server.id === "custom" ? custom || "—" : server.host}
                  </span>
                  <span className="server-ping">{server.ping ?? ""}</span>
                </button>
              ))}
            </div>
            {pick === "custom" && (
              <input
                className="connect-input"
                value={custom}
                onChange={(e) => setCustom(e.target.value)}
                placeholder="ws://host:port"
                autoFocus
              />
            )}
          </div>

          <button className="action-done connect-go" disabled={!ready}>
            Connect
          </button>
        </form>
      </div>
    </div>
  );
}
