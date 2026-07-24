/**
 * The CardFace-consuming face renderer for the plane reconciler (issue #481,
 * consuming #479): renders the single DOM card component into an entity
 * wrapper as static markup — synchronous, deterministic, headless-testable,
 * with no React roots for the reconciler to manage. The face is presentational
 * (`role="img"`); interactivity stays with the overlay keyed off the staged
 * plane's rects, so static markup loses nothing.
 *
 * The caller supplies `dataFor` — the GameView → {@link CardDisplayData}
 * mapping the shipped scene builder already owns — and the renderer overlays
 * the two facts the staging computed: the fold's `stackCount` and candidate
 * piercing (a prompt candidate always renders with the targeting treatment).
 * The signature is the carried `cardVisualSignature`, so "same-looking card"
 * means exactly what it means everywhere else.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { cardVisualSignature, type CardDisplayData } from '../card/cardFactory';
import { CardFace } from '../card/dom';
import type { PlaneRender } from './plane';
import type { PlaneFaceRenderer } from './planeReconciler';

/** Build the reconciler's default face renderer around a display-data source. */
export function cardFaceRenderer(
  dataFor: (render: PlaneRender) => CardDisplayData,
): PlaneFaceRenderer {
  const faceData = (render: PlaneRender): CardDisplayData => {
    const data = dataFor(render);
    return {
      ...data,
      stackCount: render.stackCount,
      targeting: data.targeting === true || render.candidate ? true : data.targeting,
    };
  };
  return {
    signature: (render) => cardVisualSignature(faceData(render), render.tier),
    render: (el, render) => {
      el.innerHTML = renderToStaticMarkup(<CardFace data={faceData(render)} tier={render.tier} />);
    },
  };
}
