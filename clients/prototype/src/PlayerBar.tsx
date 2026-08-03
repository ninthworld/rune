import { Pip } from "./Card";

export type PlayerData = {
  name: string;
  life: number;
  handCount?: number;
  deckCount: number;
  graveCount: number;
  exileCount: number;
  commanderCount?: number;
  /* mana floating in the pool from abilities activated this step */
  mana?: string[];
};

/* Zone glyphs, drawn rather than fetched — the project ships no icon font
   and no third-party art. Each is a 16x16 stroke drawing chosen to stay
   readable at the ~14px a seat gives it, which rules out anything with
   interior detail. */
function Glyph({ name }: { name: string }) {
  return (
    <svg
      className="glyph"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {/* a hand is cards held at an angle; a third card turns to mush at
          this size, so two carry it */}
      {name === "hand" && (
        <>
          <rect x="6" y="3.4" width="6.4" height="9.6" rx="1.1" transform="rotate(17 9.2 8.2)" />
          <rect x="3.2" y="3.4" width="6.4" height="9.6" rx="1.1" transform="rotate(-14 6.4 8.2)" />
        </>
      )}
      {/* the library is the same card squared up and stacked */}
      {name === "deck" && (
        <>
          <path d="M5 4h5.6a1.4 1.4 0 0 1 1.4 1.4V11" />
          <rect x="2.6" y="5.6" width="8.4" height="8.4" rx="1.3" />
        </>
      )}
      {/* a headstone: the one zone whose meaning survives being 14px tall */}
      {name === "grave" && (
        <>
          <path d="M4.5 12.3V7.6a3.5 3.5 0 0 1 7 0v4.7" />
          <rect x="2.4" y="12.3" width="11.2" height="1.7" rx="0.6" />
        </>
      )}
      {/* exile is a card that is no longer really there */}
      {name === "exile" && (
        <rect x="4" y="2.5" width="8" height="11" rx="1.2" strokeDasharray="2.6 1.9" />
      )}
      {name === "commander" && (
        <>
          <path
            d="M3 12V5.2l3 2.4 2-4.2 2 4.2 3-2.4V12z"
            fill="currentColor"
            strokeWidth="1.2"
          />
          <path d="M3.4 14h9.2" />
        </>
      )}
      {name === "focus" && (
        <path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10" />
      )}
    </svg>
  );
}

/* The sidebar is the player: everything you can point at that isn't a
   card on the board. Each zone is an icon with its count — the labels
   were the reason the bar needed more height than a seat has — and every
   one of them is a bounded, hoverable box rather than a run of text, so
   there is already an area here for a target or a drop to land on. */
export function PlayerBar({
  player,
  focused,
  onFocus,
  onZone,
  anchor,
}: {
  player: PlayerData;
  focused?: boolean;
  onFocus?: () => void;
  onZone?: (zone: string, count: number) => void;
  anchor?: string;
}) {
  /* the optional zones are optional per *game*, not per seat: either every
     bar has a command zone or none does, so a glyph is always in the same
     place whichever seat you are reading */
  const zones = [
    ...(player.handCount !== undefined
      ? [{ key: "hand", label: "Hand", count: player.handCount }]
      : []),
    { key: "deck", label: "Library", count: player.deckCount },
    { key: "grave", label: "Graveyard", count: player.graveCount },
    { key: "exile", label: "Exile", count: player.exileCount },
    ...(player.commanderCount !== undefined
      ? [{ key: "commander", label: "Command zone", count: player.commanderCount }]
      : []),
  ];
  return (
    <div className="player-bar">
      <div className="player-head">
        <button
          className="player-target"
          title={`Target ${player.name}`}
          data-anchor={anchor}
        >
          <span className="player-name">{player.name}</span>
          <span className="player-life">{player.life}</span>
        </button>
        {onFocus && (
          <button
            className={`focus-btn${focused ? " focus-on" : ""}`}
            onClick={onFocus}
            title={focused ? "Show every seat" : "Show only this seat"}
          >
            <Glyph name="focus" />
          </button>
        )}
      </div>
      <div className="zone-grid">
        {zones.map((zone) => (
          <button
            key={zone.key}
            className="zone-btn"
            data-zone={zone.key}
            title={`${zone.label} — ${zone.count}`}
            onClick={onZone && (() => onZone(zone.key, zone.count))}
          >
            <Glyph name={zone.key} />
            <span className="zone-count">{zone.count}</span>
          </button>
        ))}
      </div>
      {player.mana && player.mana.length > 0 && (
        <div className="mana-pool" title="Mana pool">
          {player.mana.map((symbol, i) => (
            <Pip key={i} symbol={symbol} />
          ))}
        </div>
      )}
      <div className="player-status" />
    </div>
  );
}
