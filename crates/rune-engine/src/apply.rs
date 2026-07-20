//! The state-transition pipeline: [`apply_action`] and its per-action helpers.
//!
//! [`apply_action`] is the single entry point of the engine. It validates the
//! chosen action against [`crate::valid_actions`], clones, applies the action,
//! runs state-based actions, and collects triggers, and returns the new state.
//! Enters-the-battlefield self-replacements (CR 614.1c/614.12) are not a stage of
//! this pipeline — they modify the entry event itself and so run at the
//! battlefield-entry seam ([`crate::card::apply_enters_replacements`]). Pure over
//! an immutable [`crate::GameState`].

use crate::actions::{action_is_legal, Action};
use crate::sba::run_state_based_actions;
use crate::stack::{StackId, StackObject, StackObjectKind};
use crate::state::{GameEvent, GameState};
use crate::triggers::collect_triggers;
use crate::CardDatabase;

mod cast;
mod combat;
mod commander;
mod mulligan;
mod turn;

pub(crate) use cast::*;
pub(crate) use combat::*;
pub(crate) use commander::*;
pub(crate) use mulligan::*;
pub(crate) use turn::*;

#[cfg(test)]
mod test_support;

/// The single entry point of the engine: a pure state transition.
///
/// Pipeline: validate `action` against [`crate::valid_actions`] → clone → apply
/// (a battlefield entry applies the entering card's own CR 614 self-replacements at
/// its seam) → state-based-actions loop → collect triggers and put them on the stack
/// → return. An action that is not currently legal is rejected as a no-op: the input
/// is returned unchanged (never mutated either way). `db` supplies the immutable
/// oracle data the pipeline reads.
#[must_use]
pub fn apply_action(state: &GameState, action: &Action, db: &CardDatabase) -> GameState {
    // 1. Validate against the actions actually on offer, including — for a
    //    targeted action — its chosen targets against freshly computed legal sets
    //    (ADR 0009 §Enumeration). An illegal action is a no-op: the input is
    //    returned unchanged rather than erroring.
    if !action_is_legal(state, action, db) {
        return state.clone();
    }

    // 2. Clone: every mutation below happens on this owned copy.
    let mut next = state.clone();

    // 3. Apply the chosen action.
    match action {
        Action::PassPriority => apply_pass_priority(&mut next, db),
        Action::PlayLand { card } => apply_play_land(&mut next, *card, db),
        Action::ActivateAbility {
            permanent,
            index,
            targets,
        } => {
            apply_activate_ability(&mut next, *permanent, *index, targets, db);
        }
        Action::CastSpell { card, targets } => apply_cast_spell(&mut next, *card, targets, db),
        Action::Discard { card } => apply_discard(&mut next, *card, db),
        Action::Mulligan => apply_mulligan(&mut next),
        Action::Keep { bottom } => apply_keep(&mut next, bottom),
        Action::DeclareAttackers { attackers } => {
            apply_declare_attackers(&mut next, attackers, db);
        }
        Action::DeclareBlockers { blocks } => apply_declare_blockers(&mut next, blocks),
        Action::OrderCombatDamage { orders } => apply_order_combat_damage(&mut next, orders),
        Action::ReturnCommanderToCommandZone { card } => {
            apply_return_commander(&mut next, *card);
        }
        Action::DeclineCommanderReturn { card } => apply_decline_commander_return(&mut next, *card),
        Action::Concede => apply_concede(&mut next),
    }

    // Enters-the-battlefield self-replacements (CR 614.1c/614.12 — "enters tapped",
    // "enters with counters") are NOT a stage here: a replacement modifies the entry
    // event itself, so it is applied at the battlefield-entry seam inside step 3
    // (`apply_enters_replacements`), before the state-based-action loop and before any
    // ETB trigger below. That ordering is load-bearing — a 0/0 entering with two +1/+1
    // counters must already be a 2/2 when the SBA loop runs (CR 704.5f).

    // 4. State-based actions, run to a fixed point.
    run_state_based_actions(&mut next, db);

    // 5. Collect triggers by diffing before/after and put each on the stack. They
    //    observe the post-replacement state (the entered permanent already carries
    //    its "as enters" tapped state / counters, CR 614.12).
    for trigger in collect_triggers(state, &next, db) {
        let id = next.mint_id();
        next.stack.push(StackObject {
            id: StackId(id),
            controller: trigger.controller,
            kind: StackObjectKind::Ability {
                source: trigger.source,
                effects: trigger.effects,
            },
            // Target choosing on announcement is issue #71; triggers carry none.
            targets: Vec::new(),
        });
    }

    // The terminal-result event closes the sequence. Every fact that could end the
    // game — a death, damage, a decking draw — has already been recorded at its own
    // seam above, so a `GameOver` recorded here lands last, after its causes. It is
    // derived (never stored, CR 104.2a) and emitted once, the transition it becomes
    // true.
    if state.result().is_none() {
        if let Some(result) = next.result() {
            next.record_event(GameEvent::GameOver { result });
        }
    }

    next
}
