//! Answering the mid-resolution player choice the game is waiting on.

use super::*;
use crate::choice::{
    apply_choice_outcome, pending_player_choice, take_confirmed_effects, ChoiceQuestion,
};
use crate::id::CardInstanceId;
use crate::resolve::resume_after_choice;

/// Answer the choice at the head of [`GameState::pending_choices`](crate::GameState)
/// with `chosen`, then let the suspended resolution continue.
///
/// Three steps, in this order and no other:
///
/// 1. **Take the choice off the queue.** It is answered; leaving it there while its
///    outcome runs would make [`pending_player_choice`] briefly disagree with reality
///    and, worse, let the resumed resolution see its own unanswered question.
/// 2. **Carry out the outcome** — the chosen cards move, and so do the ones passed over
///    where the outcome says so (a search shuffles, a look bottoms what it looked at).
/// 3. **Resume**, if this was the last choice its effect posed: the rest of the
///    suspended object's effects, and for a spell the card's final zone (CR 608.3).
///    That continuation may itself pose a further choice, which simply queues behind
///    whatever is left — a card that draws and then discards suspends twice.
///
/// Legality has already been established by [`crate::apply_action`]'s gate
/// ([`crate::choice::answer_is_legal`]), so this writes rather than re-deciding. An
/// answer with no choice pending is a no-op.
pub(crate) fn apply_answer_choice(
    state: &mut GameState,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Cards(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Cards(request) = &answered.question else {
        return;
    };
    apply_choice_outcome(state, request, chosen, db);
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **color choice** with `color`: carry out whatever the question was
/// for, then let any suspended resolution continue.
///
/// The same three steps [`apply_answer_choice`] takes, and the simplest instance of
/// them: there is no candidate set to re-derive, because every color is always a legal
/// answer (CR 105.1), and no aftermath for the answers not given. An effect producing
/// more than one mana queued one question per point, so answering this one leaves the
/// next at the head of the queue and the player is asked again — which is the whole
/// meaning of "in any combination of colors".
///
/// The other thing a colour answer can do is **finish a battlefield entry** (CR 614.12):
/// the permanent that was waiting for it arrives here, colour and all. It carries no
/// [`Resume`](crate::Resume) — nothing was suspended, because the entry is the last step
/// of a resolution rather than one of its effects — so the `if let` below simply finds
/// nothing, which is the correct amount of special-casing.
///
/// An answer with no color choice pending is a no-op.
pub(crate) fn apply_answer_color(
    state: &mut GameState,
    color: crate::mana::Color,
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Color(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Color(request) = &answered.question else {
        return;
    };
    crate::choice::apply_color_outcome(state, answered.chooser, request, color, db);
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **yes-or-no** with `accept`, then let the suspended resolution
/// continue (CR 608.2 — see [`crate::Effect::May`]).
///
/// The same three steps [`apply_answer_choice`] takes, with one difference that is the
/// whole design: an accepted effect is not applied *here*. It is spliced onto the front
/// of the suspended remainder and resumed through the ordinary effect walk, so
///
/// - the optional effects and the effects that followed them run in one pass, in
///   printed order;
/// - an accepted effect that poses a *further* choice suspends exactly as any other
///   effect would, with no second mechanism;
/// - declining is the same code path with nothing spliced, which is what makes "a
///   decline leaves the game as if the effect were absent" true by construction rather
///   than by matching two branches carefully.
///
/// The offer's **targets** are spliced with its effects, for the third reason: a `may`
/// declares the group of the effect it wraps, so the target was chosen at announcement
/// and must be handed back to that effect on acceptance — and must go nowhere at all on
/// a decline, or the next effect in the remainder would inherit a target that was never
/// aimed at it. The wrapped effect re-checks it on the way through (CR 608.2c), so a
/// target that has become illegal is skipped rather than applied.
///
/// Paying happens in [`take_confirmed_effects`], atomically with the acceptance.
/// Legality — including whether the cost is payable at all — has already been
/// established by [`crate::apply_action`]'s gate. An answer with no yes-or-no pending is
/// a no-op.
pub(crate) fn apply_answer_confirm(state: &mut GameState, accept: bool, db: &CardDatabase) {
    let Some(ChoiceQuestion::Confirm(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Confirm(request) = &answered.question else {
        return;
    };
    let taken = take_confirmed_effects(state, answered.chooser, request, accept);
    // A confirmation is the only choice its effect poses, so it always carries the
    // remainder; without one there is nothing left to resolve and nothing to splice on.
    if let Some(mut resume) = answered.resume {
        if let Some(mut effects) = taken {
            effects.append(&mut resume.effects);
            resume.effects = effects;
            let mut targets = request.targets.clone();
            targets.append(&mut resume.targets);
            resume.targets = targets;
        }
        resume_after_choice(state, resume, db);
    }
}
