//! RUNE rules engine — layer 3.
//!
//! Invariants (see AGENTS.md in this crate):
//! - `GameState` is an immutable value type; `apply_action` returns a new state.
//! - No I/O, no async, no globals, no time. Pure functions only.
//! - Everything derivable is computed on demand (pull-based), never cached on objects.
//!
//! The pipeline is split along its natural seams: [`actions`] is the legality
//! authority ([`Action`], [`valid_actions`]); [`apply`] is the [`apply_action`]
//! transition and its per-action helpers; [`resolve`] resolves stack objects;
//! [`sba`] runs state-based actions; [`triggers`] collects triggers by diffing.

mod ability;
mod actions;
mod apply;
mod automation;
mod card;
mod card_type;
mod catalog;
mod characteristics;
mod combat;
pub mod compat;
#[cfg(test)]
mod fixtures;
mod id;
mod mana;
mod mulligan;
mod phase;
mod player;
mod resolve;
mod rng;
mod sba;
mod scripted;
mod setup;
mod stack;
mod state;
mod triggers;
mod zone;

pub use ability::{
    is_mana_ability, Ability, Cost, Effect, PlayerRef, Target, TargetSpec, TriggerCondition,
};
pub use actions::{
    target_requirements, valid_actions, Action, Attack, Block, DamageOrder, TargetRequirement,
};
pub use apply::apply_action;
pub use automation::priority_has_no_meaningful_action;
pub use card::{
    abilities_of, AuraGrant, CardData, CardDatabase, CatalogError, Keyword, Printing,
    PrintingDatabase, Rarity, SCHEMA_VERSION,
};
pub use card_type::{CardType, Supertype};
pub use catalog::Violation;
pub use characteristics::{characteristics, Characteristics};
pub use combat::{
    attacked_players, attacker_candidates, attackers_needing_damage_order, attacking_defender_of,
    blocker_candidates, blocker_candidates_for, declared_attackers, defender_candidates,
    defending_player, pending_blocker_declarer, pending_damage_order,
};
pub use id::{
    CardId, CardInstance, CardInstanceId, FunctionalId, FunctionalIdError, OracleId, PermanentId,
    PlayerId,
};
pub use mana::{parse_mana_cost, Color, ManaCost, ManaPool};
pub use mulligan::{bottom_requirement, BottomRequirement, MulliganState, PlayerMulligan};
pub use phase::Step;
pub use player::{LossReason, Player, MAX_HAND_SIZE, STARTING_LIFE};
pub use scripted::scripted_rules_text;
pub use setup::{
    GameSetup, PlayerSetup, SetupError, DEFAULT_STARTING_HAND_SIZE, DEFAULT_STARTING_LIFE,
};
pub use stack::{StackId, StackObject, StackObjectKind};
pub use state::{
    CounterKind, DamageTarget, Duration, EffectAffects, GameEvent, GameLogEntry, GameResult,
    GameState, LoggedPermanent, Modification, Permanent, StaticEffect,
};
pub use triggers::{collect_triggers, Trigger};
pub use zone::Zone;
