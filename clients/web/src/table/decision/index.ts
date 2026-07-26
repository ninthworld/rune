/**
 * The decision surface's public API (issue #567, `control-language.md` §10).
 *
 * Consumers import `table/decision` rather than reaching into the directory, the
 * same way `table/controls` is consumed. Two things come out of here and the
 * split is the point:
 *
 * - {@link deriveDecision} — **what** is being decided. A pure, total read of
 *   the open session over the server's own `requirements`/`prompts`, producing
 *   the surface's model and the plane's staging in one pass so they cannot
 *   disagree.
 * - {@link DecisionArea} — **how** it is drawn and answered. A pure render of
 *   that model; it submits nothing and derives nothing.
 *
 * ## The integration contract
 *
 * `LiveMatchTable` is the one production caller. It renders the area as a
 * **sibling of the shell's regions** (never inside `.scene` / `.hand` /
 * `.cluster`: a region carrying a z-index creates a stacking context, and a
 * surface trapped in one cannot reach the `decision` rung it needs), and wires:
 *
 * ```tsx
 * const { surface, staging } = deriveDecision(view, {
 *   targeting, multiSelect, forced,
 *   deadline: prompt?.deadline,
 *   canRetract: targeting !== null && targeting.picks.length > 0,
 * });
 * {surface && <DecisionArea surface={surface} onConfirm={…} … />}
 * ```
 *
 * Two props are deliberately **absent** and must stay absent:
 *
 * - no `disabled: boolean` — only a server-stated reason may grey a control
 *   (§3.2, D14, GAP-4), which is why every reason here is a string;
 * - no `onEscape` — `useTableKeyboard` owns `Escape` for the whole shell, and a
 *   second handler here would let the two disagree about what one press cancels.
 *
 * ## What was removed, and why
 *
 * `plaqueAnchor.ts` implemented §10.1/D17's anchoring walk: prefer below/above
 * the subject, reject any position intersecting the subject or a candidate,
 * slide 16 px at a time, dock at the cluster, clamp to the viewport. It was
 * never reached in production. The shell had no subject, candidate, or wing rects
 * to give it — the only rects it holds are DOM measurements collected *after*
 * the plane paints, and the module's own contract forbids measuring (placement
 * must be deterministic for a given view + viewport, or it drifts a pixel a
 * frame). So every landscape ≥ 1180 px call took `dock(input, 'no-subject')` and
 * every other call took `sheet()` with no receiver band, resting on the viewport
 * edge instead of the hand. The walk, the wing rules, the slide, and the
 * dock-reason taxonomy were ~150 lines of unreachable geometry.
 *
 * #567 resolves the either/or it was left in by **removing** it: its goal is one
 * lower-right action area with the choice surface "adjacent to the primary
 * action", which is a fixed home in the cluster's column, and a fixed home has
 * nothing to anchor. Wiring it instead would have meant feeding a
 * deterministic-by-contract function with measured rects to place a surface the
 * issue asks to stop moving.
 */
export { DecisionArea } from './DecisionArea';
export type { DecisionAreaProps } from './DecisionArea';
export { confirmDisabledReason } from './confirmReason';
export { deriveDecision } from './decisionSurface';
export type {
  DecisionChoice,
  DecisionPresentation,
  DecisionRow,
  DecisionRows,
  DecisionSessions,
  DecisionStaging,
  DecisionSurface,
} from './decisionSurface';
