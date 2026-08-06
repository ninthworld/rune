//! The root game state and the shared battlefield.
//!
//! ## Randomness invariant
//!
//! All randomness in the engine draws from [`GameState::rng_seed`] and nowhere
//! else — no `rand` crate, no wall-clock time, no thread-local or ambient
//! generator. The seed is injected through the constructors, so a game replays
//! identically from the same starting state. [`GameState::new`] consumes it to
//! shuffle opening libraries (CR 103.3) with a tiny inline generator (SplitMix64,
//! see [`crate::rng`] and `docs/decisions/0006-deterministic-seeded-shuffle.md`)
//! and stores the advanced generator state back into the slot, so later draws
//! continue the same stream. Concentrating every draw here is what makes the
//! `crates/sage-engine/AGENTS.md` rule "no randomness without an injected seed"
//! structurally satisfiable, rather than satisfied only by the absence of
//! randomness.

mod log;
mod query;
mod setup;
#[cfg(test)]
mod tests;
mod turn;
mod types;
mod zone;

pub use types::{
    CommanderDamage, CounterKind, DamageTarget, Duration, EffectAffects, Emblem, GameEvent,
    GameLogEntry, GameResult, GraveyardCasting, IgnoringHexproof, LoggedIdentity, LoggedPermanent,
    Modification, Permanent, StaticEffect,
};

use crate::id::PlayerId;
use crate::mulligan::MulliganState;
use crate::phase::Step;
use crate::player::Player;
use crate::stack::StackObject;

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
    /// The emblems in the game (CR 114), in creation order — see [`Emblem`].
    ///
    /// A second list of ability sources beside [`Self::battlefield`], and deliberately
    /// not part of it: an emblem is not a permanent, is in no zone, and is never removed,
    /// so putting it on the battlefield would make every state-based action, every
    /// targeting spec, and every combat gate have to say why it does not apply. Keeping
    /// it here means each of them says nothing, which is the correct answer.
    ///
    /// The two paths that read abilities off objects — the CR 613 characteristics
    /// computation and the diff-based trigger collector — walk this list alongside the
    /// battlefield. Nothing else does. Empty in every game where no ultimate has
    /// resolved, so a game without emblems is byte-for-byte unchanged.
    pub emblems: Vec<Emblem>,
    /// Permissions to cast cards from a graveyard, granted this turn — see
    /// [`GraveyardCasting`]. Empty in almost every state, and cleared at the turn
    /// boundary.
    pub graveyard_casting: Vec<GraveyardCasting>,
    /// Permissions to aim spells and abilities as though hexproof were not there,
    /// granted this turn — see [`IgnoringHexproof`]. The same per-player, per-turn
    /// shape as [`Self::graveyard_casting`], empty in almost every state, and cleared
    /// at the same turn boundary.
    pub ignoring_hexproof: Vec<IgnoringHexproof>,
    /// One-shot **replacement effects** created this turn (CR 614.1b) — see
    /// [`PendingReplacement`](crate::PendingReplacement).
    ///
    /// The same per-turn shape as [`Self::graveyard_casting`], empty in almost every
    /// state, and cleared at the same turn boundary. It is the *second* source list the
    /// replacement layer walks; the first is the entering object's own abilities, which
    /// are read off the card and never stored (see [`crate::replacement`]).
    pub replacements: Vec<crate::replacement::PendingReplacement>,
    /// The **damage-prevention shields** in force this turn (CR 615.1), each named by
    /// the class of damage it prevents — see [`DamageFilter`](crate::DamageFilter).
    ///
    /// Raw stored state for the same reason [`Self::replacements`] is: a shield exists
    /// because an effect said so, and no snapshot of the board could recover it. It
    /// differs from that list in its lifetime, which is the difference between `the next
    /// time … this turn` and `this turn`: a shield is **not** spent by applying, and it
    /// ends in the **cleanup step** rather than at the turn boundary, simultaneously
    /// with the pump and the marked damage it is authored beside (CR 514.2).
    ///
    /// Each entry is the filter alone: nothing reads who created a shield, because a
    /// shield names no target, and CR 616.1 gives the ordering decision to the affected
    /// player rather than to its controller. Empty in almost every state.
    pub prevention: Vec<crate::replacement::DamageFilter>,
    /// **Delayed triggered abilities** created this turn (CR 603.7) — see
    /// [`PendingDelayedTrigger`](crate::PendingDelayedTrigger).
    ///
    /// The same per-turn shape as [`Self::replacements`], empty in almost every state,
    /// spent by firing, and cleared at the same turn boundary. It is a **fourth source
    /// list** for the trigger collector, and the only one that is not an object: a
    /// delayed ability belongs to nothing anyone can point at, and CR 603.7e says it
    /// fires whether or not what created it is still around.
    pub delayed_triggers: Vec<crate::delayed::PendingDelayedTrigger>,
    /// The stack of spells and abilities, bottom first (the last element is the
    /// top and resolves first). Mana abilities never appear here.
    pub stack: Vec<StackObject>,
    /// Continuous static effects currently in force (ADR 0005 slice 3). This is
    /// **raw stored input, not a derivation**: the source ability/permanent puts
    /// each effect here and its removal takes it away. A permanent's *current*
    /// characteristics fold the applicable ones in on demand via
    /// [`characteristics`](crate::characteristics::characteristics) and are never
    /// stored. The read path sorts by [`StaticEffect::timestamp`], so this
    /// vector's own order does not affect the computed result.
    pub static_effects: Vec<StaticEffect>,
    /// Monotonic source of fresh object ids ([`PermanentId`], stack ids). Only
    /// ever increases, so an id is never reused even as objects change zones —
    /// zone-change identity is the mechanism (`crates/sage-engine/AGENTS.md`).
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
    /// been made so the step advances to its priority round. Set once *every*
    /// attacked player has declared (see [`Self::blockers_declared_by`]). Reset each
    /// turn.
    pub blockers_declared: bool,
    /// Each multi-blocked attacker's chosen combat-damage assignment order (CR 510.1,
    /// issue #346): `(attacker, blockers-in-chosen-order)` pairs. The attacking
    /// player picks the order for every attacker blocked by two or more creatures;
    /// combat damage is then assigned just-lethal along that order. An attacker
    /// absent here (never multi-blocked, or not yet ordered) falls back to stable
    /// battlefield order. Raw stored state, set by the order-damage decision and
    /// cleared each turn with the other combat declarations.
    pub damage_orders: Vec<(crate::id::PermanentId, Vec<crate::id::PermanentId>)>,
    /// The planeswalkers that have already had a loyalty ability activated this turn
    /// (CR 606.3), in activation order.
    ///
    /// **Raw stored history, not a derivation** (ADR 0005 §1): "has a loyalty ability
    /// of this permanent been activated this turn" is a fact a bare snapshot cannot
    /// recover — spending loyalty leaves the same board a planeswalker that spent none
    /// would have — so, like [`Permanent::damage`] and [`Self::damage_orders`], it is
    /// recorded rather than computed. Keyed by [`PermanentId`](crate::PermanentId), so
    /// a planeswalker that leaves and returns is a new object with a fresh allowance,
    /// which is exactly what CR 606.3 says. Cleared when the next turn begins.
    pub loyalty_activations: Vec<crate::id::PermanentId>,
    /// The attacked players who have already declared blockers this combat, in the
    /// order they declared (issue #344). When attackers are split across several
    /// defenders each attacked player gets their own declare-blockers decision,
    /// resolved in APNAP order; this records who is done so the engine knows which
    /// defender owes the next declaration and when [`Self::blockers_declared`] can
    /// be set. Empty and unused in a two-player game (the sole defender declares
    /// once). Reset each turn.
    pub blockers_declared_by: Vec<PlayerId>,
    /// The seat priority returns to once every choice the game is currently waiting on
    /// has been answered, or `None` when it is waiting on none.
    ///
    /// Two kinds of choice interrupt priority this way, and they share one slot because
    /// they can be owed at the same moment and only one seat can hold priority: aiming
    /// a triggered ability already on the stack (CR 603.3d,
    /// [`pending_trigger_target_choice`](crate::pending_trigger_target_choice)) and
    /// answering a mid-resolution player choice (CR 701.8/701.17/701.19,
    /// [`pending_player_choice`](crate::pending_player_choice)).
    ///
    /// **Raw stored state, not a derivation.** The chooser is frequently not the player
    /// whose action produced the choice — a creature killed by an opponent's removal
    /// spell gives its own controller a dies trigger to aim, and a Mind Rot resolving on
    /// its caster's turn asks the other seat to discard. So the engine hands priority to
    /// the chooser and must remember whose it was, and the answer is not recoverable
    /// from a snapshot afterwards (the same reasoning as
    /// [`Player::turn_began`](crate::player::Player::turn_began)).
    ///
    /// Set when the first such choice appears and consumed the moment the last one is
    /// answered, so it is `None` outside that window. Whether a choice is *currently*
    /// owed stays derived, from the stack and from [`Self::pending_choices`].
    pub interrupted_priority: Option<PlayerId>,
    /// Mid-resolution player choices the game is waiting on, in the order they were
    /// posed and answered from the front (CR 701.8 discard, CR 701.17 scry,
    /// CR 701.19 search — see [`crate::PendingChoice`]).
    ///
    /// **Raw queued state, exactly as [`Self::stack`] is**, and for the same reason: a
    /// suspended resolution's remainder is not recoverable from a snapshot. Whether a
    /// choice is owed *right now* stays derived from this queue
    /// ([`pending_player_choice`](crate::pending_player_choice)), and so do the cards
    /// each one offers ([`choice_candidates`](crate::choice_candidates)) — there is no
    /// flag and no snapshotted candidate list that could drift from the zones.
    ///
    /// Empty in every state that is not mid-resolution, so a game that never resolves a
    /// choice-posing effect is byte-for-byte unchanged.
    pub pending_choices: Vec<crate::choice::PendingChoice>,
    /// Permanents dealt damage by a source with deathtouch (CR 702.2b), pending the
    /// CR 704.5h state-based action that destroys them.
    ///
    /// **Raw stored input, not a derivation** (ADR 0005 §1): the combat-damage
    /// step records a struck creature here (see `apply.rs :: deal_combat_damage`)
    /// because "was dealt damage by a deathtouch source" is history a bare
    /// snapshot cannot recover — the same reasoning as [`Permanent::damage`] and
    /// [`Player::attempted_draw_from_empty`](crate::player::Player::attempted_draw_from_empty). The SBA loop
    /// ([`crate::sba::run_state_based_actions`]) consumes (drains) it, so it is
    /// empty between resolutions.
    ///
    /// Combat is not the only writer: a fight's damage has a *permanent* as its source
    /// (CR 701.12), so it carries that creature's deathtouch through this same list.
    /// Damage from a spell has no source that could.
    pub deathtouch_struck: Vec<crate::id::PermanentId>,
    /// Cumulative **combat** damage each commander has dealt each player over the
    /// game (CR 903.10a), one entry per `(commander designation, damaged player)`
    /// pair that has taken any — see [`CommanderDamage`].
    ///
    /// **Raw stored history, not a derivation** (ADR 0005 §1): unlike marked
    /// [`Permanent::damage`] (which clears at cleanup) this total is *never*
    /// reset — it accumulates for the whole game and outlives the commander's zone
    /// changes because it is keyed to the designation, not to any battlefield
    /// object. Fed only by combat damage a commander deals a player (see
    /// `apply.rs :: apply_combat_batch`); non-combat damage from a commander does
    /// not count (CR 903.10a). The state-based-actions loop reads it for the CR
    /// 903.10a loss ([`crate::sba::run_state_based_actions`]). Empty for a game
    /// with no commanders, so a non-commander game is byte-for-byte unchanged.
    pub commander_damage: Vec<CommanderDamage>,
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
    /// Most recent deterministic engine events, in sequence order. This bounded
    /// window is authoritative history carried into every projected game view.
    pub log: Vec<GameLogEntry>,
    /// Next sequence number for [`Self::log`].
    pub next_log_sequence: u64,
}
