use crate::id::{PermanentId, PlayerId};
use crate::phase::Step;
use crate::state::GameState;

use super::eligibility::defending_player;

/// The players being attacked this combat, in APNAP order (CR 101.4) starting
/// from the active player — the distinct defenders named by the current
/// attackers. Each such player declares blockers for the attackers attacking
/// them (CR 509.1); the order they do so is this order.
///
/// A two-player combat yields at most the sole opponent. Deterministic and
/// reconstructable from state — no hidden iterator (issue #344).
#[must_use]
pub fn attacked_players(state: &GameState) -> Vec<PlayerId> {
    let n = state.players.len();
    if n == 0 {
        return Vec::new();
    }
    let mut ordered = Vec::new();
    for offset in 0..n {
        let seat = PlayerId((state.active_player.0 + offset) % n);
        let is_attacked = state
            .battlefield
            .iter()
            .any(|perm| perm.attacking == Some(seat));
        if is_attacked && !ordered.contains(&seat) {
            ordered.push(seat);
        }
    }
    ordered
}

/// The attacked player who owes the next declare-blockers decision, if any
/// (CR 509.1, APNAP-ordered per CR 101.4). `None` once every attacked player has
/// declared, or when there is nothing to declare.
///
/// Two-player games are unchanged: the sole opponent ([`defending_player`]) owes
/// the one declaration until [`GameState::blockers_declared`] is set — including
/// the empty declaration when no attackers were declared. With attackers split
/// across several defenders (issue #344), each attacked player still in the game
/// declares in turn; a defender already recorded in
/// [`GameState::blockers_declared_by`], or one who has been eliminated, is skipped.
#[must_use]
pub fn pending_blocker_declarer(state: &GameState) -> Option<PlayerId> {
    if state.blockers_declared {
        return None;
    }
    match defending_player(state) {
        // Two-player: the sole opponent owes the (possibly empty) declaration,
        // exactly as before.
        Some(sole) => Some(sole),
        // Multi-defender: the next attacked, still-in-the-game player who has not
        // yet declared, in APNAP order.
        None => attacked_players(state).into_iter().find(|seat| {
            !state.blockers_declared_by.contains(seat)
                && state.players.get(seat.0).is_some_and(|p| !p.has_lost)
        }),
    }
}

/// The attackers whose controller still owes a combat-damage assignment order
/// (CR 510.1, issue #346): those blocked by two or more creatures whose order has
/// not yet been chosen (they are absent from [`GameState::damage_orders`]). An
/// attacker with zero or one blocker has no choice to make and never appears here.
/// In stable battlefield order.
#[must_use]
pub fn attackers_needing_damage_order(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|p| p.attacking.is_some())
        .map(|p| p.id)
        .filter(|&atk| {
            blockers_of_unordered(state, atk).len() >= 2
                && !state.damage_orders.iter().any(|(a, _)| *a == atk)
        })
        .collect()
}

/// The attacker's blockers in bare battlefield order, ignoring any chosen order —
/// used to *count* blockers when deciding whether an ordering choice is owed
/// (independent of whether one has been made).
fn blockers_of_unordered(state: &GameState, attacker: PermanentId) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|p| p.blocking == Some(attacker))
        .map(|p| p.id)
        .collect()
}

/// The player who owes a combat-damage assignment order, if any (CR 510.1, issue
/// #346): the attacking (active) player, once every blocker declaration is in and at
/// least one attacker is multi-blocked without a chosen order. `None` once every
/// such attacker has been ordered, or when none is multi-blocked.
#[must_use]
pub fn pending_damage_order(state: &GameState) -> Option<PlayerId> {
    if state.blockers_declared && !attackers_needing_damage_order(state).is_empty() {
        Some(state.active_player)
    } else {
        None
    }
}

/// The player who owes a combat declaration in the current step, if any: the
/// active player during declare-attackers until attackers are declared
/// (CR 508.1), and, during declare-blockers, the next attacked player who owes a
/// blocker declaration (CR 509.1, APNAP-ordered — [`pending_blocker_declarer`]).
/// `None` in every other situation.
///
/// While a declaration is owed it is a turn-based *player choice*, so — like the
/// cleanup discard — only that player acts and the only action offered is the
/// declaration itself. Priority for the step's normal round is handed out only
/// once every owed declaration is made (see [`crate::apply_action`]).
#[must_use]
pub(crate) fn pending_declarer(state: &GameState) -> Option<PlayerId> {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Some(state.active_player),
        // Blockers first, then — once every attacked player has declared — the
        // attacking player's combat-damage assignment order for any multi-blocked
        // attacker (CR 510.1, issue #346).
        Step::DeclareBlockers => {
            pending_blocker_declarer(state).or_else(|| pending_damage_order(state))
        }
        _ => None,
    }
}

/// Who receives priority when the turn structure has just settled on a step: the
/// player owing that step's combat declaration if one is pending, otherwise the
/// active player (the ordinary case, CR 117.3a).
#[must_use]
pub(crate) fn priority_after_step_change(state: &GameState) -> PlayerId {
    pending_declarer(state).unwrap_or(state.active_player)
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::fixture;
    use crate::id::CardId;
    use crate::state::Permanent;

    /// Put an attacking creature of `card` under `controller` attacking
    /// `defender`; returns its id. Used by the multi-defender combat tests.
    pub(crate) fn attacker_of(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        defender: PlayerId,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card,
            controller,
            tapped: false,
            entered_turn: 0,
            attacking: Some(defender),
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    #[test]
    fn issue_344_attacked_players_are_in_apnap_order() {
        // CR 101.4: attacked players declare in APNAP order from the active player.
        // Seat 0 attacks seats 2 and 1 (declared in that order); APNAP order is 1
        // then 2, regardless of declaration order.
        let mut state = GameState::new_multiplayer(4);
        state.active_player = PlayerId(0);
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        assert_eq!(attacked_players(&state), vec![PlayerId(1), PlayerId(2)]);

        // Seat 3 is not attacked, so it is not in the list.
        assert!(!attacked_players(&state).contains(&PlayerId(3)));
    }

    #[test]
    fn issue_344_pending_declarer_walks_each_attacked_player_then_finishes() {
        let mut state = GameState::new_multiplayer(3);
        state.step = Step::DeclareBlockers;
        state.active_player = PlayerId(0);
        state.attackers_declared = true;
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));

        // Seat 1 owes the first declaration (APNAP), then seat 2, then none.
        assert_eq!(pending_blocker_declarer(&state), Some(PlayerId(1)));
        state.blockers_declared_by.push(PlayerId(1));
        assert_eq!(pending_blocker_declarer(&state), Some(PlayerId(2)));
        state.blockers_declared_by.push(PlayerId(2));
        assert_eq!(pending_blocker_declarer(&state), None);
    }

    #[test]
    fn issue_344_defender_eliminated_before_declaring_is_skipped() {
        // A defender eliminated in the declare window is skipped without stalling
        // combat: the next attacked player owes the declaration instead.
        let mut state = GameState::new_multiplayer(3);
        state.step = Step::DeclareBlockers;
        state.active_player = PlayerId(0);
        state.attackers_declared = true;
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));

        state.players[1].has_lost = true; // seat 1 dies in the declare window
        assert_eq!(
            pending_blocker_declarer(&state),
            Some(PlayerId(2)),
            "the eliminated defender is skipped; seat 2 declares"
        );
    }
}
