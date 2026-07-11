//! Combat declarations: eligibility of attackers and blockers, and the small
//! amount of turn-structure bookkeeping the declare steps need.
//!
//! Declaration only (issue #117): who *may* attack (CR 508.1a), who *may* block
//! (CR 509.1a), and which player owes the declaration in each declare step. It
//! stops short of combat damage (issue #118), first strike, and vigilance. Every
//! function here is a pure predicate/enumeration over an immutable
//! [`GameState`] — no I/O, no mutation — consistent with the engine's rules.

use crate::card_type::CardType;
use crate::id::{PermanentId, PlayerId};
use crate::phase::Step;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// The defending player this combat: in a two-player game, the one player who is
/// not the active player (CR 508.1 — the active player is the attacking player,
/// and this slice's single legal attack target is the sole opponent).
///
/// `None` on a state without an opponent to defend (fewer than two seats), so
/// callers never fabricate a defender.
#[must_use]
pub(crate) fn defending_player(state: &GameState) -> Option<PlayerId> {
    let seats = state.players.len();
    if seats < 2 {
        return None;
    }
    Some(PlayerId((state.active_player.0 + 1) % seats))
}

/// Whether `perm` has summoning sickness for its controller (CR 302.6): it has
/// **not** been under that player's control continuously since their most recent
/// turn began.
///
/// Derived from [`Permanent::entered_turn`]: a permanent that entered on an
/// earlier turn than the current one was already in play when this turn began, so
/// it is not sick; one that entered this turn is. Written for the active player,
/// whose most recent turn is the current [`GameState::turn`] — the only player
/// who declares attackers in this slice.
#[must_use]
pub(crate) fn has_summoning_sickness(perm: &Permanent, state: &GameState) -> bool {
    perm.entered_turn >= state.turn
}

/// Whether `perm` is a creature by its printed card types. Type-changing
/// continuous effects are future work, so the printed types are authoritative
/// here (as they are in [`crate::resolve::target_is_legal`]).
#[must_use]
fn is_creature(perm: &Permanent, db: &CardDatabase) -> bool {
    db.card(perm.card)
        .is_some_and(|c| c.has_type(CardType::Creature))
}

/// The permanents the active player may legally declare as attackers right now
/// (CR 508.1a): creatures they control that are untapped and free of summoning
/// sickness (CR 302.6). In stable battlefield order.
///
/// This is the multi-select candidate set for the declare-attackers action — one
/// O(N) scan of the battlefield, never a product over selections. Vigilance,
/// defender, and "can't attack" restrictions beyond summoning sickness are not
/// modeled yet.
#[must_use]
pub fn attacker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let active = state.active_player;
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.controller == active
                && is_creature(perm, db)
                && !perm.tapped
                && !has_summoning_sickness(perm, state)
        })
        .map(|perm| perm.id)
        .collect()
}

/// The permanents the defending player may legally declare as blockers right now
/// (CR 509.1a): untapped creatures they control (a tapped creature can't block).
/// In stable battlefield order. Empty when there is no defender.
///
/// This is the multi-select candidate set of *blockers* for the declare-blockers
/// action; the attacker each is assigned to comes from [`declared_attackers`].
#[must_use]
pub fn blocker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let Some(defender) = defending_player(state) else {
        return Vec::new();
    };
    state
        .battlefield
        .iter()
        .filter(|perm| perm.controller == defender && is_creature(perm, db) && !perm.tapped)
        .map(|perm| perm.id)
        .collect()
}

/// The permanents currently declared as attackers, in stable battlefield order —
/// the legal set of creatures a blocker may be assigned to block (CR 509.1a).
#[must_use]
pub fn declared_attackers(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.attacking)
        .map(|perm| perm.id)
        .collect()
}

/// The player who owes a combat declaration in the current step, if any: the
/// active player during declare-attackers until attackers are declared
/// (CR 508.1), and the defending player during declare-blockers until blockers
/// are declared (CR 509.1). `None` in every other situation.
///
/// While a declaration is owed it is a turn-based *player choice*, so — like the
/// cleanup discard — only that player acts and the only action offered is the
/// declaration itself. Priority for the step's normal round is handed out only
/// once the declaration is made (see [`crate::apply_action`]).
#[must_use]
pub(crate) fn pending_declarer(state: &GameState) -> Option<PlayerId> {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Some(state.active_player),
        Step::DeclareBlockers if !state.blockers_declared => defending_player(state),
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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::id::CardId;
    use crate::state::Permanent;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a creature (Verdant Scout, a 1/1) on the battlefield under `controller`
    /// with the given tapped state, having entered on turn `entered_turn`.
    fn creature(
        state: &mut GameState,
        controller: PlayerId,
        tapped: bool,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(CardId(6));
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card: CardId(6),
            controller,
            tapped,
            entered_turn,
            attacking: false,
            blocking: None,
            damage: 0,
            counters: Default::default(),
        });
        id
    }

    #[test]
    fn summoning_sickness_is_by_entry_turn_cr_302_6() {
        // CR 302.6: a creature that entered this turn is sick; one that entered a
        // previous turn is not.
        let mut state = GameState::new_two_player();
        state.turn = 3;
        let fresh = creature(&mut state, PlayerId(0), false, 3);
        let seasoned = creature(&mut state, PlayerId(0), false, 1);
        let fresh = state.battlefield.iter().find(|p| p.id == fresh).unwrap();
        let seasoned = state.battlefield.iter().find(|p| p.id == seasoned).unwrap();
        assert!(has_summoning_sickness(fresh, &state));
        assert!(!has_summoning_sickness(seasoned, &state));
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
}
