//! The card-effect IR: a small, closed, serde-friendly vocabulary of abilities,
//! costs, effects, and trigger conditions.
//!
//! Abilities are **data**, carried on [`crate::CardData`] and interpreted by pure
//! functions over [`crate::GameState`] (see `crate::apply_action`). Nothing here
//! is a closure or a listener: a triggered ability's condition is a value matched
//! by a pure predicate against a before/after diff, honoring the engine's
//! pull-based, no-observer rule (`crates/rune-engine/AGENTS.md`).
//!
//! Cards the closed IR cannot express fall back to the code table in
//! [`crate::scripted`]; see `docs/decisions/0007-card-effect-ir-hybrid.md`.

use serde::Deserialize;

use crate::mana::Color;

/// One ability of a card.
///
/// The set is deliberately small and grows by adding variants (static/keyword
/// abilities arrive later). Deserialized with an internal `type` tag, e.g.
/// `{"type": "activated", ...}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ability {
    /// An activated ability: pay its costs to produce its effects (e.g. a land's
    /// `{T}: Add {G}`).
    Activated {
        /// Costs paid to activate, all of which must be paid.
        cost: Vec<Cost>,
        /// Effects produced when the ability resolves (or immediately, for a
        /// mana ability — see [`is_mana_ability`]).
        effects: Vec<Effect>,
    },
    /// A triggered ability: when its condition is met, its effects go on the
    /// stack (e.g. `When this enters the battlefield, draw a card.`).
    Triggered {
        /// The condition that causes this ability to trigger.
        event: TriggerCondition,
        /// Effects produced when the triggered ability resolves.
        effects: Vec<Effect>,
    },
}

/// A cost paid to activate an ability.
///
/// Deserialized with an internal `kind` tag, e.g. `{"kind": "tap"}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cost {
    /// Tap the source permanent (`{T}`). Payable only while it is untapped.
    Tap,
}

/// A single effect an ability (or spell) produces.
///
/// Deserialized with an internal `kind` tag, e.g.
/// `{"kind": "add_mana", "color": "green", "amount": 1}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    /// Add mana to the controller's mana pool.
    AddMana {
        /// The color of mana produced.
        color: Color,
        /// How much mana of that color is produced.
        amount: u8,
    },
    /// The controller draws `count` cards. The subject is implicit (the
    /// controller), so this effect needs no target.
    DrawCard {
        /// How many cards the controller draws.
        count: u8,
    },
}

/// The condition under which a [`Ability::Triggered`] triggers.
///
/// Each variant is evaluated by [`condition_met`] as a pure predicate over the
/// states before and after an action — never via an event listener.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    /// The source permanent entered the battlefield this transition (its
    /// [`crate::PermanentId`] is present after but not before).
    SelfEntersBattlefield,
}

/// Whether an ability is a mana ability (CR 605.1a, simplified): an activated
/// ability whose every effect adds mana. Mana abilities resolve immediately and
/// do not use the stack (see `crate::apply_action`). Derived, never stored.
#[must_use]
pub fn is_mana_ability(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::Activated { effects, .. }
            if !effects.is_empty()
                && effects.iter().all(|e| matches!(e, Effect::AddMana { .. }))
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn activated_mana_ability_round_trips() {
        let json = r#"{"type":"activated","cost":[{"kind":"tap"}],"effects":[{"kind":"add_mana","color":"green","amount":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Activated {
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddMana {
                    color: Color::Green,
                    amount: 1
                }],
            }
        );
        assert!(is_mana_ability(&ability));
    }

    #[test]
    fn triggered_etb_draw_round_trips() {
        let json = r#"{"type":"triggered","event":"self_enters_battlefield","effects":[{"kind":"draw_card","count":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }
        );
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn activated_non_mana_ability_is_not_a_mana_ability() {
        let ability = Ability::Activated {
            cost: vec![Cost::Tap],
            effects: vec![Effect::DrawCard { count: 1 }],
        };
        assert!(!is_mana_ability(&ability));
    }
}
