/**
 * Shared `GameView` → DOM-card presentation mapping for the Phase 2 scene plane.
 *
 * This module only formats facts the server supplied and affordances it already
 * offered. It is shared by the live table and the Phase 1 fixture so neither
 * grows a second definition of how a plane card looks.
 */
import type { CardDisplayData } from '../card/cardFactory';
import type { CardFaceArt } from '../card/dom';
import { artUrlFor, getArtSource, getArtStyle } from '../card/art/artStore';
import type { CardView, GameView } from '../protocol';
import type { PlaneRender, PlaneStagingState } from './plane';
import { basicLandGlyph, rowKindForType, toDisplayData } from './scene/card-helpers';

/** Resolve already-loaded player-side art for a DOM card without fetching. */
export function domCardArt(card: CardView): CardFaceArt | undefined {
  const url = artUrlFor(card.functional_id);
  if (url === undefined) return undefined;
  return {
    url,
    full: getArtSource() === 'scryfall' && getArtStyle() === 'full',
  };
}

/** Map one staged permanent to the shared card display contract. */
export function planeDisplayData(
  view: GameView,
  staging: PlaneStagingState | undefined,
  render: PlaneRender,
): CardDisplayData {
  const permanent = view.battlefield.find((entry) => entry.id === render.entityId);
  if (!permanent) {
    return {
      name: render.name,
      typeLine: 'Permanent',
      colorIdentity: 'C',
      targeting: render.candidate,
    };
  }
  const blockerCount = view.battlefield.filter((entry) => entry.blocking === permanent.id).length;
  const row = rowKindForType(permanent.card.type_line);
  return toDisplayData(permanent.card, {
    tapped: permanent.tapped,
    counters: permanent.counters,
    selected: render.memberIds.includes(staging?.selectedId ?? ''),
    actionable: view.valid_actions.some((action) =>
      action.subject?.some((id) => render.memberIds.includes(id)),
    ),
    landGlyph: row === 'lands' ? basicLandGlyph(permanent.card.type_line) : undefined,
    // The land **resource tile** silhouette (card-representation §3.1/§4): the
    // same server-type-line display glue that picks `landGlyph`, so the plane's
    // reserved cell and the drawn face can never disagree about the box.
    landTile: row === 'lands',
    attacking: permanent.attacking,
    attackingPlayer: permanent.attacking_player,
    blocking: permanent.blocking !== undefined,
    blockedBy: blockerCount,
    markedDamage: permanent.damage,
  });
}

/** Map one hand card to the shared card display contract. */
export function handDisplayData(view: GameView, card: CardView): CardDisplayData {
  return toDisplayData(card, {
    selected: false,
    actionable: view.valid_actions.some((action) => action.subject?.includes(card.id)),
  });
}
