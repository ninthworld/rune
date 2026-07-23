//! Combat declarations and combat damage: eligibility of attackers and blockers,
//! the turn-structure bookkeeping the declare steps need, and the combat-damage
//! assignment the combat-damage step performs.
//!
//! Declarations (issue #117): who *may* attack (CR 508.1a), who *may* block
//! (CR 509.1a), and which player owes the declaration in each declare step.
//! Attack targets (issue #341): each attacker is declared to attack a chosen
//! defending player — [`Permanent::attacking`] records *whom* (an
//! `Option<PlayerId>`), not a bare boolean. In a two-player game the sole opponent
//! is the only legal defender ([`defender_candidates`] returns exactly it), so
//! combat plays as it always has; with more seats each attacker picks an opponent,
//! blocker eligibility is scoped to attackers attacking that blocker's controller,
//! and combat damage routes to each attacker's own defender. The multi-defender
//! *declaration flow* (issue #344): when attackers are split across several
//! defenders, each attacked player declares blockers for the attackers attacking
//! them, in APNAP order ([`attacked_players`] / [`pending_blocker_declarer`]), and
//! combat damage is computed once after the final declaration.
//! Combat damage (issue #118, extended by #154): the assignment each attacker and
//! blocker makes in a combat-damage step (CR 510.1), gathered so it can be dealt
//! simultaneously (CR 510.2). First strike splits combat into two damage steps
//! (CR 510.5, keyed by [`DamageStep`]); trample (CR 702.19e), deathtouch
//! (CR 702.2b / 510.1e), and lifelink (CR 702.15e) shape the assignment within a
//! step. Double strike (CR 702.4, issue #373): a double striker deals in *both* the
//! first-strike and the regular step, and the player-chosen damage-assignment order
//! (issue #346) applies in each. Every function here is a pure predicate/enumeration
//! over an immutable [`GameState`] — no I/O, no mutation — consistent with the
//! engine's rules.

mod damage;
mod declaration;
mod eligibility;
mod helpers;

pub(crate) use damage::{
    blocked_attackers, combat_damage, combat_has_first_strike, CombatDamage, DamageStep,
};
pub(crate) use declaration::priority_after_step_change;
pub use declaration::{
    attacked_players, attackers_needing_damage_order, pending_blocker_declarer,
    pending_damage_order,
};
pub use eligibility::{
    attacker_candidates, attacking_defender_of, blocker_candidates, blocker_candidates_for,
    declared_attackers, defender_candidates, defending_player,
};
pub(crate) use helpers::{blocker_can_block_attacker, has_keyword};
