//! Legal action enumeration — the engine's legality authority.
//!
//! [`Action`] is the closed set of things a player may take; [`valid_actions`]
//! computes, pull-based, exactly which are legal for the current priority
//! holder. [`crate::apply_action`] validates a chosen action against this set
//! before applying it.

use crate::ability::{Ability, Cost};
use crate::card::abilities_of;
use crate::card_type::CardType;
use crate::id::{CardId, CardInstance, PermanentId};
use crate::mana::parse_mana_cost;
use crate::phase::Step;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// An action a player may take. The engine generates the legal set with
/// [`valid_actions`] and validates a chosen action against it in
/// [`crate::apply_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Yield priority without taking any other action.
    PassPriority,
    /// Play a land from hand (a special action; lands do not use the stack).
    PlayLand {
        /// The specific land card in the active player's hand to play. Names the
        /// physical copy, so two identical lands in hand are distinguishable.
        card: CardInstance,
    },
    /// Activate an ability of a permanent the priority holder controls.
    ActivateAbility {
        /// The permanent whose ability is activated.
        permanent: PermanentId,
        /// Index into the permanent's abilities (see [`abilities_of`]).
        index: usize,
    },
    /// Cast a spell from hand, paying its mana cost from the caster's pool.
    CastSpell {
        /// The specific card in the caster's hand to cast. Names the physical
        /// copy, so two identical cards in hand are distinguishable.
        card: CardInstance,
    },
}

/// Enumerate the actions legal for the player who currently holds priority.
///
/// Pull-based and pure: computed fresh from `state`, never cached on it. The
/// priority holder may always pass; may play a land, cast a creature, or (for
/// permanents they control) activate abilities when the relevant timing and cost
/// conditions hold. A state with no valid priority holder offers nothing.
#[must_use]
pub fn valid_actions(state: &GameState, db: &CardDatabase) -> Vec<Action> {
    if state.priority_holder().is_none() {
        return Vec::new();
    }
    let priority = state.priority;
    let mut actions = vec![Action::PassPriority];

    // Sorcery-speed: the active player, in a main phase, with an empty stack.
    let sorcery_speed = priority == state.active_player
        && matches!(state.step, Step::PrecombatMain | Step::PostcombatMain)
        && state.stack.is_empty();

    if let Some(player) = state.players.get(priority.0) {
        // Play a land: at sorcery speed, one per turn.
        if sorcery_speed && !state.land_played {
            for &card in &player.hand {
                if is_land(db, card.card) {
                    actions.push(Action::PlayLand { card });
                }
            }
        }

        // Cast a creature spell payable from the current pool (sorcery speed).
        if sorcery_speed {
            for &card in &player.hand {
                if let Some(data) = db.card(card.card) {
                    if is_creature(db, card.card)
                        && player.mana_pool.can_pay(&parse_mana_cost(&data.mana_cost))
                    {
                        actions.push(Action::CastSpell { card });
                    }
                }
            }
        }
    }

    // Activate abilities of permanents the priority holder controls.
    for perm in &state.battlefield {
        if perm.controller != priority {
            continue;
        }
        for (index, ability) in abilities_of(db, perm.card).iter().enumerate() {
            if let Ability::Activated { cost, .. } = ability {
                if cost_payable(cost, perm) {
                    actions.push(Action::ActivateAbility {
                        permanent: perm.id,
                        index,
                    });
                }
            }
        }
    }

    actions
}

/// Whether every cost in `cost` is payable given the source `permanent`'s state.
fn cost_payable(cost: &[Cost], permanent: &Permanent) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
    })
}

/// Whether `card` is a land, by its structured printed types.
fn is_land(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|c| c.has_type(CardType::Land))
}

/// Whether `card` is a creature, by its structured printed types.
fn is_creature(db: &CardDatabase, card: CardId) -> bool {
    db.card(card)
        .is_some_and(|c| c.has_type(CardType::Creature))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    #[test]
    fn valid_actions_offers_pass_priority_to_the_priority_holder() {
        let state = GameState::new_two_player();
        assert_eq!(valid_actions(&state, &db()), vec![Action::PassPriority]);
    }

    #[test]
    fn valid_actions_on_seatless_state_is_empty() {
        // Default has no players, so no one holds priority and nothing is legal.
        assert!(valid_actions(&GameState::default(), &db()).is_empty());
    }
}
