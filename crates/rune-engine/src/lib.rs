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
mod card;
mod card_type;
mod characteristics;
mod combat;
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

pub use ability::{is_mana_ability, Ability, Cost, Effect, Target, TargetSpec, TriggerCondition};
pub use actions::{target_requirements, valid_actions, Action, Block, TargetRequirement};
pub use apply::apply_action;
pub use card::{abilities_of, CardData, CardDatabase, Printing, PrintingDatabase, Rarity};
pub use card_type::{CardType, Supertype};
pub use characteristics::{characteristics, Characteristics};
pub use combat::{attacker_candidates, blocker_candidates, declared_attackers};
pub use id::{CardId, CardInstance, CardInstanceId, OracleId, PermanentId, PlayerId};
pub use mana::{parse_mana_cost, Color, ManaCost, ManaPool};
pub use mulligan::{bottom_requirement, BottomRequirement, MulliganState, PlayerMulligan};
pub use phase::Step;
pub use player::{LossReason, Player, MAX_HAND_SIZE, STARTING_LIFE};
pub use setup::{
    GameSetup, PlayerSetup, SetupError, DEFAULT_STARTING_HAND_SIZE, DEFAULT_STARTING_LIFE,
};
pub use stack::{StackId, StackObject, StackObjectKind};
pub use state::{
    CounterKind, EffectAffects, GameResult, GameState, Modification, Permanent, StaticEffect,
};
pub use triggers::{collect_triggers, Trigger};
pub use zone::Zone;
