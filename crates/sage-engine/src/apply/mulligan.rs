use super::*;
use crate::ability::Target;
use crate::combat::priority_after_step_change;
use crate::id::{CardInstance, CardInstanceId};
use crate::mulligan::advance_after_keep;
use crate::phase::Step;
use crate::rng::SplitMix64;

/// Discard one card from the active player's hand to its owner's graveyard,
/// satisfying part of the cleanup maximum-hand-size turn-based action (CR 514.1).
///
/// Only ever reached during [`Step::Cleanup`] (the action is offered nowhere
/// else — see [`crate::valid_actions`]). When the discard brings the player to
/// the maximum hand size the cleanup step is finished, so the turn structure
/// walks on to the next step that pauses for a player; priority is re-seated by
/// [`apply_action`]'s caller path via the pass handler's assignment, so it is set
/// here too. While the player is still over the limit the step stays put and more
/// discards are offered.
pub(crate) fn apply_discard(state: &mut GameState, card: CardInstance, db: &CardDatabase) {
    let active = state.active_player;
    {
        let Some(player) = state.players.get_mut(active.0) else {
            return;
        };
        let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
            return;
        };
        let discarded = player.hand.remove(pos);
        player.graveyard.push(discarded);
    }
    if state.step == Step::Cleanup && !active_player_over_hand_size(state) {
        advance_through_turn_based_steps(state, db);
        state.consecutive_passes = 0;
        state.priority = priority_after_step_change(state);
    }
}

/// Take a mulligan during the pre-game London mulligan phase (CR 103.5): shuffle
/// the deciding seat's hand back into its library, redraw a fresh opening hand,
/// and record the mulligan.
///
/// The deciding seat is the priority holder (see [`crate::valid_actions`]).
/// Priority stays with that seat — after redrawing it decides again (keep or
/// mulligan). The reshuffle draws from
/// [`GameState::rng_seed`](crate::GameState::rng_seed) and stores the advanced
/// generator state back, so the whole game still replays from its seed.
pub(crate) fn apply_mulligan(state: &mut GameState) {
    let seat = state.priority;
    let Some(hand_size) = state.mulligan.as_ref().map(|m| m.hand_size) else {
        return;
    };
    // Read the seed, reshuffle-and-redraw for the deciding seat, then store the
    // advanced generator state back into the slot.
    let mut rng = SplitMix64::new(state.rng_seed);
    if let Some(player) = state.players.get_mut(seat.0) {
        player.library.append(&mut player.hand);
        rng.shuffle(&mut player.library);
        let draw = hand_size.min(player.library.len());
        for _ in 0..draw {
            if let Some(card) = player.library.pop() {
                player.hand.push(card);
            }
        }
    }
    state.rng_seed = rng.state();
    if let Some(decision) = state
        .mulligan
        .as_mut()
        .and_then(|m| m.decisions.get_mut(seat.0))
    {
        decision.taken += 1;
    }
    state.record_event(GameEvent::Mulligan { player: seat });
}

/// Keep the current hand during the pre-game London mulligan phase (CR 103.5).
///
/// Puts the chosen `bottom` cards (already validated to be exactly one distinct
/// hand card per mulligan taken — see [`action_is_legal`]) on the bottom of the
/// deciding seat's library in the given order, marks the seat as having kept, and
/// hands the decision to the next still-deciding seat. Once every seat has kept
/// the phase ends and turn 1 begins ([`advance_after_keep`]).
pub(crate) fn apply_keep(state: &mut GameState, bottom: &[Target]) {
    let seat = state.priority;
    if let Some(player) = state.players.get_mut(seat.0) {
        // Remove the chosen cards from hand, preserving the chosen order.
        let chosen: Vec<CardInstanceId> = bottom
            .iter()
            .filter_map(|t| match t {
                Target::Card(id) => Some(*id),
                _ => None,
            })
            .collect();
        let mut bottomed = Vec::with_capacity(chosen.len());
        for id in &chosen {
            if let Some(pos) = player.hand.iter().position(|inst| inst.id == *id) {
                bottomed.push(player.hand.remove(pos));
            }
        }
        // Place them on the bottom of the library. The top of the library is the
        // last element, so the bottom is the front: insert the chosen cards there
        // in order (first chosen ends up deepest).
        for (offset, card) in bottomed.into_iter().enumerate() {
            player.library.insert(offset, card);
        }
    }
    if let Some(decision) = state
        .mulligan
        .as_mut()
        .and_then(|m| m.decisions.get_mut(seat.0))
    {
        decision.kept = true;
    }
    state.record_event(GameEvent::HandKept { player: seat });
    advance_after_keep(state);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply::test_support::*;

    #[test]
    fn issue_116_cleanup_discards_down_to_max_hand_size_via_a_choice() {
        // CR 514.1: with more than the maximum hand size, the active player
        // discards down to it during cleanup. CR 514.3: no priority is granted, so
        // the only thing offered is the discard — a select-from-zone choice, one
        // Discard per card in hand, never an automatic discard.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1.
        let hand: Vec<CardInstance> = (0..9)
            .map(|_| state.new_instance(fixture("onakke_ogre")))
            .collect();
        state.players[0].hand = hand.clone();

        // Ending the turn walks into cleanup and stops for the discard.
        let at_cleanup = pass_full_round(&state, &db);
        assert_eq!(at_cleanup.step, Step::Cleanup);
        assert_eq!(at_cleanup.active_player, PlayerId(0));
        assert_eq!(at_cleanup.priority, PlayerId(0));
        let choices = valid_actions(&at_cleanup, &db);
        assert!(
            !choices.contains(&Action::PassPriority),
            "cleanup grants no priority (CR 514.3)"
        );
        let discards = choices
            .iter()
            .filter(|a| matches!(a, Action::Discard { .. }))
            .count();
        assert_eq!(discards, 9, "one discard choice per card in hand");
        // Concede is still offered during cleanup (CR 104.3a); nothing else is.
        assert!(choices.contains(&Action::Concede));
        assert_eq!(choices.len(), 10, "the nine discards plus concede");

        // Discard two specific cards; the second brings the hand to the maximum,
        // so cleanup completes and the turn advances to player 1.
        let s = apply_action(&at_cleanup, &Action::Discard { card: hand[0] }, &db);
        assert_eq!(
            s.step,
            Step::Cleanup,
            "still over the limit after one discard"
        );
        assert_eq!(s.players[0].hand.len(), 8);
        let s = apply_action(&s, &Action::Discard { card: hand[1] }, &db);

        assert_eq!(
            s.players[0].hand.len(),
            MAX_HAND_SIZE,
            "discarded to the max (CR 514.1)"
        );
        assert_eq!(s.players[0].graveyard.len(), 2);
        assert!(s.players[0].graveyard.contains(&hand[0]));
        assert!(s.players[0].graveyard.contains(&hand[1]));
        // Cleanup finished with no priority granted; the next turn has begun.
        assert_eq!(s.turn, 2);
        assert_eq!(s.active_player, PlayerId(1));
        assert_eq!(s.step, Step::Upkeep);
    }
}
