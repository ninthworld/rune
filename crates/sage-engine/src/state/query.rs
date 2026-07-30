//! Query and lookup methods for game state.

use crate::id::{CardId, CardInstanceId, PlayerId};
use crate::player::LossReason;

use super::{GameResult, GameState};

impl GameState {
    /// The owning player whose commander designation the physical card
    /// `instance` is, if any (CR 903.3). One player designates at most one
    /// commander today, so the designation is keyed to — and identified by — that
    /// owner's [`PlayerId`].
    ///
    /// This resolves an on-battlefield commander permanent to its stable
    /// designation key: a [`CardInstanceId`] never changes across zone moves,
    /// while a [`PermanentId`](crate::PermanentId) is minted fresh on every battlefield entry, so this
    /// is what lets the commander-damage tally follow "the same commander" (CR
    /// 903.10a) rather than resetting when the commander leaves and re-enters play.
    #[must_use]
    pub fn commander_owner_of(&self, instance: CardInstanceId) -> Option<PlayerId> {
        self.players
            .iter()
            .enumerate()
            .find_map(|(seat, player)| match player.commander {
                Some(commander) if commander.instance == instance => Some(PlayerId(seat)),
                _ => None,
            })
    }

    /// The cumulative combat damage the commander owned by `commander` has dealt
    /// `damaged` this game (CR 903.10a); `0` when none is recorded. A pure read of
    /// the stored [`Self::commander_damage`] tally — used by the view projection
    /// and the CR 903.10a loss check.
    #[must_use]
    pub fn commander_damage_taken(&self, damaged: PlayerId, commander: PlayerId) -> u32 {
        self.commander_damage
            .iter()
            .find(|entry| entry.commander == commander && entry.damaged == damaged)
            .map_or(0, |entry| entry.amount)
    }

    /// Mint a fresh [`CardInstance`](crate::id::CardInstance) for `card`, drawing a unique
    /// [`CardInstanceId`] from the monotonic counter.
    ///
    /// Called when a physical card first enters a game — deck loading (issue #9),
    /// token creation, or test setup — so every copy is individually addressable
    /// even when it shares a [`CardId`] with another.
    pub fn new_instance(&mut self, card: CardId) -> crate::id::CardInstance {
        crate::id::CardInstance {
            id: CardInstanceId(self.mint_id()),
            card,
        }
    }

    /// The player who currently holds priority, or `None` if [`Self::priority`]
    /// is out of range (as in the empty [`Default`] state).
    #[must_use]
    pub fn priority_holder(&self) -> Option<&crate::player::Player> {
        self.players.get(self.priority.0)
    }

    /// How many players are still in the game (CR 104.2a): those who have not lost.
    #[must_use]
    pub fn living_player_count(&self) -> usize {
        self.players.iter().filter(|p| !p.has_lost).count()
    }

    /// The next seat after `from` in seating order that is still in the game,
    /// wrapping around and skipping every eliminated seat (CR 800.4a — a player who
    /// has left takes no turns and receives no priority). Considers the other seats
    /// before `from` itself, so it returns `from` only when `from` is the sole
    /// survivor; `None` on a seatless state or when no seat is still in the game.
    #[must_use]
    pub fn next_living_seat(&self, from: PlayerId) -> Option<PlayerId> {
        let n = self.players.len();
        if n == 0 {
            return None;
        }
        (1..=n)
            .map(|offset| PlayerId((from.0 + offset) % n))
            .find(|seat| self.players.get(seat.0).is_some_and(|p| !p.has_lost))
    }

    /// The game's terminal result if it is over, else `None` (CR 104.2a).
    ///
    /// A game with at least two seats ends the moment at most one player has not
    /// lost: that survivor is the winner (CR 104.2a), or there is no winner when
    /// every player has lost (a draw, CR 104.4a). Derived fresh from the losers'
    /// stored [`has_lost`](crate::player::Player::has_lost)/[`loss_reason`](crate::player::Player::loss_reason);
    /// nothing terminal is cached on the state.
    #[must_use]
    pub fn result(&self) -> Option<GameResult> {
        // A game that has not seated at least two players cannot end this way.
        if self.players.len() < 2 {
            return None;
        }
        let losers: Vec<PlayerId> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| player.has_lost)
            .map(|(seat, _)| PlayerId(seat))
            .collect();
        let remaining = self.players.len() - losers.len();
        // The game is over only once someone has lost and at most one seat remains
        // (CR 104.2a). With every seat still in, there is no result yet.
        if losers.is_empty() || remaining > 1 {
            return None;
        }
        // One survivor wins (CR 104.2a); none survive → a draw (CR 104.4a).
        let winner = self
            .players
            .iter()
            .enumerate()
            .find(|(_, player)| !player.has_lost)
            .map(|(seat, _)| PlayerId(seat));
        // The deciding reason: with a winner there is exactly one loser, so its
        // reason is unambiguous; a draw takes the first loser's. `ZeroLife` is a
        // defensive fallback for an externally-constructed loser with no recorded
        // reason — the engine always records one alongside `has_lost`.
        let reason = losers
            .iter()
            .find_map(|seat| self.players[seat.0].loss_reason)
            .unwrap_or(LossReason::ZeroLife);
        Some(GameResult {
            winner,
            losers,
            reason,
        })
    }

    /// Whether the game has reached a terminal state (CR 104.2a). In a terminal
    /// state [`crate::valid_actions`] offers nothing and [`crate::apply_action`]
    /// rejects every action as a no-op.
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.result().is_some()
    }

    /// Borrow the active player, or `None` if [`Self::active_player`] is out of
    /// range (as it is in the empty [`Default`] state).
    #[must_use]
    pub fn active_player(&self) -> Option<&crate::player::Player> {
        self.players.get(self.active_player.0)
    }
}
