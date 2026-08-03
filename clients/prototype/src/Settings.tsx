import { useEffect, useRef, useState } from "react";
import { Card, type CardData } from "./Card";
import { PipDefs } from "./Pips";
import { ViewContext, type CardView } from "./glass";
import { cachedCards, clearArt, estimateBytes } from "./art";

/* every card the prototype knows how to draw — the ceiling the art
   section reports against */
const SUPPORTED = 3180;

const sample: CardData = {
  name: "Bramble Sentinel",
  art: "Grizzly Bears",
  cost: ["1", "G"],
  typeLine: "Creature — Elf Warrior",
  text: "Vigilance\nWhenever this creature attacks, you gain 1 life.",
  pt: "2/3",
};

const faces: { key: CardView; label: string; note: string }[] = [
  { key: "frame", label: "Frame only", note: "Nothing is fetched. The card is entirely ours." },
  { key: "art", label: "Frame and art", note: "The illustration alone, inside SAGE's frame." },
  { key: "full", label: "Full card", note: "The printed card face, fetched whole." },
];

function mb(bytes: number) {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${(bytes / 1e6).toFixed(1)} MB`;
}

function Cards({
  face,
  onFace,
  fetching,
  onArt,
}: {
  face: CardView;
  onFace: (face: CardView) => void;
  fetching: boolean;
  onArt: () => void;
}) {
  return (
    <section className="set-section">
      <h3 className="set-head">How cards are drawn</h3>
      <p className="set-note">
        Applies everywhere a card appears — the board, your hand, the preview.
      </p>
      <div className="face-row">
        {faces.map((f) => {
          const off = !fetching && f.key !== "frame";
          return (
            <button
              key={f.key}
              className={`face-tile${face === f.key ? " face-on" : ""}${off ? " face-off" : ""}`}
              disabled={off}
              onClick={() => onFace(f.key)}
            >
              <span className="face-card">
                <ViewContext.Provider value={f.key}>
                  <Card card={sample} />
                </ViewContext.Provider>
              </span>
              <span className="face-label">{f.label}</span>
              <span className="face-note">{f.note}</span>
            </button>
          );
        })}
      </div>
      {/* two settings that would otherwise contradict each other */}
      {!fetching && (
        <p className="set-note set-link">
          Both of these need pictures.{" "}
          <button className="link-btn" onClick={onArt}>
            Turn on card art
          </button>{" "}
          to use them.
        </p>
      )}
    </section>
  );
}

function Art({
  fetching,
  onFetching,
}: {
  fetching: boolean;
  onFetching: (on: boolean) => void;
}) {
  const [have, setHave] = useState(() => cachedCards());
  const [busy, setBusy] = useState(false);
  const timer = useRef<number>(0);

  /* the prototype has no card list to walk, so a download is played out
     rather than run — the shape of the wait is what is being judged */
  useEffect(() => {
    if (!busy) return;
    timer.current = window.setInterval(() => {
      setHave((n) => {
        if (n >= SUPPORTED) {
          setBusy(false);
          return SUPPORTED;
        }
        return Math.min(SUPPORTED, n + 37);
      });
    }, 90);
    return () => window.clearInterval(timer.current);
  }, [busy]);

  const share = Math.min(1, have / SUPPORTED);
  return (
    <section className="set-section">
      <h3 className="set-head">Card art</h3>
      <div className="set-row">
        <span className="set-row-text">
          <span className="set-row-label">Fetch art as I play</span>
          <span className="set-note">
            Your browser asks Scryfall directly, and keeps what comes back on this
            device. Nothing passes through the SAGE server.
          </span>
        </span>
        <button
          role="switch"
          aria-checked={fetching}
          className={`switch${fetching ? " switch-on" : ""}`}
          onClick={() => onFetching(!fetching)}
        >
          <span className="switch-knob" />
        </button>
      </div>

      <div className="set-block">
        <div className="meter-head">
          <span className="set-row-label">Downloaded</span>
          <span className="meter-size">{mb(estimateBytes(have))}</span>
        </div>
        <div className="meter">
          <span className="meter-fill" style={{ width: `${share * 100}%` }} />
        </div>
        <div className="meter-foot">
          {have.toLocaleString()} of {SUPPORTED.toLocaleString()} cards
          {busy && <span className="meter-busy">downloading…</span>}
          {!fetching && !busy && (
            <span className="meter-off">card art is turned off</span>
          )}
        </div>
        <div className="set-acts">
          {busy ? (
            <button className="action-done action-alt" onClick={() => setBusy(false)}>
              Stop
            </button>
          ) : (
            <button
              className="action-done"
              disabled={have >= SUPPORTED || !fetching}
              onClick={() => setBusy(true)}
            >
              {have >= SUPPORTED ? "All art downloaded" : "Download all art"}
            </button>
          )}
          <button
            className="helper-btn helper-concede"
            disabled={have === 0}
            onClick={() => {
              clearArt();
              setBusy(false);
              setHave(0);
            }}
          >
            Clear cache
          </button>
        </div>
      </div>
    </section>
  );
}

export function Settings({
  face,
  onFace,
  onClose,
}: {
  face: CardView;
  onFace: (face: CardView) => void;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"cards" | "art">("cards");
  const [fetching, setFetching] = useState(true);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="zone-view" onClick={onClose}>
      <PipDefs />
      <div className="zone-panel set-panel" onClick={(e) => e.stopPropagation()}>
        <div className="zone-head">
          <span className="zone-title">Settings</span>
          <span className="zone-tally" />
          <button className="zone-close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="set-body">
          <div className="set-rail">
            {(
              [
                ["cards", "Cards"],
                ["art", "Card art"],
              ] as ["cards" | "art", string][]
            ).map(([key, label]) => (
              <button
                key={key}
                className={`rail-btn${tab === key ? " rail-on" : ""}`}
                onClick={() => setTab(key)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="set-content">
            {tab === "cards" ? (
              <Cards
                face={face}
                onFace={onFace}
                fetching={fetching}
                onArt={() => {
                  setFetching(true);
                  setTab("art");
                }}
              />
            ) : (
              <Art
                fetching={fetching}
                onFetching={(on) => {
                  setFetching(on);
                  /* a face that needs pictures cannot survive turning them off */
                  if (!on) onFace("frame");
                }}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
