use crate::id::{PermanentId, PlayerId};
use crate::state::GameState;
use crate::CardDatabase;

use super::helpers::{has_keyword, has_summoning_sickness};
use crate::card::Keyword;

/// The players an attacker may legally be declared to attack (CR 508.1a): every
/// opponent still in the game — a seat other than the active (attacking) player
/// that has not lost. In seat order, so the enumeration is deterministic.
///
/// In a two-player game this is exactly the sole opponent, so the only legal
/// assignment for every attacker is that one player and combat plays as it always
/// has. With more seats each attacker chooses among these candidates (issue #341).
/// A player may never attack themselves or an eliminated player, so neither is a
/// candidate.
#[must_use]
pub fn defender_candidates(state: &GameState) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .enumerate()
        .filter(|(seat, player)| PlayerId(*seat) != state.active_player && !player.has_lost)
        .map(|(seat, _)| PlayerId(seat))
        .collect()
}

/// The single defending player of a two-player combat: the one opponent still in
/// the game (CR 508.1). `None` when there is not exactly one eligible defender —
/// on a state with fewer than two seats, or (once multiplayer combat lands) more
/// than one opponent, where there is no *single* defender and callers must consult
/// [`defender_candidates`] / each attacker's own [`crate::state::Permanent::attacking`]
/// target instead. This keeps every two-player code path (blocker declaration flow,
/// server view binding) working unchanged while the multi-defender flow (#344)
/// builds on the per-attacker targets.
#[must_use]
pub fn defending_player(state: &GameState) -> Option<PlayerId> {
    let candidates = defender_candidates(state);
    match candidates.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// The permanents the active player may legally declare as attackers right now
/// (CR 508.1a): creatures they control that are untapped and free of summoning
/// sickness (CR 302.6). In stable battlefield order.
///
/// This is the multi-select candidate set for the declare-attackers action — one
/// O(N) scan of the battlefield, never a product over selections. Haste (CR
/// 702.10b) exempts a creature from the summoning-sickness restriction; defender
/// and "can't attack" restrictions are not modeled yet.
#[must_use]
pub fn attacker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let active = state.active_player;
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.controller == active
                && super::helpers::is_creature(perm, db)
                && !perm.tapped
                // CR 302.6, with the CR 702.10b haste exemption: a hasty creature
                // ignores the summoning-sickness attack restriction.
                && (!has_summoning_sickness(perm, state)
                    || has_keyword(state, perm, Keyword::Haste, db))
        })
        .map(|perm| perm.id)
        .collect()
}

/// The permanents `defender` may legally declare as blockers right now
/// (CR 509.1a): untapped creatures they control (a tapped creature can't block).
/// In stable battlefield order.
///
/// This is the per-defender blocker candidate set: a player may block only with
/// their own creatures, and (enforced in the declaration's legality check, not
/// here) only against attackers attacking *them* (issue #341). The multi-defender
/// declaration flow (#344) calls this once per attacked player.
#[must_use]
pub fn blocker_candidates_for(
    state: &GameState,
    defender: PlayerId,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.controller == defender && super::helpers::is_creature(perm, db) && !perm.tapped
        })
        .map(|perm| perm.id)
        .collect()
}

/// The permanents the sole defending player of a two-player combat may legally
/// declare as blockers (CR 509.1a). Empty when there is no single defender (see
/// [`defending_player`]). A convenience over [`blocker_candidates_for`] for the
/// two-player declaration flow and server view binding; the multi-defender flow
/// (#344) uses [`blocker_candidates_for`] per attacked player.
#[must_use]
pub fn blocker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let Some(defender) = defending_player(state) else {
        return Vec::new();
    };
    blocker_candidates_for(state, defender, db)
}

/// The permanents currently declared as attackers, in stable battlefield order —
/// the legal set of creatures a blocker may be assigned to block (CR 509.1a).
#[must_use]
pub fn declared_attackers(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.attacking.is_some())
        .map(|perm| perm.id)
        .collect()
}

/// Whom the permanent `attacker` is attacking this combat (CR 508.1a), or `None`
/// if it is not on the battlefield or is not an attacker. This is the defending
/// player its combat damage routes to and the player whose creatures may block it.
#[must_use]
pub fn attacking_defender_of(state: &GameState, attacker: PermanentId) -> Option<PlayerId> {
    state
        .battlefield
        .iter()
        .find(|p| p.id == attacker)
        .and_then(|p| p.attacking)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::fixture;
    use crate::state::Permanent;

    /// Put a creature (Walking Corpse, a vanilla 2/2 with no combat keyword) on the
    /// battlefield under `controller` with the given tapped state, having entered on
    /// turn `entered_turn`.
    fn creature(
        state: &mut GameState,
        controller: PlayerId,
        tapped: bool,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(fixture("walking_corpse"));
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card: fixture("walking_corpse"),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    #[test]
    fn attacker_candidates_exclude_sick_and_tapped_creatures_cr_508_1a() {
        // CR 508.1a / 302.6: only the active player's untapped, non-sick creatures
        // are eligible attackers.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let eligible = creature(&mut state, PlayerId(0), false, 1);
        let _sick = creature(&mut state, PlayerId(0), false, 2);
        let _tapped = creature(&mut state, PlayerId(0), true, 1);
        let _opponents = creature(&mut state, PlayerId(1), false, 1);

        assert_eq!(attacker_candidates(&state, &db()), vec![eligible]);
    }

    #[test]
    fn blocker_candidates_exclude_tapped_creatures_cr_509_1a() {
        // CR 509.1a: a tapped creature can't block. Only the defender's untapped
        // creatures are eligible; summoning sickness does not stop blocking.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let eligible = creature(&mut state, PlayerId(1), false, 2); // sick but can block
        let _tapped = creature(&mut state, PlayerId(1), true, 1);
        let _attackers_creature = creature(&mut state, PlayerId(0), false, 1);

        assert_eq!(blocker_candidates(&state, &db()), vec![eligible]);
    }

    #[test]
    fn defender_is_the_sole_opponent() {
        let state = GameState::new_two_player();
        assert_eq!(defending_player(&state), Some(PlayerId(1)));
        assert_eq!(defending_player(&GameState::default()), None);
    }

    #[test]
    fn issue_341_defender_candidates_are_every_living_opponent_cr_508_1a() {
        // CR 508.1a: an attacker may be declared to attack any opponent still in the
        // game — never the active player, never an eliminated one.
        let mut state = GameState::new_multiplayer(3);
        state.active_player = PlayerId(0);
        assert_eq!(
            defender_candidates(&state),
            vec![PlayerId(1), PlayerId(2)],
            "both opponents of the active player are candidates"
        );
        // A two-player game has exactly one defender candidate — the sole opponent —
        // so `defending_player` resolves and combat plays as it always has.
        let two = GameState::new_two_player();
        assert_eq!(defender_candidates(&two), vec![PlayerId(1)]);
        assert_eq!(defending_player(&two), Some(PlayerId(1)));
        // With more than one opponent there is no single defender.
        assert_eq!(defending_player(&state), None);

        // An eliminated opponent drops out of the candidate set.
        state.players[1].has_lost = true;
        assert_eq!(defender_candidates(&state), vec![PlayerId(2)]);
    }

    #[test]
    fn issue_341_blocker_candidates_are_per_defender() {
        // Blocker candidates for a defending player include only that player's own
        // untapped creatures (issue #341); the per-attacker scoping is enforced in
        // the declaration's legality check.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let seat1_creature = creature(&mut state, PlayerId(1), false, 0);
        let seat2_creature = creature(&mut state, PlayerId(2), false, 0);

        assert_eq!(
            blocker_candidates_for(&state, PlayerId(1), &db),
            vec![seat1_creature]
        );
        assert_eq!(
            blocker_candidates_for(&state, PlayerId(2), &db),
            vec![seat2_creature]
        );
    }
}
