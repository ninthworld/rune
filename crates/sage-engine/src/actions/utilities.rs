//! Utility helpers for action validation and generation.

use crate::ability::Cost;
use crate::card_type::CardType;
use crate::id::{CardId, PermanentId};
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Whether `card` is a land, by its structured printed types.
pub(crate) fn is_land(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|c| c.has_type(CardType::Land))
}

/// Whether `card` may be cast as a spell from hand today (CR 117.1a).
///
/// A land is never cast — it is played as a special action (CR 116.2a) and is
/// offered separately. Every other card type — instant, sorcery, artifact,
/// enchantment (Auras included, since issue #152), creature — is castable, subject
/// to timing and cost checked by the caller. An Aura additionally requires a legal
/// enchant target to be *offered* (CR 303.4c/601.2c); that is enforced by the
/// per-slot candidate check in [`crate::valid_actions`] over [`crate::CardData::cast_target_specs`],
/// not here.
pub(crate) fn is_castable_spell(data: &crate::CardData) -> bool {
    !data.has_type(CardType::Land)
}

/// Whether every cost in `cost` is payable right now, given the source
/// `permanent`'s state and its controller's mana pool.
///
/// Mana affordability is decided by the same [`ManaPool::can_pay`](crate::ManaPool::can_pay)
/// the cast path uses over the same `{...}` notation, so an ability is offered
/// exactly when [`crate::apply_action`] will succeed in charging for it — the
/// offer and the charge can never disagree about a cost string.
pub(crate) fn cost_payable(state: &GameState, cost: &[Cost], permanent: &Permanent) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
        Cost::Mana { mana } => state
            .players
            .get(permanent.controller.0)
            .is_some_and(|player| {
                player
                    .mana_pool
                    .can_pay(&crate::mana::parse_mana_cost(mana))
            }),
    })
}

/// Whether `cost` contains the tap symbol `{T}` (CR 118.3f) — the cost component
/// CR 302.6 forbids a summoning-sick creature from paying.
///
/// NOTE: [`Cost::Tap`] is the only cost the effect IR models today. When the untap
/// symbol `{Q}` (CR 118.3g) is added it belongs in this predicate too: CR 302.6
/// restricts *both* symbols on a summoning-sick creature, and this is the one seam
/// that gate runs through.
pub(crate) fn cost_requires_tapping(cost: &[Cost]) -> bool {
    cost.contains(&Cost::Tap)
}

/// Whether CR 302.6 forbids activating an ability of `permanent` whose activation
/// cost is `cost`: the cost includes `{T}` and the permanent is a creature still
/// affected by summoning sickness (see
/// [`summoning_sickness_restricts`](crate::combat::summoning_sickness_restricts),
/// which applies the CR 702.10b haste exemption).
///
/// CR 605.3a exempts nothing: a mana ability with `{T}` in its cost is gated
/// exactly like any other activated ability. Non-creature permanents are never
/// summoning sick, so a land played this turn still taps for mana.
pub(crate) fn tap_cost_is_summoning_sick(
    state: &GameState,
    permanent: &Permanent,
    cost: &[Cost],
    db: &CardDatabase,
) -> bool {
    cost_requires_tapping(cost) && crate::combat::summoning_sickness_restricts(state, permanent, db)
}

/// Whether every element of `ids` is distinct. O(n²), which is fine for the
/// handful of creatures a combat declaration ever names and keeps the engine free
/// of a hashing dependency for a tiny list.
pub(crate) fn all_unique(ids: &[PermanentId]) -> bool {
    ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
}
