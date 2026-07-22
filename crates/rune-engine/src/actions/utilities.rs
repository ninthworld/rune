//! Utility helpers for action validation and generation.

use crate::ability::Cost;
use crate::card_type::CardType;
use crate::id::{CardId, PermanentId};
use crate::state::Permanent;
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

/// Whether every cost in `cost` is payable given the source `permanent`'s state.
pub(crate) fn cost_payable(cost: &[Cost], permanent: &Permanent) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
    })
}

/// Whether every element of `ids` is distinct. O(n²), which is fine for the
/// handful of creatures a combat declaration ever names and keeps the engine free
/// of a hashing dependency for a tiny list.
pub(crate) fn all_unique(ids: &[PermanentId]) -> bool {
    ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
}
