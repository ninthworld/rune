/**
 * The pregame session header, shared by the Lobby and Room places (issue #506;
 * `front-door-and-lobby.md` §5.2, §6).
 *
 * The compact lockup left — the same brand geometry as the front door's,
 * scaled down, so the identity is continuous across the place change — and the
 * session actions right: the session menu (display settings, card art,
 * Disconnect) and Disconnect itself. Disconnect stops being the only thing in
 * the header (P7: settings used to vanish the moment you connected).
 *
 * Both are **client-session actions**; the header never holds a lobby command.
 */
import { RuneMark } from '../chrome/RuneMark';
import { SessionMenu } from './SessionMenu';
import p from './styles';

export function PregameHeader({ onDisconnect }: { onDisconnect: () => void }) {
  return (
    <header className={p.header}>
      <div className={p.headerBrand}>
        <RuneMark size={28} className={p.mark} />
        <h1 className={p.headerWordmark}>RUNE</h1>
        <span className={p.headerTag}>Lobby</span>
      </div>
      <div className={p.headerActions}>
        <SessionMenu onDisconnect={onDisconnect} />
        <button
          type="button"
          className={p.button}
          onClick={onDisconnect}
          data-testid="lobby-disconnect-button"
        >
          Disconnect
        </button>
      </div>
    </header>
  );
}
