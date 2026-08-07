//! SAGE rules engine — layer 3.
//!
//! Invariants (see AGENTS.md in this crate):
//! - `GameState` is an immutable value type; `apply_action` returns a new state.
//! - No I/O, no async, no globals, no time. Pure functions only.
//! - Everything derivable is computed on demand (pull-based), never cached on objects.
//!
//! The pipeline is split along its natural seams: [`actions`] is the legality
//! authority ([`Action`], [`valid_actions`]); [`apply`] is the [`apply_action`]
//! transition and its per-action helpers; [`resolve`] resolves stack objects;
//! [`sba`] runs state-based actions; [`triggers`] collects triggers by diffing;
//! [`replacement`] modifies an event before it happens (CR 614).

mod ability;
mod actions;
mod apply;
mod automation;
mod card;
mod card_type;
mod catalog;
mod characteristics;
mod choice;
mod combat;
mod commander;
pub mod compat;
mod condition;
mod copy;
mod cost_modification;
mod delayed;
#[cfg(test)]
mod fixtures;
mod id;
mod mana;
mod mulligan;
mod phase;
mod player;
mod reflexive;
mod replacement;
mod resolve;
mod rng;
mod sba;
mod scripted;
mod setup;
mod stack;
mod state;
mod token;
mod triggers;
mod zone;

pub use ability::{
    activation_taps, group_target_counts, is_emblem_ability, is_equip_ability,
    is_graveyard_ability, is_loyalty_ability, is_mana_ability, is_sorcery_speed_ability,
    maximum_targets, minimum_targets, target_counts, Ability, ActivationTiming, ActivatorScope,
    BottomOrder, CardFilter, Chooser, Condition, Cost, CostModification, CountScope, DamageSubject,
    DerivedAmount, DestroyAffects, Effect, FoundDestination, GraveyardCardClass, GraveyardCount,
    GraveyardScope, HalvedTotal, ManaRestriction, MassAffects, ObservedActivation,
    ObservedPermanent, ObservedSpell, OptionalCost, PermanentAmount, PermanentCount,
    PlayerModification, PlayerRef, SacrificeCount, StaticAffects, StaticCondition,
    StaticModification, Target, TargetCount, TargetGroup, TargetSpec, TriggerCondition,
    TriggerStep, TurnScope,
};
pub use actions::{
    activation_discard_cost, activation_exile_cost, activation_sacrifice_cost,
    auto_activation_payment, auto_graveyard_activation_payment, auto_payment, discard_cost,
    graveyard_activation_exile_cost, is_plain_mana_source, mana_ability_pips, mode_options,
    payment_pips, payment_sources, remaining_cost_pips, sacrifice_cost, target_requirements,
    total_cast_cost, valid_actions, x_options, Action, Attack, Block, CostPayment, DamageOrder,
    DiscardCost, ExileCost, ManaSource, ModeOption, PaymentPip, SacrificeCost, TargetRequirement,
    XOption,
};
pub use apply::apply_action;
pub use automation::{forced_declaration_without_choice, priority_has_no_meaningful_action};
pub use card::{
    abilities_of, abilities_of_face, abilities_of_permanent, equip_ability, AdditionalCost,
    Attachment, AttachmentKind, BackFace, CardData, CardDatabase, CatalogError, CombatRestriction,
    DamageCharacteristic, Face, Keyword, Printing, PrintingDatabase, Rarity, RuleModification,
    SpellMode, SpellTrait, SCHEMA_VERSION,
};
pub use card_type::{CardType, Supertype};
pub use catalog::{Violation, MAX_MODES};
pub use characteristics::{
    assigns_combat_damage_by, attacks_as_though_no_defender, characteristics, controller_of,
    controller_of_id, Characteristics,
};
pub use choice::{
    choice_bounds, choice_candidates, choice_looked_at, confirm_is_payable, named_card_candidates,
    order_candidates, pending_player_choice, permanent_choice_bounds, permanent_choice_candidates,
    CardNameRequest, ChoiceOutcome, ChoiceQuestion, ChoiceRequest, ChoiceZone, ColorOutcome,
    ColorRequest, ConfirmRequest, CopyChoiceOutcome, CopyChoiceRequest, DeclineOutcome,
    NamedCardClass, OrderRequest, PendingChoice, PermanentOutcome, PermanentRequest,
    PlayCardRequest, PlayZone, ReplacementRequest, Resume, SuspendedSpell,
};
pub use combat::{
    attack_target_of, attacked_players, attacker_candidates, attackers_needing_damage_order,
    attacking_defender_of, attacking_taps, block_requirements, blocked_by_at_most_one,
    blocker_can_block_attacker, blocker_candidates, blocker_candidates_for, blocks_allowed,
    declared_attackers, defender_candidates, defending_player, defending_player_candidates,
    max_block_requirements_met, must_be_blocked_by_all_able, pending_blocker_declarer,
    pending_damage_order, permanent_has_menace, permanent_has_restriction, permanent_restrictions,
    summoning_sickness_restricts, AttackTarget,
};
pub use commander::{
    commander_tax_cost, CommanderState, COMMANDER_DAMAGE_LOSS_THRESHOLD, COMMANDER_TAX_PER_CAST,
};
pub use copy::{copiable_face, copy_choice_candidates, CopiedValues, CopyClass, CopySubject};
pub use delayed::{DelayedCondition, DelayedTrigger, PendingDelayedTrigger};
pub use id::{
    CardId, CardInstance, CardInstanceId, FunctionalId, FunctionalIdError, OracleId, PermanentId,
    PlayerId,
};
pub use mana::{
    parse_mana_cost, x_pip_count, Color, ManaCost, ManaPool, RestrictedMana, SpendPurpose,
};
pub use mulligan::{bottom_requirement, BottomRequirement, MulliganState, PlayerMulligan};
pub use phase::Step;
pub use player::{
    casts_from_hand_without_paying, maximum_hand_size, over_hand_size, plays_lands_from_graveyard,
    LossReason, Player, MAX_HAND_SIZE, STARTING_LIFE,
};
pub use reflexive::{PendingReflexive, ReflexiveCondition, ReflexiveTrigger};
pub use replacement::{
    pending_replacement_options, DamageFilter, DamageRecipient, EnteringFilter, EnteringObject,
    OfferedReplacement, PendingDamage, PendingEntry, PendingReplacement, ReplacementEffect,
    ReplacementOption,
};
pub use resolve::Resolution;
pub use scripted::scripted_rules_text;
pub use setup::{
    GameSetup, PlayerSetup, SetupError, DEFAULT_STARTING_HAND_SIZE, DEFAULT_STARTING_LIFE,
};
pub use stack::{
    AbilityOrigin, AbilitySource, PaidCost, SpellTraitKind, StackId, StackObject, StackObjectKind,
};
pub use state::{
    CommanderDamage, CounterKind, DamageTarget, Duration, EffectAffects, Emblem, ExilePlaying,
    GameEvent, GameLogEntry, GameResult, GameState, GraveyardCasting, IgnoringHexproof,
    LoggedIdentity, LoggedPermanent, Modification, Permanent, StaticEffect,
};
pub use token::{Printed, PrintedFace, TokenData};
pub use triggers::{collect_triggers, pending_trigger_target_choice, Trigger};
pub use zone::Zone;
