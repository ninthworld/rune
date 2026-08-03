import { useLayoutEffect, useRef, useState } from "react";

/* A strip of cards too wide for the space it was given.

   The native scrollbar is hidden: it would eat into the height the cards
   are measured from, and a bar across the board reads as a defect. The
   scrollable edges are masked away to transparent instead, which says
   "there is more this way" without one.

   Hiding the bar means the strip has to earn its input back, because
   `overflow-x` alone only answers a horizontal wheel and a touch drag —
   neither of which a mouse produces. So a wheel of either axis moves it,
   and it can be dragged like a map.

   Returns the ref to put on the scrolling element and the edge classes
   that drive the mask. */
export function useScrollStrip<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [edges, setEdges] = useState("");

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => {
      const left = el.scrollLeft > 1;
      const right = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
      setEdges(`${left ? " scroll-l" : ""}${right ? " scroll-r" : ""}`);
    };
    update();
    el.addEventListener("scroll", update, { passive: true });
    const obs = new ResizeObserver(update);
    obs.observe(el);
    for (const child of el.children) obs.observe(child);
    return () => {
      el.removeEventListener("scroll", update);
      obs.disconnect();
    };
  });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;

    const room = () => el.scrollWidth - el.clientWidth;

    /* a mouse only ever sends deltaY, so take whichever axis moved most
       and spend it sideways; hand the event back at either end so the
       strip never traps a gesture meant for something else */
    const onWheel = (e: WheelEvent) => {
      const max = room();
      if (max <= 0) return;
      const raw =
        Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      const step =
        e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? el.clientWidth : 1;
      const next = Math.max(0, Math.min(max, el.scrollLeft + raw * step));
      if (next === el.scrollLeft) return;
      e.preventDefault();
      el.scrollLeft = next;
    };

    let from = 0;
    let at = 0;
    let panning = false;
    const onMove = (e: PointerEvent) => {
      /* a few pixels of slop, so a click on a card is still a click */
      if (!panning && Math.abs(e.clientX - from) < 4) return;
      panning = true;
      el.scrollLeft = at - (e.clientX - from);
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      el.classList.remove("panning");
      /* a pan that ends over a card must not also read as a click on it */
      if (panning) {
        el.addEventListener("click", (e) => e.stopPropagation(), {
          capture: true,
          once: true,
        });
      }
    };
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0 || room() <= 0) return;
      from = e.clientX;
      at = el.scrollLeft;
      panning = false;
      el.classList.add("panning");
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    };

    el.addEventListener("wheel", onWheel, { passive: false });
    el.addEventListener("pointerdown", onDown);
    return () => {
      el.removeEventListener("wheel", onWheel);
      el.removeEventListener("pointerdown", onDown);
      onUp();
    };
    /* bound once: scrolling re-renders the strip, and re-running the
       effect would tear down a drag that is still in the user's hand */
  }, []);

  return { ref, edges };
}
