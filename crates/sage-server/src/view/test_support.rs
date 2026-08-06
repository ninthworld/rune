//! Shared `#[cfg(test)]` fixtures for the view submodule tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::test_support::fixture;

/// Build the [`ChooseAction`] a well-behaved client sends for `action`:
/// echoing its id and content-binding token verbatim, with no targets (no
/// current engine action carries requirements).
pub(crate) fn answer(action: &ValidAction) -> ChooseAction {
    ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: Vec::new(),
        ..Default::default()
    }
}

/// A `PrecombatMain` two-player state with the given hand for seat 0, who holds
/// priority and can act at sorcery speed.
pub(crate) fn state_with_hand(cards: &[CardId]) -> (GameState, Vec<CardInstance>) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let hand: Vec<CardInstance> = cards.iter().map(|&c| state.new_instance(c)).collect();
    state.players[0].hand = hand.clone();
    (state, hand)
}

/// Put a creature (or any card) permanent onto the battlefield under
/// `controller`, returning its fresh [`PermanentId`]. `attacking`/`tapped` let a
/// caller stage a combat state directly.
pub(crate) fn put_permanent(
    state: &mut GameState,
    card: CardId,
    controller: PlayerId,
    tapped: bool,
    attacking: bool,
) -> PermanentId {
    let id = PermanentId(state.mint_id());
    // A real per-copy instance, minted the way a card entering the battlefield gets one,
    // so two permanents of the same card are two *physical cards* here as well as two
    // objects — which is what the `physical_card` projection (issue #650) reads.
    let instance = state.new_instance(card).id;
    state.battlefield.push(sage_engine::Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        tapped,
        // Entered a previous turn, so it is free of summoning sickness in a
        // turn > 0 combat state (CR 302.6).
        entered_turn: 0,
        // The bool param stages a two-player combat: attacking the sole
        // opponent, seat 1 (issue #341 made this the defending player).
        attacking: attacking.then_some(sage_engine::AttackTarget::Player(PlayerId(1))),
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    id
}

/// A `PrecombatMain`-agnostic cleanup state (CR 514.1) with the active player
/// over the maximum hand size, for the discard `select_from_zone` projection.
pub(crate) fn cleanup_over_hand_limit() -> (GameState, Vec<CardInstance>) {
    let mut state = GameState::new_two_player();
    state.step = Step::Cleanup;
    state.priority = state.active_player;
    // Nine cards in hand — over the seven-card maximum (CR 514.1), with room to
    // shed one and still be over the limit (used by the stale-token test).
    let hand: Vec<CardInstance> = (0..9)
        .map(|_| state.new_instance(fixture("forest")))
        .collect();
    state.players[state.active_player.0].hand = hand.clone();
    (state, hand)
}
