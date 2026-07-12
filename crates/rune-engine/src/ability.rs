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

use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::mana::Color;
use crate::stack::StackId;
use crate::state::CounterKind;

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
    /// A **self-replacement** (CR 614.1c): this permanent enters the battlefield
    /// **tapped** (e.g. a tapped dual land). Unlike a triggered ability it changes
    /// nothing after the fact — it modifies the enters-the-battlefield event itself,
    /// so the permanent is tapped the instant it is on the battlefield, before any
    /// state-based action or enters-the-battlefield trigger is observed (CR 614.12).
    /// Applied at the battlefield-entry seam ([`crate::card::apply_enters_replacements`]),
    /// not as a post-action pipeline stage. Deserialized as `{"type":"enters_tapped"}`.
    EntersTapped,
    /// A **self-replacement** (CR 614.1c): this permanent enters the battlefield
    /// with `count` counters of `counter` already on it (CR 614.12) — e.g. a 0/0 that
    /// enters with two `+1/+1` counters. The counters are part of *entering*: they are
    /// present before state-based actions run, so such a creature is never a 0/0 on the
    /// battlefield and survives the CR 704.5f toughness check. Like [`Self::EntersTapped`]
    /// it is applied at the entry seam, and the co-entering ETB trigger observes the
    /// replaced state (CR 614.12). Deserialized as
    /// `{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}`.
    EntersWithCounters {
        /// The kind of counter placed as the permanent enters. Named `counter` on
        /// the wire because the enum already reserves the `type` tag for its own
        /// discriminant.
        counter: CounterKind,
        /// How many counters of that kind the permanent enters with.
        count: u32,
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
    /// Tap the single permanent this effect targets (e.g. `Tap target
    /// creature.`).
    ///
    /// Unlike [`Effect::AddMana`]/[`Effect::DrawCard`], whose subject is the
    /// controller, this effect names an explicit subject. The `target` field is
    /// the [`TargetSpec`] constraining what may be chosen; the *chosen* value is
    /// a [`Target`] recorded on the [`crate::StackObject`] when the ability is
    /// put on the stack (CR 601.2c) and re-checked against current state on
    /// resolution (CR 608.2b — see the resolve path).
    Tap {
        /// What this effect is allowed to target.
        target: TargetSpec,
    },
    /// Counter the single spell on the stack this effect targets (CR 701.5a):
    /// on resolution the targeted spell is removed from the stack without
    /// resolving and put into its owner's graveyard. The first counterspell.
    ///
    /// Like [`Effect::Tap`], the subject is an explicit target rather than the
    /// controller: `target` is the [`TargetSpec`] (a [`TargetSpec::SpellOnStack`])
    /// constraining what may be chosen, the *chosen* value is a [`Target::Spell`]
    /// recorded on the [`crate::StackObject`] at cast (CR 601.2c) and re-checked on
    /// resolution (CR 608.2b) — a spell whose target already resolved fizzles.
    CounterSpell {
        /// What this effect is allowed to target (a spell on the stack).
        target: TargetSpec,
    },
    /// Deal `amount` damage to the single target this effect names (CR 120.3).
    ///
    /// The subject is an explicit target (like [`Effect::Tap`]), chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). Damage to a creature
    /// is *marked* on it (CR 120.3d) for the lethal-damage state-based action to
    /// read (CR 704.5g); damage to a player is *lost life* (CR 120.3a), feeding
    /// the zero-life state-based action (CR 704.5a). Damage prevention/replacement
    /// and deathtouch are not modeled.
    DealDamage {
        /// What this effect is allowed to target (a creature, a player, or — for
        /// a burn spell — [`TargetSpec::AnyTarget`]).
        target: TargetSpec,
        /// How much damage is dealt.
        amount: u32,
    },
    /// Destroy the single permanent this effect targets (CR 701.7): it is put
    /// into its owner's graveyard, the same graveyard path as lethal damage
    /// (CR 704.5g). Regeneration and other destruction-replacement effects are
    /// out of scope.
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b) — a destroy whose
    /// target has already left fizzles.
    Destroy {
        /// What this effect is allowed to target (typically a creature).
        target: TargetSpec,
    },
    /// The referenced player gains `amount` life (CR 119.3). The subject is a
    /// non-targeted [`PlayerRef`] (like [`Effect::DrawCard`]'s implicit
    /// controller), so this effect chooses no target.
    GainLife {
        /// Which player gains the life.
        player_ref: PlayerRef,
        /// How much life is gained.
        amount: u32,
    },
    /// The referenced player loses `amount` life (CR 119.3). The subject is a
    /// non-targeted [`PlayerRef`]; life loss can drive the zero-life state-based
    /// action (CR 704.5a). This effect chooses no target.
    LoseLife {
        /// Which player loses the life.
        player_ref: PlayerRef,
        /// How much life is lost.
        amount: u32,
    },
    /// Put `count` counters of `kind` on the single permanent this effect targets
    /// (CR 122). Both `+1/+1` and `-1/-1` kinds are supported; they fold into the
    /// permanent's computed power/toughness (CR 613.7c) on demand, so a `-1/-1`
    /// counter can lower toughness to at or below marked damage and let the
    /// lethal-damage state-based action destroy it (CR 704.5g).
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b).
    PutCounters {
        /// What this effect is allowed to target (a permanent that can bear
        /// counters).
        target: TargetSpec,
        /// The kind of counter to place. Named `counter` on the wire because the
        /// effect enum already reserves the `kind` tag for its own discriminant.
        counter: CounterKind,
        /// How many counters of that kind to place.
        count: u32,
    },
    /// Give the single creature this effect targets `+power`/`+toughness`
    /// **until end of turn** — the pump-spell verb (e.g. `Target creature gets
    /// +3/+3 until end of turn.`). On resolution it adds a timestamped CR 613
    /// layer-7c power/toughness modifier that the cleanup step removes (CR 514.2).
    ///
    /// Like [`Effect::Tap`] the subject is an explicit target, chosen at cast
    /// (CR 601.2c) and re-checked on resolution (CR 608.2b). The amounts are
    /// signed, so a negative value is a shrink; the modifier folds into computed
    /// power/toughness on demand (CR 613.7c), after counters and in timestamp
    /// order, so two pumps in a turn stack and both wear off at cleanup.
    Pump {
        /// What this effect is allowed to target (a creature).
        target: TargetSpec,
        /// The signed amount added to the target's power until end of turn.
        power: i32,
        /// The signed amount added to the target's toughness until end of turn.
        toughness: i32,
    },
}

/// A **non-targeted player reference**: which player an implicit-subject effect
/// (e.g. [`Effect::GainLife`]) acts on, without that player being a *target*
/// (CR 115.1 — no target is chosen, so these effects never fizzle).
///
/// A closed, plain-data enum deserialized from a bare `snake_case` tag, e.g.
/// `{"kind": "gain_life", "player_ref": "controller", "amount": 3}`. It grows by
/// adding variants (each opponent, target's controller, …) as effects need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRef {
    /// The controller of the spell or ability producing the effect ("you").
    Controller,
}

impl Effect {
    /// The [`TargetSpec`] this effect must be given a chosen target for, or
    /// `None` for an effect with an implicit subject ([`Effect::AddMana`],
    /// [`Effect::DrawCard`]).
    ///
    /// The resolution path uses this to pair each of an object's stored
    /// [`Target`]s with the effect that consumes it and to re-check that
    /// target's legality (CR 608.2b). Kept exhaustive so a new targeting
    /// [`Effect`] variant must declare its spec here.
    #[must_use]
    pub fn target_spec(&self) -> Option<TargetSpec> {
        match self {
            Effect::Tap { target }
            | Effect::CounterSpell { target }
            | Effect::DealDamage { target, .. }
            | Effect::Destroy { target }
            | Effect::PutCounters { target, .. }
            | Effect::Pump { target, .. } => Some(*target),
            Effect::AddMana { .. }
            | Effect::DrawCard { .. }
            | Effect::GainLife { .. }
            | Effect::LoseLife { .. } => None,
        }
    }
}

/// A **target spec**: what an [`Effect`] is allowed to target, authored as card
/// data alongside the rest of the IR (CR 115.1 "target … as defined by the
/// spell or ability").
///
/// This is a declaration, not a chosen value: it names a *class* of legal
/// objects, while a [`Target`] names one specific object the player picked. The
/// engine turns a spec into the concrete legal set on demand (enumeration is
/// issue #71) and re-checks a chosen [`Target`] against it on resolution.
///
/// A closed, plain-data enum (no closures — ADR 0007) deserialized from a bare
/// string tag, e.g. `{"kind": "tap", "target": "any_creature"}`. It grows by
/// adding variants (any permanent of a type, an object in a named zone, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSpec {
    /// Any player in the game.
    AnyPlayer,
    /// Any permanent on the battlefield.
    AnyPermanent,
    /// Any creature on the battlefield (a permanent whose printed types include
    /// [`crate::CardType::Creature`]).
    AnyCreature,
    /// Any spell on the stack — a [`crate::StackObjectKind::Spell`] object (CR
    /// 701.5, "counter target spell"). Abilities on the stack are not spells and
    /// are never candidates; a mana ability never uses the stack at all (CR
    /// 605.3), so it can never be countered.
    SpellOnStack,
    /// Any target (CR 115.4): the modern "any target" of a burn spell — any
    /// creature on the battlefield or any player still in the game. Planeswalkers
    /// and battles are not modeled, so the legal set is exactly creatures plus
    /// players.
    AnyTarget,
}

/// A **chosen target**: a resolved reference to one specific game object the
/// player aimed a spell or ability at (CR 601.2c).
///
/// Names a specific instance/permanent/player by its per-game identity, never a
/// bare printed [`crate::CardId`] — two copies of one printing must stay
/// distinguishable (per-instance identity, issue #51). Stored on the
/// [`crate::StackObject`] and re-checked against its [`TargetSpec`] on
/// resolution; this value type is the one issue #71 will also carry on a
/// parameterized `Action`.
///
/// Plain `Copy`/`Eq` data with no closures, so [`crate::GameState`] keeps its
/// value semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A specific player, by seat.
    Player(PlayerId),
    /// A specific permanent on the battlefield, by its battlefield identity.
    Permanent(PermanentId),
    /// A specific physical card, by its per-game instance identity (for targets
    /// that name a card in a zone rather than a permanent).
    Card(CardInstanceId),
    /// A specific object on the stack, by its [`StackId`] (for targets that name
    /// a spell on the stack, e.g. a counterspell — CR 701.5).
    Spell(StackId),
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
    /// The source permanent **died** this transition: it left the battlefield for
    /// a graveyard (CR 700.4, the "dies" event of CR 603.6c). Observed by diff —
    /// its [`crate::PermanentId`] is present before but not after, and its physical
    /// instance is now in a graveyard it was not in before. A leave to any
    /// non-graveyard zone does not satisfy this, so a future bounce or exile never
    /// fires it. Fires from any cause (lethal damage, `Destroy`, or combat), all
    /// through the one leaves-battlefield seam
    /// ([`crate::GameState::move_permanent_to_graveyard`]).
    SelfDies,
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
    fn issue_151_triggered_dies_draw_round_trips() {
        // The dies trigger authors its condition as the bare `self_dies` tag
        // (CR 700.4 / 603.6c) and reuses the draw effect.
        let json = r#"{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::Triggered {
                event: TriggerCondition::SelfDies,
                effects: vec![Effect::DrawCard { count: 1 }],
            }
        );
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_155_enters_tapped_replacement_round_trips() {
        // The "enters tapped" self-replacement (CR 614.1c) authors as the bare
        // `enters_tapped` type tag and is not a mana ability.
        let json = r#"{"type":"enters_tapped"}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(ability, Ability::EntersTapped);
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_155_enters_with_counters_replacement_round_trips() {
        // The "enters with N counters" self-replacement (CR 614.12) authors its
        // counter kind under `counter` (the enum reserves `type` for its tag) and
        // its count as data.
        let json = r#"{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}"#;
        let ability: Ability = serde_json::from_str(json).unwrap();
        assert_eq!(
            ability,
            Ability::EntersWithCounters {
                counter: CounterKind::PlusOnePlusOne,
                count: 2,
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

    #[test]
    fn tap_effect_round_trips_with_its_target_spec() {
        // The target spec is authored as a bare string tag on the effect.
        let json = r#"{"kind":"tap","target":"any_creature"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::Tap {
                target: TargetSpec::AnyCreature,
            }
        );
    }

    #[test]
    fn target_spec_variants_deserialize_from_bare_strings() {
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""any_player""#).unwrap(),
            TargetSpec::AnyPlayer
        );
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""any_permanent""#).unwrap(),
            TargetSpec::AnyPermanent
        );
    }

    #[test]
    fn only_targeting_effects_report_a_target_spec() {
        // A targeting effect exposes its spec; implicit-subject effects do not.
        assert_eq!(
            Effect::Tap {
                target: TargetSpec::AnyPermanent,
            }
            .target_spec(),
            Some(TargetSpec::AnyPermanent)
        );
        assert_eq!(Effect::DrawCard { count: 1 }.target_spec(), None);
        assert_eq!(
            Effect::AddMana {
                color: Color::Green,
                amount: 1
            }
            .target_spec(),
            None
        );
    }

    #[test]
    fn counter_spell_effect_round_trips_with_its_target_spec() {
        // The counterspell effect authors its spec as a bare string tag, and only
        // it (a targeting effect) reports a spec (CR 701.5).
        let json = r#"{"kind":"counter_spell","target":"spell_on_stack"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::CounterSpell {
                target: TargetSpec::SpellOnStack,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::SpellOnStack));
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""spell_on_stack""#).unwrap(),
            TargetSpec::SpellOnStack
        );
    }

    #[test]
    fn a_tap_effect_is_not_a_mana_ability() {
        let ability = Ability::Activated {
            cost: vec![Cost::Tap],
            effects: vec![Effect::Tap {
                target: TargetSpec::AnyCreature,
            }],
        };
        assert!(!is_mana_ability(&ability));
    }

    #[test]
    fn issue_149_deal_damage_round_trips_with_its_target_spec() {
        let json = r#"{"kind":"deal_damage","target":"any_target","amount":2}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::DealDamage {
                target: TargetSpec::AnyTarget,
                amount: 2,
            }
        );
        // A targeting effect reports its spec; the "any target" spec deserializes
        // from its bare string tag.
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyTarget));
        assert_eq!(
            serde_json::from_str::<TargetSpec>(r#""any_target""#).unwrap(),
            TargetSpec::AnyTarget
        );
    }

    #[test]
    fn issue_149_destroy_round_trips_with_its_target_spec() {
        let json = r#"{"kind":"destroy","target":"any_creature"}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::Destroy {
                target: TargetSpec::AnyCreature,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
    }

    #[test]
    fn issue_149_put_counters_round_trips_with_both_kinds() {
        // The counter kind is authored under `counter` (the enum reserves `kind`
        // for its own tag) and deserializes from a snake_case string.
        let plus = r#"{"kind":"put_counters","target":"any_creature","counter":"plus_one_plus_one","count":1}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(plus).unwrap(),
            Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::PlusOnePlusOne,
                count: 1,
            }
        );
        let minus = r#"{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":2}"#;
        assert_eq!(
            serde_json::from_str::<Effect>(minus).unwrap(),
            Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::MinusOneMinusOne,
                count: 2,
            }
        );
    }

    #[test]
    fn issue_150_pump_round_trips_with_its_target_spec() {
        // The pump verb authors its target spec and signed P/T amounts as card
        // data, and (a targeting effect) reports its spec.
        let json = r#"{"kind":"pump","target":"any_creature","power":3,"toughness":3}"#;
        let effect: Effect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect,
            Effect::Pump {
                target: TargetSpec::AnyCreature,
                power: 3,
                toughness: 3,
            }
        );
        assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
    }

    #[test]
    fn issue_149_life_effects_round_trip_and_target_nothing() {
        let gain = r#"{"kind":"gain_life","player_ref":"controller","amount":3}"#;
        let gain: Effect = serde_json::from_str(gain).unwrap();
        assert_eq!(
            gain,
            Effect::GainLife {
                player_ref: PlayerRef::Controller,
                amount: 3,
            }
        );
        let lose = r#"{"kind":"lose_life","player_ref":"controller","amount":2}"#;
        let lose: Effect = serde_json::from_str(lose).unwrap();
        assert_eq!(
            lose,
            Effect::LoseLife {
                player_ref: PlayerRef::Controller,
                amount: 2,
            }
        );
        // Life gain/loss have an implicit subject, so they choose no target.
        assert_eq!(gain.target_spec(), None);
        assert_eq!(lose.target_spec(), None);
    }
}
