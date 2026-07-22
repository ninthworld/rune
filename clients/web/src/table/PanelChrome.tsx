/**
 * The player-panel chrome layer (ADR 0023; React DOM per ADR 0003).
 *
 * The fixed shell carves one bounded panel per player; the Pixi canvas draws the
 * cards inside each panel's content area, and this layer draws everything else a
 * panel owns, positioned from the scene's band rects:
 *
 *  - the panel box itself: line-work border with corner notches, tinted by the
 *    controller's identity accent (§Identity — the region answers "whose stuff";
 *    cards never wear the accent);
 *  - the header strip: life crest, nameplate, and secondary meta (hand count,
 *    active-turn marker, "attacked ×N", statuses) — every value verbatim from the
 *    view;
 *  - the zone piles column (library / graveyard / exile) parked at the panel's
 *    edge — except the local panel on the full composition, whose piles live in
 *    the bottom shell's identity panel;
 *  - the empty-panel hint, so an empty lane invites play instead of vanishing.
 *
 * Targeting mode (ADR 0009 §Client): when the active slot's candidates are
 * *players*, a candidate's header becomes the pick surface (ringed, ≥44px);
 * non-candidates dim. Only server-listed candidates are ever pickable.
 *
 * Everything derives from the current scene + view (pure projections); no layout
 * state persists across messages. The layer is chrome — `pointerEvents: none` —
 * except its buttons, so it never steals a click meant for a card.
 */
import type { CSSProperties, ReactNode } from 'react';
import type { EntityId, GameView, PlayerId } from '../protocol';
import type { Band, TableScene } from './scene';
import { cx } from '../chrome/cx';
import {
  panelBox,
  panelHeaderBox,
  panelLayer,
  emptyPanelHint,
  pileColumnBox,
  rowLabel,
  tileCollapseBox,
} from './styles';
import { playerName } from '../playerNames';
import { PileTopCard, ZonePile } from './ZonePile';
import s from './chrome.module.css';

/**
 * A browsable public zone a board pile can open (issue #262). The zone piles are the
 * single home for library/graveyard/exile counts.
 */
export type BrowsableZone = 'graveyard' | 'exile';

/** The active target slot's *player* candidates plus the pick handler. */
export interface PlayerTargeting {
  /** Entity ids that are legal targets for the active slot (players included). */
  candidates: EntityId[];
  /** Pick a player as the current slot's answer. */
  onPick: (id: EntityId) => void;
}

/**
 * Phone-portrait summary-tile focus controls (issue #400). Present only in the
 * tile composition; drives tap/keyboard expand of a collapsed opponent tile and
 * collapse of the expanded one. Ephemeral presentation only — never legality.
 */
export interface TileFocus {
  /** The opponent currently expanded in place, if any (the rest are tiles). */
  expandedId: PlayerId | null;
  /**
   * Whether the expansion is **pinned open** by an offered decision (an action or
   * target on that opponent's board): its battlefield must stay reachable, so it
   * shows no manual collapse control. A manually-focused expansion is not pinned.
   */
  pinned: boolean;
  /** Toggle a tile: expand a collapsed opponent, or collapse the expanded one. */
  onToggle: (playerId: PlayerId) => void;
}

interface Props {
  view: GameView;
  /** The scene whose band rects anchor the panels, headers, and piles. */
  scene: TableScene;
  /**
   * Open a player's graveyard/exile browser (issue #262). When provided, the
   * graveyard and exile piles are buttons; absent ⇒ static piles (read-only board).
   */
  onOpenZone?: (playerId: PlayerId, zone: BrowsableZone) => void;
  /** Present only while a player-target slot is active. */
  targeting?: PlayerTargeting;
  /** The player a game-log reference is highlighting, if any (issue #260). */
  highlightedId?: PlayerId | null;
  /** Summary-tile focus controls (issue #400); present only in the tile composition. */
  focus?: TileFocus;
  /** Snap the tile expand/collapse transition for `prefers-reduced-motion`. */
  reducedMotion?: boolean;
}

/** How many attackers are currently attacking `playerId`, straight from the view
 * (`Permanent.attacking_player`, issue #347). The client counts; it derives no combat. */
function attackersOn(view: GameView, playerId: PlayerId): number {
  return view.battlefield.filter((perm) => perm.attacking_player === playerId).length;
}

/** 21 combat damage from one commander is lethal (CR 903.10a) — the tally's denominator. */
export const COMMANDER_DAMAGE_LETHAL = 21;

/**
 * A compact `amount/21` tally of the commander damage `playerId` has taken this game
 * (issue #372), one entry per attacking commander, straight from the public
 * `commander_damage` list. Renders nothing when the player has taken none, so a
 * non-commander game (or an untouched player) shows no chrome.
 */
export function CommanderDamageTally({ view, playerId }: { view: GameView; playerId: PlayerId }) {
  const taken = (view.commander_damage ?? []).filter((d) => d.damaged === playerId);
  if (taken.length === 0) return null;
  return (
    <span className={s.cmdDamage} data-testid={`cmd-damage-${playerId}`} title="Commander damage">
      {taken.map((d) => (
        <span key={d.commander}>
          {d.amount}/{COMMANDER_DAMAGE_LETHAL}
        </span>
      ))}
    </span>
  );
}

/** The identity-accent custom property the panel chrome classes read. */
function accentStyle(accent: string): CSSProperties {
  return { '--identity-accent': accent } as CSSProperties;
}

/**
 * A player's command-zone pile plus its recast tax (issue #372), rendered only when
 * the player has a command-zone entry OR a tracked commander tax — so the pile shows
 * even with the commander currently out of the zone (count 0, tax still owed), and
 * never appears in a non-commander game. The pile reuses the shared static treatment;
 * the tax (`{N}`) rides beside it when > 0. All public, straight from the view.
 */
export function CommandPile({
  view,
  playerId,
  playerLabel,
  count,
}: {
  view: GameView;
  playerId: PlayerId;
  playerLabel: string;
  count: number;
}) {
  const hasEntry = (view.command ?? []).some((pile) => pile.player_id === playerId);
  const taxEntry = (view.commander_tax ?? []).find((t) => t.commander === playerId);
  if (!hasEntry && taxEntry === undefined) return null;
  const tax = taxEntry?.tax ?? 0;
  return (
    <>
      <ZonePile
        zone="command"
        playerLabel={playerLabel}
        count={count}
        testId={`command-pile-${playerId}`}
      />
      {tax > 0 && (
        <span className={s.cmdTax} data-testid={`cmd-tax-${playerId}`}>
          Tax {'{'}
          {tax}
          {'}'}
        </span>
      )}
    </>
  );
}

/**
 * Whether `playerId` has any command-zone presence in the view — a command-zone
 * entry or an owed commander tax (the same condition {@link CommandPile} renders
 * on). A summary tile shows its command indicator exactly when this is true, so
 * the command zone stays visible in the compact composition (issue #372/#400).
 */
function hasCommandZone(view: GameView, playerId: PlayerId): boolean {
  const hasEntry = (view.command ?? []).some((pile) => pile.player_id === playerId);
  const owedTax = (view.commander_tax ?? []).some((t) => t.commander === playerId);
  return hasEntry || owedTax;
}

/**
 * A collapsed opponent's summary-tile content (issue #400): the identity crest
 * (life), nameplate, and the counts a demoted battlefield still owes the table —
 * hand size, library size, the command-zone count where a commander game is in
 * play, and any commander damage taken — plus the active-turn / attacked markers.
 * Every value is verbatim from the view; the tile computes nothing.
 */
function summaryTileContent(view: GameView, band: Band): ReactNode {
  const opponent = view.opponents.find((o) => o.player_id === band.playerId);
  const isActive = view.active_player !== '' && view.active_player === band.playerId;
  const attacked = attackersOn(view, band.playerId);
  return (
    <>
      <span className={s.lifeCrest} title="Life">
        <span className={s.lifeCrestValue} data-testid={`hud-life-${band.playerId}`}>
          {opponent?.life ?? '—'}
        </span>
      </span>
      <div className={s.summaryTileBody}>
        <div className={s.summaryTileTop}>
          <span className={s.panelName} data-testid={`hud-name-${band.playerId}`}>
            {band.label}
          </span>
          {isActive && (
            <span className={s.panelTurnMarker} data-testid={`panel-active-${band.playerId}`}>
              ● turn
            </span>
          )}
          {opponent?.eliminated && <span className={s.panelEliminated}>eliminated</span>}
        </div>
        <div className={s.summaryTileCounts}>
          <span className={s.hudMeta} data-testid={`hud-hand-${band.playerId}`}>
            <span className={s.miniCardBack} aria-hidden="true" />
            {opponent?.hand_size ?? 0}
          </span>
          <span
            className={s.summaryCount}
            data-testid={`tile-library-${band.playerId}`}
            title="Library"
          >
            Lib {band.zones.library}
          </span>
          {hasCommandZone(view, band.playerId) && (
            <span
              className={s.summaryCount}
              data-testid={`tile-command-${band.playerId}`}
              title="Command zone"
            >
              Cmd {band.zones.command ?? 0}
            </span>
          )}
          <CommanderDamageTally view={view} playerId={band.playerId} />
          {attacked > 0 && (
            <span className={s.hudAttacked} data-testid={`hud-attacked-${band.playerId}`}>
              ▸ ×{attacked}
            </span>
          )}
        </div>
      </div>
    </>
  );
}

/** The spoken name of a summary tile — the player and the same counts it shows,
 * so the tile is an interactive element with an accessible name (issue #400). */
function tileAriaLabel(view: GameView, band: Band): string {
  const opponent = view.opponents.find((o) => o.player_id === band.playerId);
  const parts = [
    playerName(view, band.playerId),
    `${opponent?.life ?? 0} life`,
    `hand ${opponent?.hand_size ?? 0}`,
    `library ${band.zones.library}`,
  ];
  if (hasCommandZone(view, band.playerId)) parts.push(`command ${band.zones.command ?? 0}`);
  if (opponent?.eliminated) parts.push('eliminated');
  return parts.join(', ');
}

/**
 * One collapsed opponent as a **tap-to-focus summary tile** (issue #400). During a
 * player-target slot the tile keeps the shared pick contract (it becomes the pick
 * button); otherwise it is an `aria-expanded=false` toggle that expands this
 * opponent's battlefield in place — pointer- and keyboard-operable, with a name. A
 * dimmed, inert tile shows when a different player is the pick target.
 */
function SummaryTile({
  view,
  band,
  targeting,
  highlighted,
  focus,
  reducedMotion,
}: {
  view: GameView;
  band: Band;
  targeting: PlayerTargeting | undefined;
  highlighted: boolean;
  focus: TileFocus | undefined;
  reducedMotion: boolean;
}): ReactNode {
  const content = summaryTileContent(view, band);
  const highlightClass = highlighted ? s.tileHighlighted : undefined;
  const style = { ...panelBox(band.rect), ...accentStyle(band.accent), pointerEvents: 'auto' } as
    CSSProperties | undefined;
  const animate = reducedMotion ? 'false' : 'true';
  if (targeting?.candidates.includes(band.playerId)) {
    return (
      <button
        type="button"
        data-testid={`target-player-${band.playerId}`}
        aria-label={`Target player ${playerName(view, band.playerId)}`}
        onClick={() => targeting.onPick(band.playerId)}
        className={cx(s.tileButtonReset, s.summaryTile, s.targetTile, highlightClass)}
        style={style}
        data-animate={animate}
      >
        {content}
      </button>
    );
  }
  if (focus) {
    return (
      <button
        type="button"
        data-testid={`tile-focus-${band.playerId}`}
        aria-expanded={false}
        aria-label={`${tileAriaLabel(view, band)}. Expand battlefield`}
        onClick={() => focus.onToggle(band.playerId)}
        className={cx(s.tileButtonReset, s.summaryTile, targeting && s.dimmedTile, highlightClass)}
        style={style}
        data-animate={animate}
      >
        {content}
      </button>
    );
  }
  // No focus controls (defensive: the tile composition always supplies them) — a
  // static, inert tile so the identity + counts still render.
  return (
    <div
      data-testid={`tile-${band.playerId}`}
      className={cx(s.summaryTile, targeting && s.dimmedTile, highlightClass)}
      style={style}
      data-animate={animate}
    >
      {content}
    </div>
  );
}

/** A panel header's content: crest (opponents), nameplate, and secondary meta. */
function headerContent(view: GameView, band: Band): ReactNode {
  const opponent = view.opponents.find((o) => o.player_id === band.playerId);
  const attacked = attackersOn(view, band.playerId);
  const isActive = view.active_player !== '' && view.active_player === band.playerId;
  const hasPriority = view.priority_player !== undefined && view.priority_player === band.playerId;
  return (
    <>
      {!band.isLocal && (
        <span className={s.lifeCrest} title="Life">
          <span className={s.lifeCrestValue} data-testid={`hud-life-${band.playerId}`}>
            {opponent?.life ?? '—'}
          </span>
        </span>
      )}
      {/* Commander damage taken (issue #372), near the life crest — a second lethal
          clock alongside life. The receiver's own tally lives in the bottom shell's
          MePanel, so the local band skips it here (as with the life crest). */}
      {!band.isLocal && <CommanderDamageTally view={view} playerId={band.playerId} />}
      <span className={s.panelName} data-testid={`hud-name-${band.playerId}`}>
        {band.label}
      </span>
      <span className={s.panelMeta}>
        {isActive && (
          <span className={s.panelTurnMarker} data-testid={`panel-active-${band.playerId}`}>
            ● turn
          </span>
        )}
        {hasPriority && !isActive && <span className={s.panelPriority}>priority</span>}
        {attacked > 0 && (
          <span className={s.hudAttacked} data-testid={`hud-attacked-${band.playerId}`}>
            ▸ attacked ×{attacked}
          </span>
        )}
        {opponent?.eliminated && <span className={s.panelEliminated}>eliminated</span>}
        {opponent && (
          <span className={s.hudMeta} data-testid={`hud-hand-${band.playerId}`}>
            <span className={s.miniCardBack} aria-hidden="true" />
            {opponent.hand_size}
          </span>
        )}
        {opponent?.statuses && opponent.statuses.length > 0 && (
          <span className={s.hudStatuses} data-testid={`hud-statuses-${band.playerId}`}>
            {opponent.statuses.join(', ')}
          </span>
        )}
      </span>
    </>
  );
}

/**
 * A panel's header, applying the shared player-targeting contract: outside
 * targeting it is display (focusable on a multiplayer table so keyboard reaches
 * every player area, issue #348); a candidate becomes a pickable button;
 * a non-candidate dims and stays inert.
 */
function panelHeader(
  view: GameView,
  band: Band,
  targeting: PlayerTargeting | undefined,
  highlighted: boolean,
  multiplayer: boolean,
): ReactNode {
  const content = headerContent(view, band);
  const style = { ...panelHeaderBox(band.headerRect), ...accentStyle(band.accent) };
  const highlightClass = highlighted ? s.tileHighlighted : undefined;
  if (targeting?.candidates.includes(band.playerId)) {
    return (
      <button
        type="button"
        data-testid={`target-player-${band.playerId}`}
        aria-label={`Target player ${playerName(view, band.playerId)}`}
        onClick={() => targeting.onPick(band.playerId)}
        className={cx(s.tileButtonReset, s.panelHeader, s.targetTile, highlightClass)}
        style={style}
      >
        {content}
      </button>
    );
  }
  const opponent = view.opponents.find((o) => o.player_id === band.playerId);
  const focusable = multiplayer && !band.isLocal;
  const focusLabel = opponent
    ? [playerName(view, band.playerId), `${opponent.life} life`, `hand ${opponent.hand_size}`]
        .concat(opponent.eliminated ? ['eliminated'] : [])
        .join(', ')
    : undefined;
  return (
    <div
      data-testid={`tile-${band.playerId}`}
      className={cx(s.panelHeader, targeting && s.dimmedTile, highlightClass)}
      data-highlighted={highlighted || undefined}
      style={style}
      {...(focusable && focusLabel !== undefined
        ? { tabIndex: 0, 'data-focus-item': '', role: 'group', 'aria-label': focusLabel }
        : {})}
    >
      {content}
    </div>
  );
}

export function PanelChrome({
  view,
  scene,
  onOpenZone,
  targeting,
  highlightedId,
  focus,
  reducedMotion,
}: Props) {
  const multiplayer = view.opponents.length > 1;
  return (
    <div
      data-testid="panel-chrome"
      data-animate={reducedMotion ? 'false' : 'true'}
      style={panelLayer(scene.width, scene.height)}
    >
      {scene.bands.map((band) => {
        // A collapsed opponent renders as a tap-to-focus summary tile (issue #400)
        // — the whole panel is the tile, so it skips the board box / rows / piles.
        if (band.summary) {
          return (
            <div key={band.playerId} data-testid={`player-panel-${band.playerId}`}>
              <SummaryTile
                view={view}
                band={band}
                targeting={targeting}
                highlighted={highlightedId === band.playerId}
                focus={focus}
                reducedMotion={reducedMotion ?? false}
              />
            </div>
          );
        }
        // The one expanded opponent (tile composition) carries a collapse control on
        // its header, unless the expansion is pinned open by an offered decision.
        const collapsible =
          focus !== undefined &&
          !band.isLocal &&
          band.playerId === focus.expandedId &&
          !focus.pinned &&
          !(targeting?.candidates.includes(band.playerId) ?? false);
        return (
          <div key={band.playerId} data-testid={`player-panel-${band.playerId}`}>
            {/* The panel box: line-work border + corner notches in the controller's
                accent (§Identity). Chrome only, never interactive. */}
            <div
              className={cx(s.panelBox, band.isLocal && s.panelBoxLocal)}
              style={{ ...panelBox(band.rect), ...accentStyle(band.accent) }}
              aria-hidden="true"
            />
            {panelHeader(view, band, targeting, highlightedId === band.playerId, multiplayer)}
            {collapsible && focus !== undefined && (
              <button
                type="button"
                data-testid={`tile-collapse-${band.playerId}`}
                aria-expanded={true}
                aria-label={`Collapse ${playerName(view, band.playerId)} battlefield`}
                className={s.tileCollapse}
                style={tileCollapseBox(band.headerRect)}
                onClick={() => focus.onToggle(band.playerId)}
              >
                ▾
              </button>
            )}
            {/* Only the lands row earns a label — rows are a sorting convention, not
              zones (issue #318). */}
            {band.rows.map(
              (row) =>
                row.label && (
                  <span
                    key={`${band.playerId}-${row.kind}`}
                    data-testid={`row-label-${band.playerId}-${row.kind}`}
                    style={rowLabel(row.rect)}
                  >
                    {row.label}
                  </span>
                ),
            )}
            {band.isEmpty && (
              <div data-testid={`empty-band-${band.playerId}`} style={emptyPanelHint(band.rect)}>
                {band.isLocal
                  ? 'Your battlefield — play a card to put it here'
                  : `${band.label} — battlefield`}
              </div>
            )}
            {/* Zone piles: findable card-shaped objects parked at the panel's edge —
              table furniture, not header chrome. The local panel on the full
              composition parks none (its piles live in the bottom shell). */}
            {band.pileRect.w > 0 && (
              <div
                className={cx(s.zonePiles, s.zonePilesCondensed)}
                style={pileColumnBox(band.pileRect)}
                data-testid={`pile-column-${band.playerId}`}
              >
                <ZonePile
                  zone="library"
                  playerLabel={band.label}
                  count={band.zones.library}
                  testId={`library-pile-${band.playerId}`}
                />
                <ZonePile
                  zone="graveyard"
                  playerLabel={band.label}
                  count={band.zones.graveyard}
                  onOpen={onOpenZone ? () => onOpenZone(band.playerId, 'graveyard') : undefined}
                  faceUp={
                    band.zones.graveyardTop && (
                      <PileTopCard
                        name={band.zones.graveyardTop.name}
                        colorIdentity={band.zones.graveyardTop.colorIdentity}
                      />
                    )
                  }
                  testId={
                    onOpenZone
                      ? `table-graveyard-${band.playerId}`
                      : `graveyard-pile-${band.playerId}`
                  }
                />
                <ZonePile
                  zone="exile"
                  playerLabel={band.label}
                  count={band.zones.exile}
                  onOpen={onOpenZone ? () => onOpenZone(band.playerId, 'exile') : undefined}
                  testId={
                    onOpenZone ? `table-exile-${band.playerId}` : `exile-pile-${band.playerId}`
                  }
                />
                <CommandPile
                  view={view}
                  playerId={band.playerId}
                  playerLabel={band.label}
                  count={band.zones.command ?? 0}
                />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
