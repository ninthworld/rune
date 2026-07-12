//! The London mulligan (CR 103.5) as a pure pre-game decision phase.
//!
//! After opening hands are dealt (CR 103.5, see [`crate::setup`]) the game does
//! **not** begin turn 1: it first runs the London mulligan. Each player, in turn,
//! is offered a keep-or-mulligan decision through [`crate::valid_actions`]:
//!
//! - **Mulligan** ([`Action::Mulligan`](crate::Action::Mulligan)) shuffles the
//!   hand back into the library and draws a fresh hand of the opening size, then
//!   the same player decides again. The count of mulligans a player has taken is
//!   tracked here (CR 103.5).
//! - **Keep** ([`Action::Keep`](crate::Action::Keep)) finalizes the hand. Under
//!   the London rule a player who has taken `N` mulligans must put `N` cards from
//!   hand on the **bottom** of their library; those cards are chosen through a
//!   single multi-select requirement slot ([`bottom_requirement`]) whose
//!   candidates are the hand's cards. Keeping a first hand (`N == 0`) bottoms
//!   nothing.
//!
//! Turn 1 begins only once **every** player has kept (CR 103.5): while any player
//! is still deciding, [`crate::valid_actions`] offers nothing but that player's
//! keep/mulligan decision, and the turn structure does not advance.
//!
//! This module is pure data and pure functions over [`GameState`]: the only
//! randomness (the mulligan reshuffle) draws from
//! [`GameState::rng_seed`](crate::GameState::rng_seed) exactly like the opening
//! shuffle, so a game with mulligans still replays identically from its seed. The
//! decision phase is driven entirely by `valid_actions`/`apply_action`; it holds
//! no I/O, timers, or observers.

use crate::ability::Target;
use crate::actions::Action;
use crate::id::{CardInstanceId, PlayerId};
use crate::state::GameState;

/// One seat's progress through the [London mulligan](self) (CR 103.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerMulligan {
    /// How many mulligans this seat has taken so far. Under the London rule this
    /// is exactly how many cards the seat must put on the bottom of its library
    /// when it keeps (CR 103.5).
    pub taken: usize,
    /// Whether this seat has finalized its hand (kept, and bottomed any owed
    /// cards). A kept seat makes no further mulligan decisions.
    pub kept: bool,
}

/// The pre-game [London mulligan](self) decision phase (CR 103.5).
///
/// Present on [`GameState::mulligan`] while the phase is in progress and cleared
/// to `None` the instant every seat has kept, at which point turn 1 begins. Plain
/// `Clone`/`Eq` value data, so [`GameState`] keeps its value semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MulliganState {
    /// Per-seat progress, indexed in parallel with
    /// [`GameState::players`](crate::GameState::players).
    pub decisions: Vec<PlayerMulligan>,
    /// The opening-hand size a mulligan redraws to (CR 103.5) — the same
    /// `starting_hand_size` the game was set up with.
    pub hand_size: usize,
}

impl MulliganState {
    /// A fresh mulligan phase for `seats` players redrawing to `hand_size`: no one
    /// has mulliganed or kept yet.
    #[must_use]
    pub fn new(seats: usize, hand_size: usize) -> Self {
        Self {
            decisions: vec![PlayerMulligan::default(); seats],
            hand_size,
        }
    }

    /// Whether every seat has kept, so the phase is finished and turn 1 may begin
    /// (CR 103.5). Vacuously `true` for a seatless phase.
    #[must_use]
    pub fn all_kept(&self) -> bool {
        self.decisions.iter().all(|d| d.kept)
    }
}

/// A mulligan bottoming requirement: how many cards a keeping player must put on
/// the bottom of their library (CR 103.5, London) and the candidate hand cards
/// they may choose among.
///
/// This is the single **multi-select requirement slot** the London bottoming maps
/// onto: [`count`](Self::count) cards chosen from [`candidates`](Self::candidates),
/// each a [`Target::Card`] naming a specific hand card by its per-instance
/// identity. It is exactly the `requirements` slot with a multi-select `chosen`
/// that `docs/protocol.md` already reserves — no new protocol message type is
/// needed to carry it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BottomRequirement {
    /// Exactly how many cards must be chosen: the number of mulligans the keeping
    /// player has taken, capped at their current hand size (CR 103.5).
    pub count: usize,
    /// The cards the player may choose to bottom — one [`Target::Card`] per card
    /// in their hand, in hand order.
    pub candidates: Vec<Target>,
}

/// The mulligan actions offered to the seat currently deciding, or `None` when
/// `state` is not in a mulligan phase that is waiting on this seat.
///
/// While the [phase](MulliganState) is unfinished the deciding seat is the
/// priority holder (see [`crate::valid_actions`]); it is offered exactly a
/// [`Mulligan`](Action::Mulligan) and a [`Keep`](Action::Keep) in its *requirement
/// form* (an empty bottom selection — the concrete cards to bottom are filled in
/// from [`bottom_requirement`]). A seat that has already kept is offered nothing,
/// and neither is any seat once every player has kept (CR 103.5).
#[must_use]
pub(crate) fn mulligan_actions(state: &GameState) -> Option<Vec<Action>> {
    let mull = state.mulligan.as_ref()?;
    if mull.all_kept() {
        // The phase is over; normal play resumes. (Reached only transiently — the
        // phase is cleared to `None` as the last keep is applied.)
        return Some(Vec::new());
    }
    let seat = state.priority;
    match mull.decisions.get(seat.0) {
        Some(decision) if !decision.kept => {
            Some(vec![Action::Mulligan, Action::Keep { bottom: Vec::new() }])
        }
        // The priority seat has already kept (or is out of range): nothing to do
        // here until priority reaches a still-deciding seat.
        _ => Some(Vec::new()),
    }
}

/// How many cards seat `seat` must bottom if it keeps now (CR 103.5): the number
/// of mulligans it has taken, capped at its current hand size. `0` when it has not
/// mulliganed, or when `state` is not in a mulligan phase.
#[must_use]
pub(crate) fn bottom_count(state: &GameState, seat: PlayerId) -> usize {
    let Some(mull) = state.mulligan.as_ref() else {
        return 0;
    };
    let taken = mull.decisions.get(seat.0).map_or(0, |d| d.taken);
    let hand = state.players.get(seat.0).map_or(0, |p| p.hand.len());
    taken.min(hand)
}

/// The bottoming requirement for `action` against `state`, or `None` when there is
/// nothing to choose.
///
/// Returns `Some` only for an [`Action::Keep`] taken by a player who owes a
/// bottoming (CR 103.5) — i.e. one who has taken at least one mulligan and still
/// holds cards. A first-hand keep (`count == 0`) and every non-keep action return
/// `None`, so the caller offers a plain, choice-free action.
#[must_use]
pub fn bottom_requirement(state: &GameState, action: &Action) -> Option<BottomRequirement> {
    if !matches!(action, Action::Keep { .. }) {
        return None;
    }
    let seat = state.priority;
    let count = bottom_count(state, seat);
    if count == 0 {
        return None;
    }
    let candidates = state
        .players
        .get(seat.0)?
        .hand
        .iter()
        .map(|inst| Target::Card(inst.id))
        .collect();
    Some(BottomRequirement { count, candidates })
}

/// Whether the `bottom` selection carried by an [`Action::Keep`] is a legal answer
/// to the deciding seat's [`bottom_requirement`] (CR 103.5).
///
/// Legal exactly when the selection names precisely [`bottom_count`] cards — no
/// more, no fewer — each a distinct [`Target::Card`] currently in the deciding
/// seat's hand. A first-hand keep is legal only with an empty selection.
#[must_use]
pub(crate) fn keep_bottom_is_legal(state: &GameState, bottom: &[Target]) -> bool {
    let seat = state.priority;
    let required = bottom_count(state, seat);
    if bottom.len() != required {
        return false;
    }
    let Some(player) = state.players.get(seat.0) else {
        return required == 0;
    };
    let mut seen: Vec<CardInstanceId> = Vec::with_capacity(bottom.len());
    for target in bottom {
        let Target::Card(id) = target else {
            // Only hand cards may be bottomed; a permanent/player target is never
            // a legal bottoming choice.
            return false;
        };
        if seen.contains(id) {
            return false; // no card may be bottomed twice
        }
        if !player.hand.iter().any(|inst| inst.id == *id) {
            return false; // the card must be in the deciding seat's hand
        }
        seen.push(*id);
    }
    true
}

/// Advance the mulligan phase after seat `state.priority` has just kept: hand the
/// decision to the next seat that has not kept, or, if every seat has now kept,
/// end the phase and begin turn 1.
///
/// Ending the phase clears [`GameState::mulligan`] to `None` and seats priority
/// with the active player at the current (untap) step, which is exactly the
/// normal turn-1 starting configuration (CR 103.5 — the game begins only once all
/// hands are kept). Pure; a no-op on a state with no mulligan phase.
pub(crate) fn advance_after_keep(state: &mut GameState) {
    let seats = state.players.len();
    let Some(mull) = state.mulligan.as_ref() else {
        return;
    };
    if mull.all_kept() || seats == 0 {
        state.mulligan = None;
        state.priority = state.active_player;
        state.consecutive_passes = 0;
        return;
    }
    // Find the next still-deciding seat in seating order after the current one.
    for offset in 1..=seats {
        let candidate = (state.priority.0 + offset) % seats;
        if mull.decisions.get(candidate).is_some_and(|d| !d.kept) {
            state.priority = PlayerId(candidate);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::apply_action;
    use crate::card::CardDatabase;
    use crate::fixtures::fixture;
    use crate::id::CardId;
    use crate::phase::Step;
    use crate::setup::GameSetup;
    use crate::valid_actions;

    /// The bundled card database, for tests that build a real game from a setup.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A 40-card decklist cycling through six bundled cards.
    ///
    /// Named by authored identity, not by handle: a `CardId` is interned from the
    /// catalog's sort order (ADR 0018 §3), so `CardId(1)` means a different card the
    /// moment one is authored ahead of it.
    fn sample_decklist() -> Vec<CardId> {
        const CARDS: [&str; 6] = [
            "thornback_boar",
            "riverbank_otter",
            "emberfang_jackal",
            "stonehide_basilisk",
            "forest",
            "verdant_scout",
        ];
        (0..40).map(|i| fixture(CARDS[i % 6])).collect()
    }

    /// A fresh two-player game (seed `seed`) sitting in the opening London
    /// mulligan phase (CR 103.5): opening hands dealt, seat 0 deciding first.
    fn opening(seed: u64) -> GameState {
        GameState::new(
            &GameSetup::two_player(sample_decklist(), sample_decklist(), seed),
            &db(),
        )
        .unwrap()
    }

    /// The `Keep` requirement form the generator advertises (empty bottom).
    fn keep() -> Action {
        Action::Keep { bottom: Vec::new() }
    }

    #[test]
    fn cr_103_5_opening_hand_offers_keep_and_mulligan() {
        // CR 103.5: after opening hands are dealt the deciding player is offered a
        // keep-or-mulligan decision — and nothing else (no lands/passes: turn 1
        // has not begun).
        let state = opening(1);
        assert!(
            state.mulligan.is_some(),
            "the game opens in the mulligan phase"
        );
        assert_eq!(state.turn, 1);
        assert_eq!(state.step, Step::Untap);

        let actions = valid_actions(&state, &db());
        // Keep/mulligan for the deciding seat, plus the always-available concede
        // (CR 104.3a) — a player may leave even during the mulligan.
        assert_eq!(actions, vec![Action::Mulligan, keep(), Action::Concede]);
        assert!(
            !actions.contains(&Action::PassPriority),
            "turn 1 has not begun"
        );
    }

    #[test]
    fn cr_103_5_the_decision_passes_seat_by_seat() {
        // The mulligan decision is offered to one seat at a time (the priority
        // holder), in seating order: seat 0 first, then — once it keeps — seat 1.
        let db = db();
        let state = opening(2);
        assert_eq!(state.priority, PlayerId(0));

        let after = apply_action(&state, &keep(), &db);
        assert_eq!(after.priority, PlayerId(1), "the decision passed to seat 1");
        assert_eq!(
            valid_actions(&after, &db),
            vec![Action::Mulligan, keep(), Action::Concede]
        );
    }

    #[test]
    fn cr_103_5_mulligan_redraws_a_full_hand_and_counts_it() {
        // CR 103.5 (London): a mulligan shuffles the hand into the library and
        // draws a fresh hand of the opening size; the count of mulligans taken is
        // tracked, and the same player decides again.
        let db = db();
        let state = opening(3);
        let lib_before = state.players[0].library.len();
        let hand_before = state.players[0].hand.len();

        let after = apply_action(&state, &Action::Mulligan, &db);
        // Hand is refilled to the opening size; the library is back to full size.
        assert_eq!(after.players[0].hand.len(), hand_before);
        assert_eq!(
            after.players[0].library.len(),
            lib_before,
            "the whole hand went back and a fresh hand was drawn",
        );
        // The mulligan is recorded and the same seat decides again.
        assert_eq!(after.mulligan.as_ref().unwrap().decisions[0].taken, 1);
        assert!(!after.mulligan.as_ref().unwrap().decisions[0].kept);
        assert_eq!(after.priority, PlayerId(0));
        assert_eq!(
            valid_actions(&after, &db),
            vec![Action::Mulligan, keep(), Action::Concede]
        );
    }

    #[test]
    fn cr_103_5_mulligan_reshuffle_is_seed_deterministic() {
        // The reshuffle draws only from the injected seed, so the same seed yields
        // the same post-mulligan hand — the game stays replayable (CR 103.3 style).
        let db = db();
        let a = apply_action(&opening(99), &Action::Mulligan, &db);
        let b = apply_action(&opening(99), &Action::Mulligan, &db);
        assert_eq!(a, b);
    }

    #[test]
    fn cr_103_5_nth_mulligan_requires_bottoming_n_cards() {
        // CR 103.5 (London): a player who has taken N mulligans must bottom N
        // cards when they keep. After two mulligans the keep advertises a single
        // multi-select requirement slot asking for two of the hand's cards.
        let db = db();
        let state = opening(4);
        let state = apply_action(&state, &Action::Mulligan, &db);
        let state = apply_action(&state, &Action::Mulligan, &db);
        assert_eq!(state.mulligan.as_ref().unwrap().decisions[0].taken, 2);

        let req = bottom_requirement(&state, &keep()).expect("two mulligans owe a bottoming");
        assert_eq!(req.count, 2, "bottom one card per mulligan taken");
        // Candidates are exactly the hand cards, as `Target::Card`.
        assert_eq!(req.candidates.len(), state.players[0].hand.len());
        for inst in &state.players[0].hand {
            assert!(req.candidates.contains(&Target::Card(inst.id)));
        }
    }

    #[test]
    fn cr_103_5_first_hand_keep_bottoms_nothing() {
        // Keeping the opening hand (no mulligans) requires no bottoming: the keep
        // is a plain, choice-free action.
        let state = opening(5);
        assert_eq!(bottom_requirement(&state, &keep()), None);
        assert!(keep_bottom_is_legal(&state, &[]));
    }

    #[test]
    fn cr_103_5_keep_puts_chosen_cards_on_the_bottom_of_the_library() {
        // CR 103.5 (London): the chosen cards go to the bottom of the library, and
        // the kept hand shrinks by that many. The bottom of the library is the
        // front of the vec (its top is the last element).
        let db = db();
        let state = apply_action(&opening(6), &Action::Mulligan, &db); // one mulligan
        let chosen = state.players[0].hand[0];
        let hand_before = state.players[0].hand.len();
        let lib_before = state.players[0].library.len();

        let kept = apply_action(
            &state,
            &Action::Keep {
                bottom: vec![Target::Card(chosen.id)],
            },
            &db,
        );

        // The chosen card left the hand for the bottom of the library.
        assert!(!kept.players[0].hand.iter().any(|c| c.id == chosen.id));
        assert_eq!(kept.players[0].hand.len(), hand_before - 1);
        assert_eq!(kept.players[0].library.len(), lib_before + 1);
        assert_eq!(
            kept.players[0].library.first().copied(),
            Some(chosen),
            "the bottomed card sits at the bottom (front) of the library",
        );
    }

    #[test]
    fn cr_103_5_keep_with_wrong_bottom_count_is_a_no_op() {
        // A keep must bottom exactly N cards. After one mulligan, keeping with zero
        // or two chosen cards is rejected as a no-op (the state is unchanged).
        let db = db();
        let state = apply_action(&opening(7), &Action::Mulligan, &db);
        let hand = state.players[0].hand.clone();

        // Too few (zero when one is owed).
        let none = apply_action(&state, &keep(), &db);
        assert_eq!(none, state);

        // Too many (two when one is owed).
        let two = apply_action(
            &state,
            &Action::Keep {
                bottom: vec![Target::Card(hand[0].id), Target::Card(hand[1].id)],
            },
            &db,
        );
        assert_eq!(two, state);
    }

    #[test]
    fn cr_103_5_keep_bottoming_a_card_not_in_hand_is_a_no_op() {
        // A bottoming choice must name a card in the deciding seat's hand; a
        // duplicate or a foreign instance id is rejected.
        let db = db();
        let state = apply_action(&opening(8), &Action::Mulligan, &db);
        let foreign = Target::Card(CardInstanceId(999_999));
        let after = apply_action(
            &state,
            &Action::Keep {
                bottom: vec![foreign],
            },
            &db,
        );
        assert_eq!(after, state);

        // A duplicate of a single real card (owed two) is likewise illegal.
        let state2 = apply_action(&state, &Action::Mulligan, &db); // now owes two
        let dup = state2.players[0].hand[0];
        let after2 = apply_action(
            &state2,
            &Action::Keep {
                bottom: vec![Target::Card(dup.id), Target::Card(dup.id)],
            },
            &db,
        );
        assert_eq!(after2, state2, "the same card cannot be bottomed twice");
    }

    #[test]
    fn cr_103_5_turn_one_waits_until_all_players_keep() {
        // CR 103.5: turn 1 begins only once every player has kept. After seat 0
        // keeps, the phase is still in progress (seat 1 deciding); once seat 1
        // keeps too, the phase ends and normal turn-1 play resumes.
        let db = db();
        let state = opening(10);

        // Seat 0 keeps its first hand (no bottoming).
        let after0 = apply_action(&state, &keep(), &db);
        assert!(after0.mulligan.is_some(), "seat 1 has not decided yet");
        assert_eq!(after0.priority, PlayerId(1));
        assert!(after0.mulligan.as_ref().unwrap().decisions[0].kept);

        // Seat 1 keeps too: the mulligan phase ends and turn 1 begins.
        let started = apply_action(&after0, &keep(), &db);
        assert!(
            started.mulligan.is_none(),
            "all players kept — the game begins"
        );
        assert_eq!(started.turn, 1);
        assert_eq!(started.step, Step::Untap);
        assert_eq!(started.active_player, PlayerId(0));
        assert_eq!(started.priority, PlayerId(0));
        // Normal play is now offered (pass priority is available again).
        assert!(valid_actions(&started, &db).contains(&Action::PassPriority));
    }

    #[test]
    fn cr_103_5_a_player_may_mulligan_then_the_opponent_keeps_first_hand() {
        // A mix of decisions still resolves: seat 0 mulligans once then keeps
        // (bottoming one), seat 1 keeps its first hand; the game then begins.
        let db = db();
        let state = apply_action(&opening(11), &Action::Mulligan, &db);
        let bottom = Target::Card(state.players[0].hand[0].id);
        let seat0_kept = apply_action(
            &state,
            &Action::Keep {
                bottom: vec![bottom],
            },
            &db,
        );
        assert!(seat0_kept.mulligan.is_some());
        assert_eq!(seat0_kept.priority, PlayerId(1));

        let started = apply_action(&seat0_kept, &keep(), &db);
        assert!(started.mulligan.is_none());
        assert_eq!(started.mulligan, None);
        // Seat 0 kept a six-card hand (7 drawn, 1 bottomed).
        assert_eq!(started.players[0].hand.len(), 6);
        assert_eq!(started.players[1].hand.len(), 7);
    }

    #[test]
    fn cr_103_5_cannot_take_normal_actions_during_the_mulligan_phase() {
        // While the mulligan phase is in progress, non-mulligan actions are not on
        // offer and are rejected as no-ops (turn 1 has not begun).
        let db = db();
        let state = opening(12);
        let after = apply_action(&state, &Action::PassPriority, &db);
        assert_eq!(after, state, "passing priority is not legal mid-mulligan");
    }

    #[test]
    fn scaffold_state_is_not_in_a_mulligan_phase() {
        // The bare test scaffold starts a game already in progress, so it is not in
        // a mulligan phase and offers normal actions immediately.
        let state = GameState::new_two_player();
        assert!(state.mulligan.is_none());
        assert!(valid_actions(&state, &db()).contains(&Action::PassPriority));
    }

    #[test]
    fn all_kept_is_true_only_when_every_seat_has_kept() {
        let mut mull = MulliganState::new(2, 7);
        assert!(!mull.all_kept());
        mull.decisions[0].kept = true;
        assert!(!mull.all_kept());
        mull.decisions[1].kept = true;
        assert!(mull.all_kept());
    }
}
