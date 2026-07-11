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
mod id;
mod mana;
mod phase;
mod player;
mod resolve;
mod sba;
mod scripted;
mod stack;
mod state;
mod triggers;
mod zone;

pub use ability::{is_mana_ability, Ability, Cost, Effect, TriggerCondition};
pub use actions::{valid_actions, Action};
pub use apply::apply_action;
pub use card::{abilities_of, CardData, CardDatabase};
pub use card_type::{CardType, Supertype};
pub use characteristics::{characteristics, Characteristics};
pub use id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
pub use mana::{parse_mana_cost, Color, ManaCost, ManaPool};
pub use phase::Step;
pub use player::{Player, STARTING_LIFE};
pub use stack::{StackId, StackObject, StackObjectKind};
pub use state::{CounterKind, GameState, Permanent};
pub use triggers::{collect_triggers, Trigger};
pub use zone::Zone;
