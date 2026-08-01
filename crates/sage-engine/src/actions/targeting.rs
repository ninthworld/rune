//! Action targeting — enumeration of legal targets per action and target slot.

use crate::ability::{Ability, Effect, Target, TargetGroup, TargetSpec};
use crate::id::PlayerId;
use crate::resolve::target_is_legal;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::{Action, TargetRequirement};

/// The ordered target requirements of `action` against the current `state`: one
/// [`TargetRequirement`] per target slot the action must fill, each carrying the
/// legal candidate set for that slot. Empty for an action that targets nothing.
///
/// # Combinatorial guard (ADR 0004 §Enumeration)
///
/// This builds one candidate **set per slot**: its cost is the *sum* of the
/// per-slot candidate counts — O(N) for a single slot over N candidates. It
/// never forms the *cartesian product* of the slots (which would be O(Nᵏ) for k
/// slots of N candidates each), so advertising a targeted action stays linear in
/// board size per slot. This is exactly the "core complexity" ADR 0001 flagged:
/// legal-set enumeration must be per-slot, not per-combination. A caller
/// assembles a concrete selection by picking one candidate from each slot; the
/// engine validates that assembled selection in [`crate::apply_action`] without ever
/// materializing the product.
#[must_use]
pub fn target_requirements(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<TargetRequirement> {
    // Every spec is resolved relative to the player who would take this action
    // ([`acting_player`]). That is what makes "target creature you control"
    // enumerate *their* creatures.
    let controller = acting_player(state, action);
    action_target_groups(state, db, action)
        .into_iter()
        // One group becomes one slot per target it may take, so the client answers a
        // slot at a time whether the effect wants one target or two. The slots past the
        // group's minimum are the ones the player may leave empty — "up to two target
        // creatures" is one required slot's worth of nothing and two optional ones.
        .flat_map(|group| {
            let candidates = legal_targets_for_spec(group.spec, state, controller, db);
            (0..group.max).map(move |index| TargetRequirement {
                spec: group.spec,
                optional: index >= group.min,
                candidates: candidates.clone(),
            })
        })
        .collect()
}

/// The ordered [`TargetGroup`]s `action` must be given targets for — one per
/// targeting effect the action declares, in resolution order. Empty for an action
/// with no targeting effects (or one the state cannot resolve).
///
/// An [`Action::ActivateAbility`] reads its activated ability's effects; an
/// [`Action::CastSpell`] reads the cast card's cast target groups
/// ([`crate::CardData::cast_target_groups`]) — the spell-effect target slots plus, for an
/// Aura, its enchant restriction (CR 303.4a) — so a spell chooses targets exactly
/// as an ability does (CR 601.2c). Every other action targets nothing.
pub(crate) fn action_target_groups(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<TargetGroup> {
    match action {
        Action::ActivateAbility {
            permanent, index, ..
        } => {
            let Some(perm) = state.battlefield.iter().find(|p| p.id == *permanent) else {
                return Vec::new();
            };
            let abilities = crate::card::abilities_of_permanent(db, perm);
            let Some(Ability::Activated { effects, .. }) = abilities.get(*index) else {
                return Vec::new();
            };
            effects.iter().filter_map(Effect::target_group).collect()
        }
        Action::CastSpell { card, .. } => db
            .card(card.card)
            .map(crate::card::CardData::cast_target_groups)
            .unwrap_or_default(),
        // A trigger's slots are the target groups of the effects it carries on the
        // stack — read from the object itself, since a triggered ability's effects
        // were copied there when it triggered and are what will resolve.
        Action::ChooseTriggerTargets { ability, .. } => state
            .stack
            .iter()
            .find(|o| o.id == *ability)
            .map(|o| match &o.kind {
                crate::stack::StackObjectKind::Ability { effects, .. } => {
                    effects.iter().filter_map(Effect::target_group).collect()
                }
                crate::stack::StackObjectKind::Spell { .. } => Vec::new(),
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The player whose seat `action` is taken from, and therefore the frame of
/// reference every possessive [`TargetSpec`] is evaluated in.
///
/// Ordinarily this is the priority holder — the player taking the action, by
/// definition (CR 117.1). The exception is aiming a triggered ability: the *trigger's
/// controller* chooses, and while one is owed the engine has already handed them
/// priority, so the two agree. Reading it off the stack object anyway keeps the
/// answer true of the action rather than of the moment.
pub(crate) fn acting_player(state: &GameState, action: &Action) -> PlayerId {
    match action {
        Action::ChooseTriggerTargets { ability, .. } => {
            crate::triggers::controller_of_stack_object(state, *ability).unwrap_or(state.priority)
        }
        _ => state.priority,
    }
}

/// The set of [`Target`]s legal for `spec` against current `state`, as a single
/// O(N) pass over the candidate universe the spec names.
///
/// Defined *in terms of* [`target_is_legal`] — the candidate universe is filtered
/// by that same predicate — so an object is in this set exactly when it would
/// pass the resolution-time re-check. Building this list is the per-slot cost the
/// combinatorial guard on [`target_requirements`] bounds; nothing here multiplies
/// slots together.
pub(crate) fn legal_targets_for_spec(
    spec: TargetSpec,
    state: &GameState,
    controller: PlayerId,
    db: &CardDatabase,
) -> Vec<Target> {
    // The candidate *universe* is the coarse zone a spec draws from; the fine
    // restrictions (type, controller, tapped-ness, in-game-ness) are all applied by
    // the `target_is_legal` filter below. Only the zone is decided here, so no
    // restriction is ever written twice — and one written only here would be a
    // restriction the resolution-time re-check does not enforce.
    let players: Vec<Target> = (0..state.players.len())
        .map(|seat| Target::Player(PlayerId(seat)))
        .collect();
    let permanents: Vec<Target> = state
        .battlefield
        .iter()
        .map(|perm| Target::Permanent(perm.id))
        .collect();
    let universe: Vec<Target> = match spec {
        TargetSpec::AnyPlayer | TargetSpec::AnyOpponent => players,
        // "Target player or planeswalker": both universes, filtered below to players
        // still in the game and permanents that are planeswalkers.
        TargetSpec::AnyPlayerOrPlaneswalker => players.into_iter().chain(permanents).collect(),
        TargetSpec::AnyPermanent
        | TargetSpec::AnyNonlandPermanent
        | TargetSpec::AnyCreature
        | TargetSpec::AnyCreatureYouControl
        | TargetSpec::AnyCreatureAnOpponentControls
        | TargetSpec::AnyCreatureWithFlying
        | TargetSpec::AnyTappedCreature
        | TargetSpec::AnyArtifact
        | TargetSpec::AnyEnchantment
        | TargetSpec::AnyArtifactOrEnchantment
        | TargetSpec::AnyArtifactEnchantmentOrCreatureWithFlying
        | TargetSpec::AnyLand => permanents,
        // A graveyard is public, so its cards are enumerable exactly as the battlefield
        // is — the universe is the choosing player's own graveyard, and the
        // `target_is_legal` filter below keeps only the creature cards within the mana
        // value the spec names.
        TargetSpec::CreatureCardInYourGraveyard { .. } => state
            .players
            .get(controller.0)
            .map(|player| {
                player
                    .graveyard
                    .iter()
                    .map(|card| Target::Card(card.id))
                    .collect()
            })
            .unwrap_or_default(),
        // "Any target" (CR 115.4): players and battlefield permanents together; the
        // `target_is_legal` filter below keeps only creatures, planeswalkers, and
        // in-game players, so an artifact or a land never survives it.
        TargetSpec::AnyTarget => players.into_iter().chain(permanents).collect(),
        // Only spells on the stack are candidates — abilities are not spells, and
        // mana abilities never use the stack (CR 605.3), so neither can be a
        // "counter target spell" candidate.
        TargetSpec::SpellOnStack | TargetSpec::CreatureSpellOnStack => state
            .stack
            .iter()
            .filter(|o| matches!(o.kind, crate::stack::StackObjectKind::Spell { .. }))
            .map(|o| Target::Spell(o.id))
            .collect(),
    };
    universe
        .into_iter()
        .filter(|&target| target_is_legal(spec, target, state, controller, db))
        .collect()
}
