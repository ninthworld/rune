//! Legal action enumeration — the engine's legality authority.
//!
//! [`Action`] is the closed set of things a player may take; [`valid_actions`]
//! computes, pull-based, exactly which are legal for the current priority
//! holder. [`crate::apply_action`] validates a chosen action against this set —
//! and, for a targeted action, against freshly computed legal target sets — in
//! [`action_is_legal`] before applying it.

mod definition;
mod generation;
mod legality;
mod targeting;
mod utilities;

#[cfg(test)]
mod tests;

pub use definition::{Action, Attack, Block, DamageOrder, TargetRequirement};
pub use generation::valid_actions;
pub(crate) use legality::action_is_legal;
pub use targeting::target_requirements;
