/**
 * The **primary derivation** — which offered action, if any, fills the one blue
 * slot (`docs/design/control-language.md` §4, issue #534, under
 * [ADR 0032](../../../../docs/decisions/0032-contextual-shell-anatomy.md)).
 *
 * §4.1 allows at most one blue primary on screen, and it is never invented: the
 * slot holds a control only when the eight-rule table of §4.2 resolves to an
 * entry that is already in `valid_actions[]`, and the drawn word is that entry's
 * server-supplied `label`. This module is that table, and nothing else.
 *
 * ## Why it is a pure function
 *
 * The rule that makes the whole cluster safe is that **every test below is a
 * count or a membership test over the server's own list** — never a judgement
 * about legality, cost, or effect. Keeping the table in one pure function is how
 * that stays checkable: `controlPrimary.test.ts` walks it rule by rule, and a
 * reviewer can read the whole decision without opening a component. A derivation
 * scattered across JSX is a derivation that grows a special case.
 *
 * ## The two deliberate refusals
 *
 * - **Rules 4 and 7 return an empty primary on a tie.** With two or more actions
 *   offered on equal terms, picking the "best" one to promote would be a client
 *   judgement about the game. They render flat, as equal-weight secondaries. Any
 *   future "improvement" that ranks them is the bug this comment exists to stop.
 * - **`concede` is never eligible** (D9), for the primary *or* the echo. §3.3
 *   puts it behind the game menu with a confirmation, physically away from the
 *   most-pressed button in the game.
 *
 * ## What it does not decide
 *
 * The `RESOLVE` the zones baseline draws is rule 5 with the *server* labelling
 * `pass_priority` contextually (GAP-2). The client may not swap `PASS PRIORITY`
 * for `RESOLVE` itself: deciding that a pass resolves the top of the stack is a
 * rules judgement (ADR 0020 makes exactly this point). This module therefore
 * reports which `ValidAction` won the slot and never a string.
 */
import type { EntityId, ValidAction } from '../../protocol';

/**
 * The open decision session, if any. §4.2 rules 1 and 2 hand the advance to the
 * decision plaque's own CONFIRM/CANCEL while one is open, so the cluster's blue
 * slot deliberately empties rather than offering a second way to commit.
 */
export type ControlSession = 'none' | 'targeting' | 'multiSelect';

/**
 * Which row of the §4.2 table matched, so a caller (and a test) can name the
 * reason for an empty slot rather than guessing at it.
 *
 * `0` is the table's own silence: no row matched, because a rule-3/4 selection
 * carries no offered action, or nothing subject-less is offered while entity
 * actions are. §4.2 does not enumerate that case; the slot stays empty and the
 * cluster degrades exactly as it does for a tie. It is NOT rule 8 — actions
 * exist, so the plaque must not claim the player is waiting.
 */
export type PrimaryRule = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

/** Everything §4.2 is allowed to look at. Note what is absent: the board. */
export interface PrimaryInput {
  /** The server's list — the only source of interactivity (`AGENTS.md`). */
  validActions: readonly ValidAction[];
  /** The client's ephemeral selection, if one entity is current. */
  selectedId?: EntityId;
  /** Whether a targeting or multi-select session is open. Defaults to `none`. */
  session?: ControlSession;
  /**
   * `view.stack.length`. A view field, not a derivation — it drives §4.3's
   * RESPOND and §4.4's form switch, and nothing about which action wins.
   */
  stackDepth?: number;
}

/** The drawn form of the primary slot (§4.4 / D7). */
export type PrimaryForm = 'stadium' | 'compact';

/** What §4.2 resolved to. */
export interface PrimaryDerivation {
  /** The §4.2 row that matched; see {@link PrimaryRule} for `0`. */
  rule: PrimaryRule;
  /** The action filling the blue slot, absent wherever the table says "empty". */
  primary?: ValidAction;
  /**
   * The equal-weight echo: every eligible action the primary did not take. For a
   * selection that is the selected entity's own subject-actions (ADR 0004); with
   * no selection it is the subject-less entries, `concede` excluded.
   */
  secondaries: ValidAction[];
  /**
   * §4.3's navigation control renders. **Not an action** — no such entry exists
   * in `valid_actions` and inventing one is forbidden. It only moves focus into
   * the hand so the player can cast instead of passing.
   */
  respond: boolean;
  /**
   * §4.4/D7: while the stack is non-empty the cluster swaps the full-width
   * stadium for the compact primary, so the pair reads against the stack rail
   * above it. The cluster itself does not move.
   */
  form: PrimaryForm;
  /**
   * Rule 8 exactly: the server offered nothing at all, so the plaque reads
   * "Waiting". Distinct from "no primary" — see {@link PrimaryRule} `0`.
   */
  waiting: boolean;
}

/** The drawn word of §4.3's navigation control. A client word, not a server label. */
export const RESPOND_LABEL = 'RESPOND';

/**
 * §4.3's accessible name. The drawn word does not say what the control does to
 * someone who cannot see that it sits beside the primary.
 */
export const RESPOND_ACCESSIBLE_NAME = 'Respond instead of passing';

/** An action's subject list, with the wire's "absent means empty" convention. */
function subjectOf(action: ValidAction): readonly EntityId[] {
  return action.subject ?? [];
}

/**
 * The §4.2 rule 6/7 eligibility filter: subject-less, and never `concede` (D9).
 * A membership test on two fields the server sent — no classification.
 */
function isEligibleGlobal(action: ValidAction): boolean {
  return subjectOf(action).length === 0 && action.type !== 'concede';
}

/** The rule-3/4 population: the offered actions that name this entity. */
function subjectActionsFor(actions: readonly ValidAction[], id: EntityId): ValidAction[] {
  return actions.filter((action) => subjectOf(action).includes(id));
}

/** The table's verdict, before the view-driven fields are attached. */
interface RuleMatch {
  rule: PrimaryRule;
  primary?: ValidAction;
  secondaries: ValidAction[];
}

/**
 * §4.2, evaluated top to bottom with first match winning. Split out from
 * {@link derivePrimary} so the table reads as a table: every branch is one row,
 * in the document's order, and nothing view-derived is mixed into it.
 */
function matchRule(input: PrimaryInput): RuleMatch {
  const actions = input.validActions;
  const session = input.session ?? 'none';

  // 1 — a multi-select session owns the advance; the plaque's CONFIRM carries it.
  if (session === 'multiSelect') return { rule: 1, secondaries: [] };

  // 2 — a targeting session submits on the last pick; the plaque holds CANCEL.
  if (session === 'targeting') return { rule: 2, secondaries: [] };

  const selectedId = input.selectedId;
  if (selectedId !== undefined) {
    const own = subjectActionsFor(actions, selectedId);
    // 3 — exactly one action names the selection: it is the primary, verbatim.
    if (own.length === 1) return { rule: 3, primary: own[0], secondaries: [] };
    // 4 — two or more: a tie the client refuses to break. All render flat.
    if (own.length >= 2) return { rule: 4, secondaries: own };
    // A selection with nothing offered on it matches no row of the table; fall
    // through so rule 8 can still claim a genuinely empty list.
    return { rule: actions.length === 0 ? 8 : 0, secondaries: [] };
  }

  const globals = actions.filter(isEligibleGlobal);

  // 5 — the shipped, most-pressed control. Membership on `type`, not on meaning:
  // whether this pass resolves the stack top is the server's to say (GAP-2).
  const pass = actions.find((action) => action.type === 'pass_priority');
  if (pass !== undefined) {
    return { rule: 5, primary: pass, secondaries: globals.filter((a) => a !== pass) };
  }

  // 6 — exactly one subject-less entry, `concede` excluded.
  if (globals.length === 1) return { rule: 6, primary: globals[0], secondaries: [] };

  // 7 — two or more: the second deliberate tie. All render as secondaries.
  if (globals.length >= 2) return { rule: 7, secondaries: globals };

  // 8 — nothing offered at all. The plaque reads "Waiting" and the cluster
  // degrades to panel 6b. Nothing that was actionable is hidden.
  return { rule: actions.length === 0 ? 8 : 0, secondaries: [] };
}

/**
 * Resolve §4.2 for one view + selection + session.
 *
 * The returned {@link PrimaryDerivation.primary} is an entry of the input list by
 * identity, never a copy and never a constructed action, so a caller can only
 * echo back an id the server issued.
 */
export function derivePrimary(input: PrimaryInput): PrimaryDerivation {
  const match = matchRule(input);
  const stackDepth = input.stackDepth ?? 0;
  return {
    ...match,
    // §4.3: RESPOND is gated on the primary being `pass_priority` AND the stack
    // being non-empty. Both halves matter — beside a `CAST SPELL` primary there
    // is nothing to respond instead of.
    respond: match.primary?.type === 'pass_priority' && stackDepth > 0,
    // §4.4/D7 switches the primary's FORM on the stack alone, independently of
    // which rule won: the pair has to read against the rail whatever it says.
    form: stackDepth > 0 ? 'compact' : 'stadium',
    waiting: match.rule === 8,
  };
}
