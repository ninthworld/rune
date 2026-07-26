/**
 * The **control cluster** — the lower-right column that is now the one action
 * home (`docs/design/control-language.md` §3.3 and §4, issue #534, under
 * [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * 268 px wide, 28 px from the viewport edge, stacked with 12 px gaps:
 *
 * ```
 *   PASS PRIORITY                 primary  (§4.2, one blue control at most)
 *   ◉  UNDO                       menu icon (D5) + utility pill (§8)
 *   MANA ⓖⓖ                       the receiver's floating mana (#567)
 *   MAIN PHASE / YOUR TURN ●●●○›  phase plaque (§5)
 * ```
 *
 * The decision surface (`table/decision`) stacks directly above this column when
 * the server is waiting on an answer, which is what makes the whole lower-right
 * corner one action area rather than three places to look.
 *
 * ADR 0032 moved this surface: ADR 0023 put the action dock beside the hand, both
 * approved baselines put it here, and C5 records the resolution — the
 * *commitment* ("one action home") is preserved, its *location* moves.
 *
 * ## Three forms, one geometry
 *
 * - **Full** (panel 6) — the stack is empty: a full-width stadium primary.
 * - **Paired** (§4.4 / D7, zones panel 10) — the stack is non-empty: the primary
 *   becomes the compact 118 px form with the RESPOND secondary below it, so the
 *   pair reads against the stack rail above. *The cluster does not move* — the
 *   rail's foot and the cluster are the same place; only the form changes.
 * - **Compact** (panel 6b) — nothing is offered: plaque + menu icon only. Those
 *   are the two things always available, which is what makes 6b coherent, and
 *   nothing that was actionable is hidden by the degrade.
 *
 * ## What this component refuses to do
 *
 * It derives nothing about the game. Which action fills the blue slot is
 * {@link derivePrimary}'s eight-rule table over the server's own list, the drawn
 * words are `action.label` verbatim (never `RESOLVE` for `PASS PRIORITY` — that
 * is GAP-2, a rules judgement the client may not make), and pressing anything
 * echoes back the `ValidAction` the server sent. `concede` never reaches this
 * surface at all: D9 excludes it from the primary and §3.3 puts it behind the
 * menu with a confirmation, so the highest-stakes action in the game is never one
 * slip away from the most-pressed button.
 *
 * The `P` binding on the primary is drawn as a hint only. The key itself is the
 * shell's shipped global binding (§7.1); adding a second listener here would give
 * one key two owners.
 */
import type { EntityId, GameView, Phase, ValidAction } from '../../protocol';
import { cx } from '../../chrome/cx';
import { ControlButton, IconButton } from './ControlButton';
import type { ControlSession } from './controlPrimary';
import { RESPOND_ACCESSIBLE_NAME, RESPOND_LABEL, derivePrimary } from './controlPrimary';
import { ManaReservoir } from './ManaReservoir';
import { PhasePlaque } from './PhasePlaque';
import s from './cluster.module.css';

export interface ControlClusterProps {
  /** The one authoritative view. Everything below is rebuilt from it. */
  view: GameView;
  /** The client's ephemeral selection, if one entity is current (ADR 0004). */
  selectedId?: EntityId;
  /** Whether a targeting or multi-select session owns the advance (§4.2 1–2). */
  session?: ControlSession;
  /** Echo the chosen action back (the store reads its id + binding token). */
  onChoose: (action: ValidAction) => void;
  /**
   * §4.3's navigation control: move focus into the hand fan so the player can
   * cast instead of passing. It **sends nothing** — `RESPOND` is not an action
   * and no such entry exists in `valid_actions`. Omitted, the control does not
   * render, because a navigation control with nowhere to go is not a control.
   */
  onRespond?: () => void;
  /** Open the game menu (D5): settings, shortcuts, log, concede-with-confirm. */
  onOpenMenu: () => void;
  /** Whether the menu drawer is open, for the icon's `aria-expanded`. */
  menuOpen?: boolean;
  /** The id of the menu surface the icon discloses. */
  menuControls?: string;
  /** Set the receiver's priority stops from the plaque's step list (ADR 0020). */
  onSetStops?: (stops: Phase[]) => void;
  /**
   * §8's retract-one-local-step control, offered ONLY while a targeting session
   * has an answered slot to give back. It is never a takeback: no `undo` exists
   * in `valid_actions` and there is no client→server message for one (GAP-1),
   * which is why panel 6 draws this pill in the neutral state and the cluster
   * does not (C8 — an unavailable action does not render).
   */
  onUndo?: () => void;
  /**
   * The local, ≤5 s, non-load-bearing submission lock (D13/GAP-5). Presentation
   * only: the protocol carries no acknowledgement, so it can never be
   * authoritative, and a fresh mount never reproduces it.
   */
  pending?: boolean;
  /** Bumped on `view.action_rejected` to replay the recovery shake once. */
  shakeNonce?: number;
}

export function ControlCluster({
  view,
  selectedId,
  session,
  onChoose,
  onRespond,
  onOpenMenu,
  menuOpen,
  menuControls,
  onSetStops,
  onUndo,
  pending,
  shakeNonce,
}: ControlClusterProps) {
  const derived = derivePrimary({
    validActions: view.valid_actions,
    selectedId,
    session,
    stackDepth: view.stack.length,
  });

  // Bound once: every press below must echo the SAME entry the derivation
  // returned, by identity, so there is no path by which a re-derived action
  // reaches `ChooseAction`.
  const primary = derived.primary;
  const respond = derived.respond && onRespond !== undefined;
  const paired = derived.form === 'compact' && primary !== undefined;

  // §3.3's degrade condition, stated as it is written there: "no primary and no
  // utility control is offered". That is a superset of rule 8 — an entity action
  // with nothing selected offers the cluster nothing either — and rendering an
  // empty 268 px column in that case would be the "hidden but present" chrome
  // ADR 0032 removed. The plaque still reads "Waiting" for rule 8 ALONE, because
  // §13 pins that word to an empty `valid_actions` and the player is not idle
  // merely because the blue slot is.
  const compact = primary === undefined && derived.secondaries.length === 0 && onUndo === undefined;

  return (
    <div
      className={cx(s.cluster, compact && s.clusterCompact)}
      role="group"
      aria-label="Match controls"
      data-testid="control-cluster"
      data-compact={compact || undefined}
    >
      {primary !== undefined && (
        <ControlButton
          variant={paired ? 'primaryCompact' : 'primary'}
          label={primary.label}
          onPress={() => onChoose(primary)}
          // The countdown chip rides the control the clock is waiting on.
          deadlineSeconds={view.action_deadline}
          pending={pending}
          shakeNonce={shakeNonce}
          hint={primary.type === 'pass_priority' ? 'P' : undefined}
          testId="control-primary"
        />
      )}

      {respond && (
        <ControlButton
          variant="secondary"
          label={RESPOND_LABEL}
          accessibleName={RESPOND_ACCESSIBLE_NAME}
          onPress={() => onRespond?.()}
          testId="control-respond"
        />
      )}

      {/* The echo (§4.2 rules 4 and 7, ADR 0004). Equal weight, deliberately: the
          client refuses to rank several offered actions, so nothing here is
          promoted and nothing is blue. */}
      {derived.secondaries.length > 0 && (
        <div className={s.echo} data-testid="control-echo">
          {derived.secondaries.map((action) => (
            <ControlButton
              key={action.id}
              variant="secondary"
              label={action.label}
              onPress={() => onChoose(action)}
              testId={`control-secondary-${action.id}`}
            />
          ))}
        </div>
      )}

      {/* Panel 6's second row. The menu icon is always here; the utility pill
          only when there is a local step to retract. In the compact form the two
          survivors share the plaque's row instead. */}
      {!compact && (
        <div className={s.utilityRow}>
          <MenuIcon onOpenMenu={onOpenMenu} menuOpen={menuOpen} menuControls={menuControls} />
          {onUndo && (
            <ControlButton
              variant="utility"
              label="UNDO"
              accessibleName="Undo the last target pick"
              onPress={onUndo}
              testId="control-undo"
            />
          )}
        </div>
      )}

      {/* The receiver's floating mana, above the plaque and below the controls
          that spend it (#567). Absent when the pool is empty — an empty
          reservoir is a permanent widget reading zero, which is the always-there
          dashboard ADR 0032 removed. */}
      <ManaReservoir pool={view.mana_pool} />

      {compact ? (
        <div className={s.compactRow}>
          <PhasePlaque view={view} onSetStops={onSetStops} waiting={derived.waiting} compact />
          <MenuIcon onOpenMenu={onOpenMenu} menuOpen={menuOpen} menuControls={menuControls} />
        </div>
      ) : (
        <PhasePlaque view={view} onSetStops={onSetStops} waiting={derived.waiting} />
      )}
    </div>
  );
}

/**
 * D5's menu handle. Extracted because it renders in two places — the full
 * cluster's utility row and the compact cluster's plaque row — and the two must
 * stay the same control, not two controls that drift.
 */
function MenuIcon({
  onOpenMenu,
  menuOpen,
  menuControls,
}: Pick<ControlClusterProps, 'onOpenMenu' | 'menuOpen' | 'menuControls'>) {
  return (
    <IconButton
      glyph="☰"
      label="Game menu"
      onPress={onOpenMenu}
      expanded={menuOpen}
      controls={menuControls}
      testId="control-menu"
    />
  );
}
