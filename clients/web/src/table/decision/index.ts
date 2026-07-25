/**
 * The decision plaque's public surface (issue #534, `control-language.md` §10).
 *
 * Consumers import `table/decision` rather than reaching into the directory, the
 * same way `table/controls` is consumed. Two things come out of here and the
 * split is the point:
 *
 * - {@link placeDecisionPlaque} — **where** the decision goes. Pure, deterministic
 *   layout arithmetic over rects the caller already has.
 * - {@link DecisionPlaque} — **what** it draws. A pure render of the session the
 *   caller assembled; it submits nothing and derives nothing.
 *
 * ## The integration contract
 *
 * `LiveMatchTable` is the one production caller. It renders the plaque as a
 * **sibling of the shell's regions** (never inside `.top` / `.scene` / `.rail` /
 * `.bottom`: a region carrying a z-index creates a stacking context, and a plaque
 * trapped inside one cannot reach the `decision` rung it needs), and wires:
 *
 * ```tsx
 * const size = estimatePlaqueSize(controlCount);          // controls actually rendered
 * const placement = placeDecisionPlaque({
 *   viewport: bands.viewport,      // shellBands(viewport).viewport
 *   board: bands.scene,            // the top-half/bottom-half test's frame
 *   cluster: clusterSlot,          // the lower-right control cluster's rect
 *   seatCount: view.seat_order.length,
 *   subject: rectOf(action.subject?.[0]),      // viewport coords, or undefined
 *   candidates: activeCandidateIds.map(rectOf).filter(Boolean),
 *   wings: wingSlotRects,          // viewport coords
 *   receiverBand: receiverBandRect,
 *   size,
 * });
 * <DecisionPlaque
 *   title={session.action.label}
 *   placement={placement}
 *   confirm={hasOptions(session) ? undefined : {
 *     label: 'Confirm',
 *     onConfirm: confirmMultiSelect,
 *     disabledReason: confirmDisabledReason(allSlotsSatisfied(session), activeSlot(session)?.prompt),
 *   }}
 *   onAdvance={session.slots.length > 1 && !isLastSlot(session) ? advanceSlot : undefined}
 *   onUndo={hasPickToRetract ? retractLastPick : undefined}
 *   cancel={forced === null ? { onCancel: cancelMultiSelect } : undefined}
 * />
 * ```
 *
 * Every rect passed in must be in **viewport coordinates** — the space
 * `getBoundingClientRect` reports, which `LiveMatchTable.notePlaneGeometry`
 * already converts the plane's own rects into. That conversion is the seam where
 * a coordinate-space mistake would be silent, so it is stated here as part of the
 * contract rather than left to be inferred.
 *
 * Three props are deliberately **absent** and must stay absent:
 *
 * - no question, progress, count, or deadline — §10.1 gives those to `PromptStrip`;
 * - no `disabled: boolean` — only a server-stated reason may grey a control (D14);
 * - no `onEscape` — `useTableKeyboard` owns `Escape` for the whole shell, and a
 *   second handler here would let the two disagree about what one press cancels.
 */
export { DecisionPlaque } from './DecisionPlaque';
export type { DecisionPlaqueProps, PlaqueConfirm, PlaqueCancel } from './DecisionPlaque';
export { confirmDisabledReason } from './confirmReason';
export {
  PLAQUE,
  clampToViewport,
  estimatePlaqueSize,
  placeDecisionPlaque,
  plaqueForm,
} from './plaqueAnchor';
export type {
  PlaqueAnchorInput,
  PlaqueDockReason,
  PlaqueForm,
  PlaquePlacement,
  PlaqueSide,
} from './plaqueAnchor';
