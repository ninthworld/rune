use super::*;
use crate::id::{CardInstance, CardInstanceId};
use crate::player::LossReason;

/// CR 104.3a: the priority holder concedes — they leave the game and lose
/// immediately, at any time they could act. Modeled by marking the conceding seat
/// as having lost with [`LossReason::Concede`]; the state-based-actions loop then
/// settles and [`GameState::result`] derives the winner (CR 104.2a).
pub(crate) fn apply_concede(state: &mut GameState) {
    let seat = state.priority;
    if let Some(player) = state.players.get_mut(seat.0) {
        player.has_lost = true;
        player.loss_reason.get_or_insert(LossReason::Concede);
    }
}

/// CR 903.9a (accept): move the priority holder's commander from the graveyard or
/// exile it went to into their command zone, as a fresh object, and log the move.
///
/// The card is found by its stable instance id in whichever of the two zones holds
/// it, removed, and pushed to the command zone; the pending return decision is
/// cleared. Its [`crate::CardInstanceId`] carries over unchanged (as it does for
/// any move between non-battlefield zones), so the commander designation keeps
/// tracking it; a later recast mints a fresh [`crate::PermanentId`] on battlefield
/// entry, which is where "fresh object" is observable. Only ever reached for a
/// commander whose owner holds priority with a pending return (see
/// [`crate::valid_actions`]); a no-op if the card cannot be found.
pub(crate) fn apply_return_commander(state: &mut GameState, card: CardInstance) {
    let owner = state.priority;
    let Some(player) = state.players.get_mut(owner.0) else {
        return;
    };
    // Take the commander out of the graveyard or exile it currently sits in.
    let removed = remove_instance(&mut player.graveyard, card.id)
        .or_else(|| remove_instance(&mut player.exile, card.id));
    let Some(instance) = removed else {
        return;
    };
    player.command.push(instance);
    if let Some(commander) = player.commander.as_mut() {
        commander.return_pending = false;
    }
    state.record_event(GameEvent::CommanderReturnedToCommandZone {
        player: owner,
        card: instance,
    });
}

/// CR 903.9a (decline): leave the commander where it went and clear the pending
/// return decision, so [`crate::valid_actions`] stops offering the choice and
/// normal play resumes. Records nothing — the card did not move.
pub(crate) fn apply_decline_commander_return(state: &mut GameState, _card: CardInstance) {
    let owner = state.priority;
    if let Some(commander) = state
        .players
        .get_mut(owner.0)
        .and_then(|p| p.commander.as_mut())
    {
        commander.return_pending = false;
    }
}

/// Remove and return the first [`CardInstance`] in `pile` with instance id `id`,
/// or `None` if absent. Preserves the order of the remaining cards.
fn remove_instance(pile: &mut Vec<CardInstance>, id: CardInstanceId) -> Option<CardInstance> {
    let pos = pile.iter().position(|c| c.id == id)?;
    Some(pile.remove(pos))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply::test_support::*;

    #[test]
    fn issue_119_concede_ends_the_game_with_the_opponent_as_winner_cr_104_3a() {
        // CR 104.3a: conceding makes the conceding player lose; CR 104.2a: the
        // remaining player wins.
        let db = db();
        let state = GameState::new_two_player(); // seat 0 holds priority.
        assert!(valid_actions(&state, &db).contains(&Action::Concede));

        let after = apply_action(&state, &Action::Concede, &db);
        assert!(after.players[0].has_lost);
        assert_eq!(after.players[0].loss_reason, Some(LossReason::Concede));
        let result = after.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(1)));
        assert_eq!(result.losers, vec![PlayerId(0)]);
        assert_eq!(result.reason, LossReason::Concede);
    }

    #[test]
    fn cr_903_10a_twenty_one_commander_damage_across_combats_loses_at_positive_life() {
        // CR 903.10a: 21 combat damage from one commander, accumulated across
        // several combats, loses the game — even while the player is at a healthy
        // positive life total (proving this is not the CR 704.5a life loss). A
        // 7-power commander hits a 100-life player three times: 21 total, life 79.
        let db = commander_db();
        let mut state = GameState::new_two_player();
        state.players[1].life = 100;
        let (_cmd, _inst) = place_commander_attacker(
            &mut state,
            id_in(&db, "test_general"),
            PlayerId(0),
            PlayerId(1),
        );

        for _ in 0..3 {
            deal_combat_damage(&mut state, &db);
        }
        assert_eq!(
            state.commander_damage_taken(PlayerId(1), PlayerId(0)),
            21,
            "the per-commander tally accumulated across the three combats"
        );
        assert_eq!(
            state.players[1].life, 79,
            "the player is still at positive life"
        );

        run_state_based_actions(&mut state, &db);
        assert!(
            state.players[1].has_lost,
            "21 commander damage loses even at positive life (CR 903.10a)"
        );
        assert_eq!(
            state.players[1].loss_reason,
            Some(LossReason::CommanderDamage),
            "the loss is attributed to commander damage, not life loss"
        );
    }

    #[test]
    fn cr_903_10a_damage_from_two_different_commanders_does_not_pool() {
        // CR 903.10a scopes the 21 to "any one commander": 11 from one commander and
        // 10 from a different one is 21 total but not lethal, because the two never
        // pool. Seats 0 and 2 each have a commander attacking seat 1 (life 100).
        let db = commander_db();
        let mut state = GameState::new_multiplayer(3);
        state.players[1].life = 100;
        place_commander_attacker(
            &mut state,
            id_in(&db, "test_captain"),
            PlayerId(0),
            PlayerId(1),
        ); // 11
        place_commander_attacker(
            &mut state,
            id_in(&db, "test_marshal"),
            PlayerId(2),
            PlayerId(1),
        ); // 10

        deal_combat_damage(&mut state, &db);
        assert_eq!(state.commander_damage_taken(PlayerId(1), PlayerId(0)), 11);
        assert_eq!(state.commander_damage_taken(PlayerId(1), PlayerId(2)), 10);

        run_state_based_actions(&mut state, &db);
        assert!(
            !state.players[1].has_lost,
            "11 + 10 from two different commanders does not pool to a loss (CR 903.10a)"
        );
    }

    #[test]
    fn cr_903_10a_non_combat_damage_from_a_commander_does_not_count() {
        // CR 903.10a counts only *combat* damage. Damage a commander deals a player
        // outside combat (here the non-combat life-loss seam) never touches the
        // commander-damage tally, so it can never contribute to the 21 loss.
        let db = commander_db();
        let mut state = GameState::new_two_player();
        let (_cmd, _inst) = place_commander_attacker(
            &mut state,
            id_in(&db, "test_warlord"),
            PlayerId(0),
            PlayerId(1),
        );

        // Non-combat damage (e.g. a burn spell the commander is the source of) uses
        // the plain player-damage seam, which never feeds the tally.
        state.deal_damage_to_player(PlayerId(1), 21);
        assert!(
            state.commander_damage.is_empty(),
            "non-combat damage from a commander does not count (CR 903.10a)"
        );
        run_state_based_actions(&mut state, &db);
        assert_eq!(
            state.players[1].loss_reason,
            Some(LossReason::ZeroLife),
            "the 21 non-combat damage is ordinary life loss, not a commander-damage loss"
        );
    }

    #[test]
    fn cr_903_10a_tally_persists_across_the_commanders_zone_changes_and_recast() {
        // CR 903.10a "the same commander": the tally follows the designation, not any
        // battlefield object. A commander deals 7, leaves the battlefield, and is
        // recast as a brand-new permanent (fresh `PermanentId`, same instance); its
        // next 7 accrues onto the *same* key, proving the fresh id did not reset it.
        let db = commander_db();
        let mut state = GameState::new_two_player();
        state.players[1].life = 100;
        let (first, instance) = place_commander_attacker(
            &mut state,
            id_in(&db, "test_general"),
            PlayerId(0),
            PlayerId(1),
        );

        deal_combat_damage(&mut state, &db);
        assert_eq!(state.commander_damage_taken(PlayerId(1), PlayerId(0)), 7);

        // The commander leaves the battlefield (a zone change), then re-enters as a
        // new object with a fresh `PermanentId` but the same physical instance.
        state.battlefield.retain(|p| p.id != first);
        let second = place_commander_permanent(
            &mut state,
            id_in(&db, "test_general"),
            instance,
            PlayerId(0),
            PlayerId(1),
        );
        assert_ne!(first, second, "the recast commander is a fresh PermanentId");

        deal_combat_damage(&mut state, &db);
        assert_eq!(
            state.commander_damage_taken(PlayerId(1), PlayerId(0)),
            14,
            "the tally survived the zone change and recast (keyed to the designation)"
        );
    }

    #[test]
    fn cr_903_10a_multiplayer_commander_damage_loss_eliminates_via_cr_800_4a() {
        // CR 903.10a + CR 800.4a: in a game of three, a player dealt 21 commander
        // damage loses and *leaves the game* through the existing elimination
        // lifecycle — their objects are removed, the departure is logged, and the
        // game continues with the two other seats.
        let db = commander_db();
        let mut state = GameState::new_multiplayer(3);
        state.players[1].life = 100;
        let victim_perm =
            place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        place_commander_attacker(
            &mut state,
            id_in(&db, "test_warlord"),
            PlayerId(0),
            PlayerId(1),
        ); // 21

        deal_combat_damage(&mut state, &db);
        run_state_based_actions(&mut state, &db);

        assert!(
            state.players[1].has_lost && state.players[1].left_game,
            "the player lost to commander damage and left the game (CR 800.4a)"
        );
        assert_eq!(
            state.players[1].loss_reason,
            Some(LossReason::CommanderDamage)
        );
        assert!(
            !state.battlefield.iter().any(|p| p.id == victim_perm),
            "the eliminated player's permanents leave the game"
        );
        assert!(
            state.log.iter().any(
                |e| matches!(e.event, GameEvent::PlayerEliminated { player, reason }
                    if player == PlayerId(1) && reason == LossReason::CommanderDamage)
            ),
            "the elimination is logged with the commander-damage reason"
        );
        assert!(
            state.result().is_none(),
            "the game continues — two players remain (CR 800.4a)"
        );
    }
}
