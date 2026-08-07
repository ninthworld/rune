//! Game construction and initialization methods.

use crate::id::PlayerId;
use crate::phase::Step;
use crate::player::Player;

use super::GameState;

impl GameState {
    /// An initial two-player game: turn 1, player 0 to act, at the [`Step::Untap`]
    /// step of the first turn. Both players start with **empty** libraries and
    /// hands — this is the bare scaffold for tests and turn-structure code. To
    /// start a game from real decklists (shuffled libraries, opening hands drawn),
    /// use [`Self::new`] with a [`GameSetup`](crate::GameSetup).
    ///
    /// The RNG seed defaults to `0`; use [`Self::new_two_player_with_seed`] to
    /// inject an explicit seed. Defaulting here keeps existing call sites
    /// unchanged while reserving the deterministic-randomness slot.
    #[must_use]
    pub fn new_two_player() -> Self {
        Self::new_two_player_with_seed(0)
    }

    /// An initial two-player game seeded with `rng_seed`, otherwise identical to
    /// [`Self::new_two_player`]. The seed feeds all future engine randomness
    /// (e.g. shuffling); see [`Self::rng_seed`].
    #[must_use]
    pub fn new_two_player_with_seed(rng_seed: u64) -> Self {
        let mut state = Self {
            turn: 1,
            active_player: PlayerId(0),
            priority: PlayerId(0),
            consecutive_passes: 0,
            step: Step::Untap,
            players: vec![Player::new(), Player::new()],
            battlefield: Vec::new(),
            emblems: Vec::new(),
            graveyard_casting: Vec::new(),
            exile_playing: Vec::new(),
            ignoring_hexproof: Vec::new(),
            replacements: Vec::new(),
            prevention: Vec::new(),
            delayed_triggers: Vec::new(),
            reflexive_triggers: Vec::new(),
            stack: Vec::new(),
            static_effects: Vec::new(),
            next_object_id: 1,
            land_played: false,
            attackers_declared: false,
            blockers_declared: false,
            damage_orders: Vec::new(),
            loyalty_activations: Vec::new(),
            limited_activations: Vec::new(),
            exiled_until: Vec::new(),
            blockers_declared_by: Vec::new(),
            interrupted_priority: None,
            pending_choices: Vec::new(),
            deathtouch_struck: Vec::new(),
            commander_damage: Vec::new(),
            extra_turns: Vec::new(),
            extra_steps: Vec::new(),
            rng_seed,
            // The bare scaffold starts a game already in progress, past any
            // mulligan; the London mulligan phase is entered only by [`Self::new`]
            // from a real [`GameSetup`](crate::GameSetup).
            mulligan: None,
            log: Vec::new(),
            next_log_sequence: 1,
        };
        state.mark_active_turn_began();
        state
    }

    /// Record that the active player's most recent turn is the current one — the
    /// construction-time counterpart of what
    /// [`GameState::advance`](crate::GameState::advance) does at every later turn
    /// boundary, so [`Player::turn_began`] is never a lie about seat 0's first turn.
    pub(crate) fn mark_active_turn_began(&mut self) {
        let turn = self.turn;
        if let Some(player) = self.players.get_mut(self.active_player.0) {
            player.turn_began = turn;
        }
    }

    /// A bare in-progress scaffold with `seats` players (clamped to at least two),
    /// seeded with `rng_seed`; the multiplayer generalization of
    /// [`Self::new_two_player_with_seed`]. Turn and priority start on seat 0 and no
    /// combat is in progress, so it is a ready base for the engine's multiplayer
    /// combat and elimination tests (issues #341/#342/#344).
    #[must_use]
    pub fn new_multiplayer_with_seed(seats: usize, rng_seed: u64) -> Self {
        let mut state = Self {
            players: (0..seats.max(2)).map(|_| Player::new()).collect(),
            ..Self::new_two_player_with_seed(rng_seed)
        };
        state.mark_active_turn_began();
        state
    }

    /// A bare in-progress scaffold with `seats` players (clamped to at least two);
    /// the multiplayer counterpart of [`Self::new_two_player`].
    #[must_use]
    pub fn new_multiplayer(seats: usize) -> Self {
        Self::new_multiplayer_with_seed(seats, 0)
    }

    /// Mint a fresh, never-reused object id from the monotonic counter.
    ///
    /// Used when a permanent enters the battlefield or an object goes on the
    /// stack, so each gets a distinct identity.
    pub fn mint_id(&mut self) -> u64 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        id
    }
}
