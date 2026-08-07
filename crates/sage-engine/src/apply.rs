//! The state-transition pipeline: [`apply_action`] and its per-action helpers.
//!
//! [`apply_action`] is the single entry point of the engine. It validates the
//! chosen action against [`crate::valid_actions`], clones, applies the action,
//! runs state-based actions, and collects triggers, and returns the new state.
//! Enters-the-battlefield self-replacements (CR 614.1c/614.12) are not a stage of
//! this pipeline — they modify the entry event itself and so run at the
//! battlefield-entry seam (the CR 614 replacement layer). Pure over
//! an immutable [`crate::GameState`].

use crate::actions::{action_is_legal, Action};
use crate::sba::run_state_based_actions;
use crate::stack::{AbilityOrigin, StackId, StackObject, StackObjectKind};
use crate::state::{GameEvent, GameState};
use crate::triggers::collect_triggers;
use crate::CardDatabase;

mod cast;
mod choice;
mod combat;
mod commander;
mod mulligan;
mod turn;

pub(crate) use cast::*;
pub(crate) use choice::*;
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
    //    (ADR 0004 §Enumeration). An illegal action is a no-op: the input is
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
            payment,
        } => {
            apply_activate_ability(&mut next, *permanent, *index, targets, payment, db);
        }
        // CR 113.6: the same announcement over a card in a graveyard. Only the source
        // differs — it is a card in a zone, not a permanent — so this is its own helper
        // rather than a branch inside one that assumes the battlefield.
        Action::ActivateAbilityFromGraveyard {
            card,
            index,
            targets,
            payment,
        } => {
            apply_activate_ability_from_graveyard(&mut next, *card, *index, targets, payment, db);
        }
        // CR 601.2, in order and in one step: the payment's mana abilities are activated
        // first, then the cost is paid and the spell goes on the stack. Both halves have
        // already been found legal together above, and `next` is a copy — so a cast that
        // could not be completed never reaches here, and the state returned is the one
        // that came in, with nothing tapped. That is the rules' own rewind, for free.
        Action::CastSpell {
            card,
            mode,
            x,
            targets,
            payment,
        } => {
            crate::actions::apply_payment(&mut next, db, payment);
            apply_cast_spell(&mut next, *card, *mode, *x, targets, payment, db);
        }
        Action::ChooseTriggerTargets {
            ability,
            mode,
            targets,
        } => {
            apply_choose_trigger_targets(&mut next, *ability, *mode, targets);
        }
        Action::AnswerChoice { chosen } => apply_answer_choice(&mut next, chosen, db),
        Action::AnswerConfirm { accept } => apply_answer_confirm(&mut next, *accept, db),
        Action::AnswerColor { color } => apply_answer_color(&mut next, *color, db),
        Action::AnswerReplacement { index } => apply_answer_replacement(&mut next, *index, db),
        Action::AnswerCardName { card } => apply_answer_card_name(&mut next, *card, db),
        Action::AnswerOrder { order } => apply_answer_order(&mut next, order, db),
        Action::AnswerPermanents { chosen } => apply_answer_permanents(&mut next, chosen, db),
        Action::AnswerPermanent { chosen } => apply_answer_permanent(&mut next, *chosen, db),
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

    // CR 608.2f: a resolution that offered a card has been answered by the player playing
    // it. The offer was the question, so taking it settles the question — the pending entry
    // is dropped and the suspended resolution picks up where it left off. Done here rather
    // than inside the cast so that the cast stays the ordinary one, with nothing about
    // being-offered threaded through it.
    settle_offered_play(&mut next, action, db);

    // Enters-the-battlefield self-replacements (CR 614.1c/614.12 — "enters tapped",
    // "enters with counters") are NOT a stage here: a replacement modifies the entry
    // event itself, so it is applied at the battlefield-entry seam inside step 3
    // (the CR 614 replacement layer), before the state-based-action loop and before any
    // ETB trigger below. That ordering is load-bearing — a 0/0 entering with two +1/+1
    // counters must already be a 2/2 when the SBA loop runs (CR 704.5f).

    // 4. State-based actions, run to a fixed point.
    run_state_based_actions(&mut next, db);

    // 5. Collect triggers by diffing before/after and put each on the stack. They
    //    observe the post-replacement state (the entered permanent already carries
    //    its "as enters" tapped state / counters, CR 614.12).
    //    A **delayed** triggered ability (CR 603.7) is collected from a fourth source
    //    list that is not an object, so it is a sibling call rather than a branch inside
    //    the collector — and firing spends it (CR 603.7b: `the next time`, once), which is
    //    the one thing the pure collector cannot do for itself.
    let mut collected = collect_triggers(state, &next, db);
    for (spent, trigger) in crate::delayed::delayed_triggers_fired(state, &next, db) {
        next.delayed_triggers.retain(|pending| pending.id != spent);
        collected.push(trigger);
    }
    //    A **reflexive** ability (CR 603.11) is a fifth source, and the only one that was
    //    created by the very transition it fires in: the resolution decided whether it
    //    fired while it was running, so there is nothing left to evaluate here and the
    //    list is simply drained. It arrives unaimed like a printed trigger, which is what
    //    CR 603.11b asks for — a `when you do` chooses its target as it goes on the stack.
    for pending in std::mem::take(&mut next.reflexive_triggers) {
        collected.push(crate::triggers::Trigger {
            source: crate::stack::AbilitySource::Permanent(pending.source),
            controller: pending.controller,
            effects: pending.effects,
            targets: Vec::new(),
            paid: crate::PaidCost {
                source_power: pending.source_power,
                ..crate::PaidCost::default()
            },
            // A reflexive ability is never modal (CR 603.11): it is the consequence of a
            // choice already made, not a further one.
            modes: Vec::new(),
        });
    }
    for trigger in collected {
        // CR 603.3c: a triggered ability that requires targets and has no legal
        // choice for one of its slots is removed from the stack — so it never goes on
        // in the first place here. Checked against the *controller's* legal sets,
        // since a possessive spec means different things from different seats.
        //
        // Read off the **groups** rather than off a single spec, so an ability with more
        // than one slot is judged on all of them. That is what makes CR 603.3c right for
        // a trigger with one slot per seat: a player controlling nothing leaves its slot
        // with no legal choice, and the whole ability is removed — which is what
        // `for each player, choose target permanent that player controls` does.
        //
        // Only a **required** slot can make an ability unanswerable. An `up to` group has
        // a legal answer with an empty candidate set: none of them.
        let groups: Vec<_> = trigger
            .effects
            .iter()
            .flat_map(|effect| effect.target_groups(next.players.len()))
            .filter(|group| group.min >= 1)
            .collect();
        // A trigger the event already aimed (CR 603.7c) is not looking for a legal
        // choice — it has one — so CR 603.3c has nothing to say about it.
        let unanswerable = trigger.targets.is_empty()
            && groups.iter().any(|group| {
                crate::actions::legal_targets_for_spec(
                    group.spec,
                    &next,
                    trigger.controller,
                    trigger.source.permanent(),
                    db,
                )
                .is_empty()
            });
        if unanswerable {
            continue;
        }
        let id = next.mint_id();
        next.stack.push(StackObject {
            id: StackId(id),
            controller: trigger.controller,
            kind: StackObjectKind::Ability {
                source: trigger.source,
                // The *trigger* push site (CR 603.3): the game put this here, no
                // player activated it. The counterpart to the activation push in
                // `apply_activate_ability` (issue #579).
                origin: AbilityOrigin::Triggered,
                effects: trigger.effects,
                // A **modal** trigger arrives with its modes and no answer, exactly as it
                // arrives with target slots and nothing in them (CR 603.3c/603.3d). Both
                // are answered by its controller before anyone gets priority, and both
                // are "owed" purely by being unanswered — there is no flag either way.
                modes: trigger.modes,
                mode: None,
            },
            // A trigger arrives **unaimed** (CR 603.3d): the game put it here, so its
            // controller has had no chance to choose. When it declares target slots,
            // step 6 hands them priority to fill them before anyone else acts. The
            // exception is a **delayed** ability, whose slot the trigger event itself
            // filled (CR 603.7c) — there is nothing for its controller to decide.
            targets: trigger.targets,
            // A trigger has no cost (CR 603.3), so this is empty for every printed one —
            // the exception is what a reflexive ability recorded about its own source
            // before that source could die (CR 608.2h).
            paid: trigger.paid,
        });
    }

    // 6. Hand priority to whoever the game is currently waiting on, and give it back
    //    when it is waiting on no one.
    //
    //    Two kinds of choice interrupt priority, and they are checked in the order they
    //    must be answered. A **mid-resolution player choice** comes first: an object is
    //    part-way through resolving and nothing else — not even aiming a trigger that
    //    resolution produced — may happen until the question is answered. Then a
    //    **triggered ability owed targets** (CR 603.3b/603.3d): it is put on the stack,
    //    and its controller chooses its targets, before any player receives priority.
    //
    //    The interrupted holder is remembered in one slot because the chooser is
    //    frequently not them and only one seat can hold priority: a creature killed by
    //    an opponent's removal spell gives its own controller a trigger to aim while the
    //    opponent retains priority, and a Mind Rot resolving on its caster's turn asks
    //    the other seat to discard.
    let chooser = crate::pending_player_choice(&next)
        .map(|pending| pending.chooser)
        .or_else(|| {
            crate::pending_trigger_target_choice(&next)
                .and_then(|id| crate::triggers::controller_of_stack_object(&next, id))
        });
    match chooser {
        Some(chooser) => {
            if next.interrupted_priority.is_none() {
                next.interrupted_priority = Some(next.priority);
            }
            next.priority = chooser;
        }
        None => {
            if let Some(restored) = next.interrupted_priority.take() {
                next.priority = restored;
            }
        }
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

/// Settle a [`ChoiceQuestion::PlayCard`] the player answered by **playing the card**
/// (CR 608.2f, issue #787).
///
/// The card has already moved and the spell is already on the stack, or the land is already
/// on the battlefield — this only drops the question that offered it and resumes what was
/// suspended. A play of some *other* card cannot reach here: while the question stands, the
/// offer path returns that one card's play and nothing else, and the legality gate re-checks
/// it against the same list.
///
/// Separate from the cast itself so the cast stays the ordinary one, with nothing about
/// being-offered threaded through it.
fn settle_offered_play(state: &mut GameState, action: &Action, db: &CardDatabase) {
    let played = match action {
        Action::CastSpell { card, .. } | Action::PlayLand { card } => card.id,
        _ => return,
    };
    let Some(pending) = state.pending_choices.first() else {
        return;
    };
    if pending
        .question
        .play_card()
        .is_none_or(|request| request.card.id != played)
    {
        return;
    }
    let answered = state.pending_choices.remove(0);
    // The freeze is over: whoever was interrupted gets priority back, exactly as when any
    // other question is answered.
    state.interrupted_priority = None;
    if let Some(request) = answered.question.play_card() {
        crate::choice::bottom_the_rest(state, request, Some(played));
    }
    if let Some(resume) = answered.resume {
        crate::resolve::resume_after_choice(state, resume, db);
    }
}
