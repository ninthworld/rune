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
}

/** How many attackers are currently attacking `playerId`, straight from the view
 * (`Permanent.attacking_player`, issue #347). The client counts; it derives no combat. */
function attackersOn(view: GameView, playerId: PlayerId): number {
  return view.battlefield.filter((perm) => perm.attacking_player === playerId).length;
}

/** The identity-accent custom property the panel chrome classes read. */
function accentStyle(accent: string): CSSProperties {
  return { '--identity-accent': accent } as CSSProperties;
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

export function PanelChrome({ view, scene, onOpenZone, targeting, highlightedId }: Props) {
  const multiplayer = view.opponents.length > 1;
  return (
    <div data-testid="panel-chrome" style={panelLayer(scene.width, scene.height)}>
      {scene.bands.map((band) => (
        <div key={band.playerId} data-testid={`player-panel-${band.playerId}`}>
          {/* The panel box: line-work border + corner notches in the controller's
              accent (§Identity). Chrome only, never interactive. */}
          <div
            className={cx(s.panelBox, band.isLocal && s.panelBoxLocal)}
            style={{ ...panelBox(band.rect), ...accentStyle(band.accent) }}
            aria-hidden="true"
          />
          {panelHeader(view, band, targeting, highlightedId === band.playerId, multiplayer)}
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
                testId={onOpenZone ? `table-exile-${band.playerId}` : `exile-pile-${band.playerId}`}
              />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
