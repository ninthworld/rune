//! The root game state and the shared battlefield.
//!
//! ## Randomness invariant
//!
//! All randomness in the engine draws from [`GameState::rng_seed`] and nowhere
//! else — no `rand` crate, no wall-clock time, no thread-local or ambient
//! generator. The seed is injected through the constructors, so a game replays
//! identically from the same starting state. [`GameState::new`] consumes it to
//! shuffle opening libraries (CR 103.3) with a tiny inline generator (SplitMix64,
//! see [`crate::rng`] and `docs/decisions/0014-deterministic-seeded-shuffle.md`)
//! and stores the advanced generator state back into the slot, so later draws
//! continue the same stream. Concentrating every draw here is what makes the
//! `crates/rune-engine/AGENTS.md` rule "no randomness without an injected seed"
//! structurally satisfiable, rather than satisfied only by the absence of
//! randomness.

use std::collections::BTreeMap;

use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::mulligan::MulliganState;
use crate::phase::Step;
use crate::player::{LossReason, Player};
use crate::stack::StackObject;

/// The terminal outcome of a game (CR 104.2a / CR 104.4a), derived on demand from
/// player state — never stored on [`GameState`], in keeping with the engine's
/// "everything derivable is computed on demand" invariant.
///
/// Produced by [`GameState::result`] once at most one player remains: the sole
/// survivor is the winner (CR 104.2a), or there is no winner when every player has
/// lost simultaneously (a draw, CR 104.4a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameResult {
    /// The single remaining player who won (CR 104.2a), or `None` for a draw where
    /// no player remains (CR 104.4a).
    pub winner: Option<PlayerId>,
    /// Every player who has lost, in seat order (CR 104.3).
    pub losers: Vec<PlayerId>,
    /// Why the game ended: the loss reason of the deciding loser (the sole loser
    /// when there is a winner; the first loser in seat order for a draw).
    pub reason: LossReason,
}

/// A kind of counter that can sit on a [`Permanent`].
///
/// Only the power/toughness counters the layer system folds into computed
/// characteristics today are modeled (ADR 0010 slice 2, CR 613.7c). Other kinds
/// (loyalty, charge, …) are deferred until an effect needs them, at which point
/// a variant is added here. Used as a [`BTreeMap`] key in
/// [`Permanent::counters`], so ordering is derived and replay-stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CounterKind {
    /// A `+1/+1` counter: adds 1 to power and 1 to toughness (CR 122, CR 613.7c).
    PlusOnePlusOne,
    /// A `-1/-1` counter: subtracts 1 from power and 1 from toughness.
    MinusOneMinusOne,
}

/// A permanent on the shared battlefield.
///
/// Its [`PermanentId`] is minted fresh on battlefield entry and is distinct
/// from the [`CardId`] of the card it represents. It also links the
/// [`CardInstanceId`] of the physical card it originated from, so identity is
/// preserved when the permanent leaves the battlefield for another zone.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Permanent {
    /// Battlefield identity, fresh on entry.
    pub id: PermanentId,
    /// The physical card this permanent originated from. Stable across the zone
    /// change that put it here, unlike [`Self::id`].
    pub instance: CardInstanceId,
    /// The card this permanent represents.
    pub card: CardId,
    /// The player who currently controls it.
    pub controller: PlayerId,
    /// Whether the permanent is tapped.
    pub tapped: bool,
    /// The turn number on which this permanent entered the battlefield under its
    /// current controller (came under their control). Raw stored state, set on
    /// battlefield entry from [`GameState::turn`]; `0` for the empty [`Default`].
    ///
    /// This is the fact "summoning sickness" is derived from (CR 302.6): a
    /// creature has been controlled continuously since its controller's most
    /// recent turn began exactly when it entered on an *earlier* turn than the
    /// current one. The engine cannot derive that from a bare snapshot — it is
    /// history — so, like [`Self::damage`], it is stored rather than computed.
    /// Not a zone-change counter: a permanent re-entering the battlefield gets a
    /// fresh [`PermanentId`] and a fresh `entered_turn`; nothing counts entries.
    pub entered_turn: u32,
    /// Whether this permanent is currently attacking, i.e. it was declared as an
    /// attacker this combat and has not yet been removed from combat (CR 508.1,
    /// CR 511.3). Raw stored state, set when attackers are declared and cleared at
    /// the end-of-combat step. `false` for a permanent not in combat.
    pub attacking: bool,
    /// The attacker this permanent is blocking, if it was declared as a blocker
    /// this combat (CR 509.1); `None` for a permanent that is not blocking.
    ///
    /// A blocker is assigned to exactly one attacking creature (this field is that
    /// assignment); several blockers may name the same attacker. Raw stored state,
    /// set when blockers are declared and cleared at the end-of-combat step
    /// (CR 511.3).
    pub blocking: Option<PermanentId>,
    /// Damage marked on this permanent this turn (CR 120.3). Raw stored state,
    /// zeroed as a turn-based action during the cleanup step (CR 514.2) and,
    /// once combat lands (issue #118), compared against toughness by the
    /// state-based-actions loop (CR 704.5g). `0` means no marked damage.
    pub damage: u32,
    /// Counters on this permanent, keyed by [`CounterKind`] and mapped to how
    /// many of that kind are present.
    ///
    /// This is **raw stored state, not a derivation** (ADR 0010 §1): nothing
    /// else in [`GameState`] determines a permanent's counters, so the
    /// "no cached derivations" invariant does not apply to it. Current
    /// power/toughness *is* derived and folds these in on demand via
    /// [`characteristics`](crate::characteristics::characteristics); it is never
    /// stored. A kind absent from the map means zero of that counter; a present
    /// entry is a positive count.
    pub counters: BTreeMap<CounterKind, u32>,
}

impl Permanent {
    /// How many counters of `kind` are on this permanent, `0` when none are.
    #[must_use]
    pub fn counter_count(&self, kind: CounterKind) -> u32 {
        self.counters.get(&kind).copied().unwrap_or(0)
    }
}

/// A continuous static effect currently in force (ADR 0010 slice 3, §4).
///
/// This is **raw stored input, not a derivation** (ADR 0010 §1): the source
/// ability or permanent puts the effect here and its removal takes it away.
/// Nothing else in [`GameState`] determines it, so the "no cached derivations"
/// invariant does not apply to it — the same way [`Permanent::counters`] are
/// stored. A permanent's *current* power/toughness folds the applicable effects
/// in on demand via
/// [`characteristics`](crate::characteristics::characteristics) and is never
/// stored; removing an effect from [`GameState::static_effects`] therefore
/// reverts every affected permanent's computed value with nothing to invalidate.
///
/// This slice models only the layer-7c power/toughness modification an anthem or
/// pump performs; other layers slot in as new [`Modification`] variants behind
/// the same read path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticEffect {
    /// Object id of the source that put this effect into force — a permanent's
    /// [`PermanentId`](crate::PermanentId) value today, or a future stack
    /// object's id. It is minted from the monotonic [`GameState::next_object_id`],
    /// so it is strictly increasing and replay-stable, and it doubles as this
    /// effect's **timestamp**: within a layer, effects apply in ascending
    /// `source` order (CR 613.7, ADR 0010 §4). No wall-clock and no ambient
    /// counter is involved. Because it derives from the source object's id,
    /// removing that source (and this entry with it) reverts the computed value.
    pub source: u64,
    /// Which permanents this effect applies to.
    pub affects: EffectAffects,
    /// The continuous modification this effect performs. The variant fixes the
    /// CR 613 layer; only layer 7c power/toughness modification ships in this
    /// slice.
    pub modification: Modification,
}

impl StaticEffect {
    /// This effect's timestamp for intra-layer ordering: its [`source`] object
    /// id (ADR 0010 §4 — the id assigned when the effect was created). Exposed as
    /// a named accessor so ordering code reads by intent rather than by field.
    ///
    /// [`source`]: Self::source
    #[must_use]
    pub fn timestamp(&self) -> u64 {
        self.source
    }
}

/// Selects the permanents a [`StaticEffect`] applies to.
///
/// A deliberately small closed set for this slice: no targeting (that is ADR
/// 0009, a separate decision) and no authored-card selectors yet (those arrive
/// with the cards that create these effects). The one variant models the
/// canonical anthem, "creatures you control".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectAffects {
    /// Every creature controlled by the given player (anthem-style "creatures
    /// you control"). A permanent matches when it is currently a creature and
    /// its controller equals this player.
    CreaturesControlledBy(PlayerId),
}

/// The continuous modification a [`StaticEffect`] performs. The variant fixes
/// the CR 613 layer the effect applies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modification {
    /// CR 613 **layer 7c**: add the given signed amounts to power and toughness
    /// (a negative amount subtracts). Applied after counters, in timestamp order
    /// (ADR 0010 §3–§4).
    PowerToughness {
        /// Amount added to power.
        power: i32,
        /// Amount added to toughness.
        toughness: i32,
    },
}

/// The complete, immutable state of a game at one moment.
///
/// Every field is either raw state or a stable id; nothing derivable (current
/// characteristics, legal actions, whose turn it "feels" like) is stored here —
/// those are computed on demand by pure functions.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GameState {
    /// Current turn number (1-based); `0` in the empty [`Default`] state.
    pub turn: u32,
    /// The player whose turn it is, as an index into [`Self::players`].
    pub active_player: PlayerId,
    /// The player who currently holds priority, as an index into
    /// [`Self::players`]. Priority rotates through the seats as players pass;
    /// when all have passed in succession the step ends and priority returns to
    /// the active player. Out of range (as in [`Default`]) means no one holds
    /// priority, so no actions are legal.
    pub priority: PlayerId,
    /// How many players have passed priority in unbroken succession. When this
    /// reaches the number of seats, the step ends (see [`crate::apply_action`]);
    /// any action that is not a pass resets it to `0`.
    pub consecutive_passes: usize,
    /// The current phase/step of the turn.
    pub step: Step,
    /// Every player, in seating (turn) order.
    pub players: Vec<Player>,
    /// The shared battlefield, owned by the game rather than any one player.
    pub battlefield: Vec<Permanent>,
    /// The stack of spells and abilities, bottom first (the last element is the
    /// top and resolves first). Mana abilities never appear here.
    pub stack: Vec<StackObject>,
    /// Continuous static effects currently in force (ADR 0010 slice 3). This is
    /// **raw stored input, not a derivation**: the source ability/permanent puts
    /// each effect here and its removal takes it away. A permanent's *current*
    /// characteristics fold the applicable ones in on demand via
    /// [`characteristics`](crate::characteristics::characteristics) and are never
    /// stored. The read path sorts by [`StaticEffect::timestamp`], so this
    /// vector's own order does not affect the computed result.
    pub static_effects: Vec<StaticEffect>,
    /// Monotonic source of fresh object ids ([`PermanentId`], stack ids). Only
    /// ever increases, so an id is never reused even as objects change zones —
    /// zone-change identity is the mechanism (`crates/rune-engine/AGENTS.md`).
    pub next_object_id: u64,
    /// Whether the active player has played a land this turn. Reset when the next
    /// turn begins. Enforces the one-land-per-turn rule.
    pub land_played: bool,
    /// Whether the active player has declared attackers this combat (CR 508.1).
    ///
    /// Declaring attackers is a turn-based action the active player performs as a
    /// player *choice* (offered through [`crate::valid_actions`], like the cleanup
    /// discard), so the engine must record that the choice has been made to know
    /// the declare-attackers step has moved on from it to its priority round. An
    /// empty declaration still sets this (declaring *no* attackers is legal,
    /// CR 508.1a). Reset each turn.
    pub attackers_declared: bool,
    /// Whether the defending player has declared blockers this combat (CR 509.1).
    ///
    /// The mirror of [`Self::attackers_declared`] for the declare-blockers step:
    /// the defender's declaration is a player choice, and this records that it has
    /// been made so the step advances to its priority round. Reset each turn.
    pub blockers_declared: bool,
    /// Extra turns waiting to be taken, as a stack: the entry pushed last is
    /// taken first (MTG rule 720.1 — the most recently created extra turn
    /// happens first). Each entry is the player who takes that turn.
    pub extra_turns: Vec<PlayerId>,
    /// Extra steps to visit before the turn's natural sequence resumes, as a
    /// stack: the entry pushed last is visited first. An additional phase
    /// (e.g. an extra combat) is represented by queueing its constituent steps.
    pub extra_steps: Vec<Step>,
    /// Deterministic RNG seed/state for this game, injected at construction and
    /// advanced deterministically each time randomness is consumed (e.g. a
    /// future shuffle), so the whole game replays identically from the same
    /// starting seed. Every engine randomness draw takes from this slot and
    /// nowhere else — see the [module docs](self) for the full invariant. No
    /// generator ships yet; the slot is reserved so shuffling can land without a
    /// breaking state-shape change.
    ///
    /// Never included in any `GameView`: exposing it would leak future shuffle
    /// outcomes to players, so the engine→protocol projection must not copy it.
    pub rng_seed: u64,
    /// The pre-game [London mulligan](crate::mulligan) decision phase, when one is
    /// in progress (CR 103.5). `Some` from the moment opening hands are dealt
    /// ([`Self::new`]) until every player has kept, during which
    /// [`crate::valid_actions`] offers only each player's keep/mulligan decision
    /// and the turn structure does not advance; cleared to `None` — the value in
    /// every test-scaffold and post-mulligan state — once the game has begun.
    pub mulligan: Option<MulliganState>,
}

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
        Self {
            turn: 1,
            active_player: PlayerId(0),
            priority: PlayerId(0),
            consecutive_passes: 0,
            step: Step::Untap,
            players: vec![Player::new(), Player::new()],
            battlefield: Vec::new(),
            stack: Vec::new(),
            static_effects: Vec::new(),
            next_object_id: 1,
            land_played: false,
            attackers_declared: false,
            blockers_declared: false,
            extra_turns: Vec::new(),
            extra_steps: Vec::new(),
            rng_seed,
            // The bare scaffold starts a game already in progress, past any
            // mulligan; the London mulligan phase is entered only by [`Self::new`]
            // from a real [`GameSetup`](crate::GameSetup).
            mulligan: None,
        }
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

    /// Mint a fresh [`CardInstance`] for `card`, drawing a unique
    /// [`CardInstanceId`] from the monotonic counter.
    ///
    /// Called when a physical card first enters a game — deck loading (issue #9),
    /// token creation, or test setup — so every copy is individually addressable
    /// even when it shares a [`CardId`] with another.
    pub fn new_instance(&mut self, card: CardId) -> CardInstance {
        CardInstance {
            id: CardInstanceId(self.mint_id()),
            card,
        }
    }

    /// The player who currently holds priority, or `None` if [`Self::priority`]
    /// is out of range (as in the empty [`Default`] state).
    #[must_use]
    pub fn priority_holder(&self) -> Option<&Player> {
        self.players.get(self.priority.0)
    }

    /// The game's terminal result if it is over, else `None` (CR 104.2a).
    ///
    /// A game with at least two seats ends the moment at most one player has not
    /// lost: that survivor is the winner (CR 104.2a), or there is no winner when
    /// every player has lost (a draw, CR 104.4a). Derived fresh from the losers'
    /// stored [`has_lost`](Player::has_lost)/[`loss_reason`](Player::loss_reason);
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
    pub fn active_player(&self) -> Option<&Player> {
        self.players.get(self.active_player.0)
    }

    /// Advance the game to the next step of the turn structure, returning a new
    /// state (the input is never mutated).
    ///
    /// Order of precedence: a queued [extra step](Self::extra_steps) is visited
    /// first; otherwise the turn walks its natural sequence via [`Step::next`];
    /// advancing past [`Step::Cleanup`] ends the turn and begins the next one.
    ///
    /// This is the turn-structure FSM only. It does not touch priority, the
    /// stack, or state-based actions — those arrive with the action pipeline.
    #[must_use]
    pub fn advance(&self) -> Self {
        let mut next = self.clone();
        if let Some(step) = next.extra_steps.pop() {
            next.step = step;
        } else if next.step == Step::Cleanup {
            next.begin_next_turn();
        } else {
            next.step = next.step.next();
        }
        next
    }

    /// Begin the next turn on this owned state: bump the turn counter, hand the
    /// turn to the taker of a pending [extra turn](Self::extra_turns) or, absent
    /// one, to the next player in seating order, and reset to [`Step::Untap`].
    ///
    /// A no-op on a seatless state, so player rotation never divides by zero.
    fn begin_next_turn(&mut self) {
        if self.players.is_empty() {
            return;
        }
        self.turn += 1;
        self.active_player = match self.extra_turns.pop() {
            Some(taker) => taker,
            None => PlayerId((self.active_player.0 + 1) % self.players.len()),
        };
        self.step = Step::Untap;
        self.land_played = false;
        // A new turn is a new combat: the previous turn's declarations no longer
        // apply (CR 508.1 / 509.1 are performed afresh each combat).
        self.attackers_declared = false;
        self.blockers_declared = false;
    }

    /// Return a copy with an extra turn granted to `player`. Because extra turns
    /// are taken LIFO, this turn is taken before any extra turn granted earlier
    /// (MTG rule 720.1).
    #[must_use]
    pub fn with_extra_turn(&self, player: PlayerId) -> Self {
        let mut next = self.clone();
        next.extra_turns.push(player);
        next
    }

    /// Return a copy with `step` queued as an extra step, visited before the
    /// turn's natural sequence resumes. Queue the steps of an additional phase
    /// in reverse so they are visited in play order.
    #[must_use]
    pub fn with_extra_step(&self, step: Step) -> Self {
        let mut next = self.clone();
        next.extra_steps.push(step);
        next
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::player::STARTING_LIFE;

    #[test]
    fn new_two_player_initial_invariants() {
        let state = GameState::new_two_player();
        assert_eq!(state.turn, 1);
        assert_eq!(state.active_player, PlayerId(0));
        assert_eq!(state.step, Step::Untap);
        assert_eq!(state.players.len(), 2);
        assert!(state.battlefield.is_empty());
        assert!(state.stack.is_empty());
        assert!(!state.land_played);
        // The RNG seed slot defaults to 0 when no seed is injected.
        assert_eq!(state.rng_seed, 0);

        for player in &state.players {
            assert_eq!(player.life, STARTING_LIFE);
            assert!(player.library.is_empty());
            assert!(player.hand.is_empty());
            assert!(player.graveyard.is_empty());
            assert!(player.exile.is_empty());
        }

        // The active player resolves to an actual seat.
        let active = state.active_player().unwrap();
        assert_eq!(active.life, STARTING_LIFE);
    }

    #[test]
    fn seeded_constructor_records_the_seed_and_changes_nothing_else() {
        // The injected seed is stored verbatim, and the only difference from the
        // default constructor is that one field — the slot is inert for now.
        let seeded = GameState::new_two_player_with_seed(0xDEAD_BEEF);
        assert_eq!(seeded.rng_seed, 0xDEAD_BEEF);

        let mut normalized = seeded.clone();
        normalized.rng_seed = 0;
        assert_eq!(normalized, GameState::new_two_player());
    }

    #[test]
    fn cr_104_2a_result_is_none_while_both_players_remain() {
        // The game is not over while at least two players remain.
        let state = GameState::new_two_player();
        assert!(state.result().is_none());
        assert!(!state.is_over());
    }

    #[test]
    fn cr_104_2a_last_player_standing_wins() {
        // CR 104.2a: when one player remains, the game is over and that player wins.
        let mut state = GameState::new_two_player();
        state.players[1].has_lost = true;
        state.players[1].loss_reason = Some(LossReason::Concede);

        let result = state.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(0)));
        assert_eq!(result.losers, vec![PlayerId(1)]);
        assert_eq!(result.reason, LossReason::Concede);
        assert!(state.is_over());
    }

    #[test]
    fn cr_104_4a_simultaneous_loss_is_a_draw() {
        // CR 104.4a: if every remaining player loses at once, no one wins (a draw).
        let mut state = GameState::new_two_player();
        for player in &mut state.players {
            player.has_lost = true;
            player.loss_reason = Some(LossReason::ZeroLife);
        }

        let result = state.result().unwrap();
        assert_eq!(result.winner, None, "a simultaneous loss has no winner");
        assert_eq!(result.losers, vec![PlayerId(0), PlayerId(1)]);
    }

    #[test]
    fn default_state_is_empty() {
        let state = GameState::default();
        assert_eq!(state.turn, 0);
        assert_eq!(state.step, Step::Untap);
        assert!(state.players.is_empty());
        // No seats, so there is no active player to borrow.
        assert!(state.active_player().is_none());
    }

    #[test]
    fn advance_walks_one_full_turn_without_rotating() {
        // From Untap, eleven advances reach Cleanup, all within turn 1 for the
        // same active player — no rotation happens mid-turn.
        let mut state = GameState::new_two_player();
        let sequence = [
            Step::Upkeep,
            Step::Draw,
            Step::PrecombatMain,
            Step::BeginCombat,
            Step::DeclareAttackers,
            Step::DeclareBlockers,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain,
            Step::End,
            Step::Cleanup,
        ];
        for expected in sequence {
            state = state.advance();
            assert_eq!(state.step, expected);
            assert_eq!(state.turn, 1);
            assert_eq!(state.active_player, PlayerId(0));
        }
    }

    #[test]
    fn advance_past_cleanup_starts_next_players_turn() {
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;

        let next = state.advance();
        assert_eq!(next.turn, 2);
        assert_eq!(next.active_player, PlayerId(1));
        assert_eq!(next.step, Step::Untap);
    }

    #[test]
    fn two_turns_cycle_back_to_the_first_player() {
        // Player 0 (turn 1) -> player 1 (turn 2) -> player 0 (turn 3).
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(1));

        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.turn, 3);
        assert_eq!(state.active_player, PlayerId(0));
    }

    #[test]
    fn extra_turn_is_taken_before_normal_rotation() {
        // Active player 0 has an extra turn queued; ending the turn hands the
        // turn back to player 0 rather than rotating to player 1.
        let mut state = GameState::new_two_player().with_extra_turn(PlayerId(0));
        state.step = Step::Cleanup;

        let next = state.advance();
        assert_eq!(next.turn, 2);
        assert_eq!(next.active_player, PlayerId(0));
        assert_eq!(next.step, Step::Untap);
        assert!(next.extra_turns.is_empty());
    }

    #[test]
    fn extra_turns_are_taken_last_in_first_out() {
        // Grant player 1's extra turn, then player 0's: player 0 goes first.
        let mut state = GameState::new_two_player()
            .with_extra_turn(PlayerId(1))
            .with_extra_turn(PlayerId(0));

        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(0));

        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(1));

        // With the queue drained, rotation resumes normally.
        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(0));
    }

    #[test]
    fn extra_step_is_visited_before_the_natural_sequence() {
        // An additional precombat main phase inserted after the postcombat main.
        let mut state = GameState::new_two_player();
        state.step = Step::PostcombatMain;
        let state = state.with_extra_step(Step::PrecombatMain);

        let next = state.advance();
        assert_eq!(next.step, Step::PrecombatMain);
        assert_eq!(next.turn, 1);
        assert_eq!(next.active_player, PlayerId(0));
        assert!(next.extra_steps.is_empty());

        // Once the extra step is consumed, the sequence resumes from it.
        assert_eq!(next.advance().step, Step::BeginCombat);
    }

    #[test]
    fn advance_does_not_mutate_input() {
        let before = GameState::new_two_player();
        let _ = before.advance();
        assert_eq!(before.step, Step::Untap);
        assert_eq!(before.turn, 1);
    }

    #[test]
    fn advance_on_seatless_state_does_not_panic() {
        // Default state has no players; ending its turn must not divide by zero.
        let state = GameState {
            step: Step::Cleanup,
            ..GameState::default()
        };
        let next = state.advance();
        assert_eq!(next.turn, 0);
        assert_eq!(next.step, Step::Cleanup);
    }
}
