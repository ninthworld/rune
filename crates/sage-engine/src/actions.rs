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
mod payment;
mod targeting;
mod utilities;

#[cfg(test)]
mod tests;

pub(crate) use definition::discards_of;
pub use definition::{
    Action, Attack, Block, CostPayment, DamageOrder, ManaSource, TargetRequirement,
};
pub use generation::valid_actions;
pub(crate) use legality::action_is_legal;
pub(crate) use payment::{apply_payment, payment_covers_cast};
pub use payment::{
    auto_payment, discard_cost, is_plain_mana_source, mana_ability_pips, payment_pips,
    payment_sources, remaining_cost_pips, DiscardCost, PaymentPip,
};
pub(crate) use targeting::legal_targets_for_spec;
pub use targeting::target_requirements;
pub(crate) use utilities::potential_mana_pool;
