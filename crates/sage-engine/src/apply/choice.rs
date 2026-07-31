//! Answering the mid-resolution player choice the game is waiting on.

use super::*;
use crate::choice::{apply_choice_outcome, pending_player_choice};
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
    if pending_player_choice(state).is_none() {
        return;
    }
    let answered = state.pending_choices.remove(0);
    apply_choice_outcome(state, &answered.request, chosen, db);
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}
