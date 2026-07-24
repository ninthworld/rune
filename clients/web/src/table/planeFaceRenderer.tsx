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
 *
 * Re-renders **morph the existing face tree in place** instead of replacing
 * it: attributes sync on the same element instances and only structurally
 * changed subtrees are swapped. The transform-bearing `.inner` node therefore
 * persists across a state change, so CardFace's own CSS transitions have a
 * previous style to interpolate from — a tap re-render animates the ~25°
 * rotation (tap/untap motion class) instead of mounting a new face already at
 * its final angle, and `prefers-reduced-motion` snaps it via the face's own
 * media query with a byte-identical end state.
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
      const markup = renderToStaticMarkup(<CardFace data={faceData(render)} tier={render.tier} />);
      const current = el.firstElementChild;
      if (!current) {
        el.innerHTML = markup;
        return;
      }
      const template = el.ownerDocument.createElement('template');
      template.innerHTML = markup;
      const next = template.content.firstElementChild;
      if (!next) {
        el.innerHTML = markup;
        return;
      }
      morph(current, next);
    },
  };
}

/**
 * Sync `target` to look exactly like `source`, preserving element identity
 * wherever the structure matches (a tiny, deterministic morphdom): attributes
 * are rewritten in the source's order — so a morphed tree serializes
 * byte-identically to a fresh render — children pair by index, and only a
 * node whose type changed is replaced. `source` is a throwaway tree; moving
 * its nodes into `target` is fine.
 */
function morph(target: Element, source: Element): void {
  const targetAttrs = Array.from(target.attributes);
  const sourceAttrs = Array.from(source.attributes);
  const sameShape =
    targetAttrs.length === sourceAttrs.length &&
    targetAttrs.every((attr, i) => sourceAttrs[i]!.name === attr.name);
  if (sameShape) {
    for (const attr of sourceAttrs) {
      if (target.getAttribute(attr.name) !== attr.value) target.setAttribute(attr.name, attr.value);
    }
  } else {
    for (const attr of targetAttrs) target.removeAttribute(attr.name);
    for (const attr of sourceAttrs) target.setAttribute(attr.name, attr.value);
  }

  const targetChildren = Array.from(target.childNodes);
  const sourceChildren = Array.from(source.childNodes);
  const count = Math.max(targetChildren.length, sourceChildren.length);
  for (let i = 0; i < count; i += 1) {
    const t = targetChildren[i];
    const s = sourceChildren[i];
    if (t === undefined) {
      target.appendChild(s!);
    } else if (s === undefined) {
      t.remove();
    } else if (t.nodeType !== s.nodeType || t.nodeName !== s.nodeName) {
      target.replaceChild(s, t);
    } else if (t.nodeType === Node.TEXT_NODE) {
      if (t.nodeValue !== s.nodeValue) t.nodeValue = s.nodeValue;
    } else {
      morph(t as Element, s as Element);
    }
  }
}
