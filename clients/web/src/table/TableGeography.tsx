/**
 * The table's geography layer (issue #278, React DOM per ADR 0003).
 *
 * A fresh game is otherwise an unlabeled void between the tiles and the hand: you
 * cannot tell where a played card lands or which region is whose, and the hidden
 * zones (library/graveyard/exile) exist only as tile text. This layer draws the
 * missing skin over the band/hand rects the scene computes:
 *
 *  - each player's battlefield lane is bounded and labeled by its **controller**
 *    (zone placement follows control, ui-requirements §2), including when empty —
 *    an empty local lane invites play rather than rendering as nothing;
 *  - each lane carries its zone piles: a card-back library pile with a live count
 *    (hidden info — no card identity) and graveyard/exile piles that open the
 *    existing browsers (issue #262), reusing the counts already in the view;
 *  - the hand row is separated and labeled so "my hand" reads apart from "my
 *    battlefield".
 *
 * Everything derives from the current scene (itself a pure projection of one
 * `GameView`); no layout state persists across messages. The layer is chrome —
 * `pointerEvents: none` — except the graveyard/exile pile buttons, so it never
 * steals a click meant for a card in the overlay stacked above it.
 */
import { Fragment } from 'react';
import type { PlayerId } from '../protocol';
import type { TableScene } from './scene';
import { bandRegion, emptyBandHint, geographyLayer, regionHeader, rowLabel } from './styles';
import { ZonePile } from './ZonePile';
import s from './chrome.module.css';

/**
 * A browsable public zone a board pile can open (issue #262). The zone piles are the
 * single home for library/graveyard/exile — the player HUDs (issue #296) no longer
 * repeat these counts, so this type lives here with the piles that own them.
 */
export type BrowsableZone = 'graveyard' | 'exile';

interface Props {
  /** The scene whose band/hand rects anchor the labels, boundaries, and piles. */
  scene: TableScene;
  /**
   * Open a player's graveyard/exile browser (issue #262). When provided, the
   * graveyard and exile piles are buttons; absent ⇒ they are omitted (the board is
   * read-only, e.g. a game-over view still shows labeled lanes and the library).
   */
  onOpenZone?: (playerId: PlayerId, zone: BrowsableZone) => void;
}

export function TableGeography({ scene, onOpenZone }: Props) {
  return (
    <div data-testid="table-geography" style={geographyLayer(scene.width, scene.height)}>
      {scene.bands.map((band) => (
        <Fragment key={band.playerId}>
          <div style={bandRegion(band.rect, band.isLocal)} aria-hidden="true" />
          {/* Row labels: only the lands row is labeled — the type-grouped rows are a
              sorting convention, not zones (issue #318). */}
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
            <div data-testid={`empty-band-${band.playerId}`} style={emptyBandHint(band.rect)}>
              {band.isLocal
                ? 'Your battlefield — play a card to put it here'
                : `${band.label} — battlefield`}
            </div>
          )}
          <div style={regionHeader(band.rect)}>
            <span data-testid={`band-label-${band.playerId}`} className={s.regionLabel}>
              {band.label}
            </span>
            {/*
             * Zone piles (issue #319): findable card-shaped objects, not text chips,
             * parked in this lane's corner. The library is count-only (hidden info);
             * graveyard/exile open their browsers when the board is interactive. Each
             * count lives here and nowhere else (the HUD no longer repeats them).
             */}
            <div className={s.zonePiles}>
              <ZonePile
                zone="library"
                playerLabel={band.label}
                count={band.zones.library}
                testId={`library-pile-${band.playerId}`}
              />
              {onOpenZone && (
                <>
                  <ZonePile
                    zone="graveyard"
                    playerLabel={band.label}
                    count={band.zones.graveyard}
                    onOpen={() => onOpenZone(band.playerId, 'graveyard')}
                    testId={`table-graveyard-${band.playerId}`}
                  />
                  <ZonePile
                    zone="exile"
                    playerLabel={band.label}
                    count={band.zones.exile}
                    onOpen={() => onOpenZone(band.playerId, 'exile')}
                    testId={`table-exile-${band.playerId}`}
                  />
                </>
              )}
            </div>
          </div>
        </Fragment>
      ))}

      {/* The hand row: a labeled, bounded region separating it from the battlefield.
          Its cards render on the canvas; only the label/boundary live here. */}
      <div style={bandRegion(scene.handRegion.rect, true)} aria-hidden="true" />
      <div style={regionHeader(scene.handRegion.rect)}>
        <span data-testid="hand-label" className={s.regionLabel}>
          {scene.handRegion.label}
        </span>
      </div>
    </div>
  );
}
