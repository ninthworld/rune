/**
 * The contextual **stack stage** and **activity surface** — the public surface of
 * issue #534's replacement for ADR 0023's permanent right rail, under
 * [ADR 0032](../../../../../docs/decisions/0032-contextual-shell-anatomy.md).
 *
 * Design authorities: `docs/design/stack-and-relationships.md` §1–§3, §9, §10.4
 * (the stage) and `docs/design/control-language.md` §4.4/D7 (the column the stage
 * shares with the control cluster).
 *
 * ## The integration contract
 *
 * Both components mount as **direct children of the match shell root**
 * (`live-match.module.css`'s `.shell`, already `position: relative`) — siblings
 * of `.top` / `.scene` / `.bottom`, never inside one. Two reasons, both binding:
 *
 * 1. A shell region carrying a `z-index` creates a stacking context its
 *    descendants cannot escape (`shellLayout.ts`'s `LAYER` note). Rendered inside
 *    a region, the history overlay would be pinned below `--rune-z-decision`.
 * 2. A region gives its content a *track*. The whole acceptance criterion here is
 *    that an empty stack and an empty log consume **no** permanent battlefield
 *    width, and both components return `null` when they have nothing to show —
 *    which only means anything if they were not occupying a grid cell.
 *
 * ```tsx
 * <div className={styles.shell}>
 *   … .top / .scene / .bottom …
 *   <StackStage
 *     view={view}
 *     compact={compact}
 *     targeting={targeting ? { candidates: activeCandidates(targeting), onPick: pickTarget } : undefined}
 *     onInspect={setInspectedId}
 *   />
 *   <ActivitySurface view={view} onHighlight={highlight} highlightedId={highlightedId} />
 * </div>
 * ```
 *
 * Every prop above already exists in `LiveMatchTable.tsx` and was being passed to
 * `Rail.tsx`, in the same shapes. `Rail`'s remaining props — `isUnseen`,
 * `unreadCount`, `onSeen` — are gone on purpose: {@link ActivitySurface} owns
 * `useUnreadLog` itself so the badge cannot disagree with the panel it opens.
 *
 * ## What integration must NOT do
 *
 * - Do not give either component a grid track, a min-width, or a wrapper with a
 *   background. They position themselves.
 * - Do not render a RESOLVE / RESPOND pair inside the stage. Those are the
 *   control cluster's, at the stage's foot (§1.4, D17); the stage leaves
 *   {@link CLUSTER_CLEARANCE} px of the flank free for exactly that.
 * - Do not retire `GameLog.tsx` when `Rail.tsx` goes. {@link ActivitySurface}
 *   composes it as the full-history body; it is the "explicit way to inspect full
 *   history" the issue requires.
 *
 * ## Where each derived value is consumed
 *
 * `deriveStackStage` → `StackStage.tsx` (the only production caller);
 * `stackStyleVars` / `STACK_STAGE` / `CLUSTER_CLEARANCE` → `stack.module.css`
 * through both components' inline custom properties; `deriveActivity` /
 * `newestSequence` / `ACTIVITY` → `ActivitySurface.tsx`. Nothing here is exported
 * without a caller.
 */
export { StackStage } from './StackStage';
export type { StackStageProps, StackTargeting } from './StackStage';

export { ActivitySurface } from './ActivitySurface';
export type { ActivitySurfaceProps } from './ActivitySurface';

export {
  deriveStackStage,
  stackStyleVars,
  railViewportHeight,
  tierHeight,
  STACK_STAGE,
  CLUSTER_CLEARANCE,
} from './stackStage';
export type {
  StackStageModel,
  StackStageEntry,
  StackStageOptions,
  StackStageView,
  StackTier,
  StackLayout,
  StackSticky,
} from './stackStage';

export { deriveActivity, isMeaningful, newestSequence, ACTIVITY } from './activityFeed';
export type { ActivityModel, ActivityLine, ActivityView } from './activityFeed';
