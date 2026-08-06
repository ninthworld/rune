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

pub(crate) use definition::{discards_of, sacrifices_of};
pub use definition::{
    Action, Attack, Block, CostPayment, DamageOrder, ManaSource, TargetRequirement,
};
pub use generation::valid_actions;
pub(crate) use legality::action_is_legal;
pub use payment::{
    activation_discard_cost, activation_sacrifice_candidates, auto_activation_payment,
    auto_payment, discard_cost, is_plain_mana_source, mana_ability_pips, payment_pips,
    payment_sources, remaining_cost_pips, sacrifice_cost, DiscardCost, PaymentPip, SacrificeCost,
};
pub(crate) use payment::{
    apply_payment, chosen_costs_are_payable, payment_covers_activation, payment_covers_cast,
};
pub(crate) use targeting::legal_targets_for_spec;
pub use targeting::target_requirements;
pub(crate) use utilities::{graveyard_ability, potential_mana_pool};
