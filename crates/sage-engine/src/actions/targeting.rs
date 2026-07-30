//! Action targeting — enumeration of legal targets per action and target slot.

use crate::ability::{Ability, Effect, Target, TargetSpec};
use crate::id::PlayerId;
use crate::resolve::target_is_legal;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::{Action, TargetRequirement};

/// The ordered target requirements of `action` against the current `state`: one
/// [`TargetRequirement`] per target slot the action must fill, each carrying the
/// legal candidate set for that slot. Empty for an action that targets nothing.
///
/// # Combinatorial guard (ADR 0009 §Enumeration)
///
/// This builds one candidate **set per slot**: its cost is the *sum* of the
/// per-slot candidate counts — O(N) for a single slot over N candidates. It
/// never forms the *cartesian product* of the slots (which would be O(Nᵏ) for k
/// slots of N candidates each), so advertising a targeted action stays linear in
/// board size per slot. This is exactly the "core complexity" ADR 0002 flagged:
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
    action_target_specs(state, db, action)
        .into_iter()
        .map(|spec| TargetRequirement {
            spec,
            candidates: legal_targets_for_spec(spec, state, db),
        })
        .collect()
}

/// The ordered [`TargetSpec`]s `action` must be given a target for — one per
/// targeting effect the action declares, in resolution order. Empty for an action
/// with no targeting effects (or one the state cannot resolve).
///
/// An [`Action::ActivateAbility`] reads its activated ability's effects; an
/// [`Action::CastSpell`] reads the cast card's cast target specs
/// ([`crate::CardData::cast_target_specs`]) — the spell-effect target slots plus, for an
/// Aura, its enchant restriction (CR 303.4a) — so a spell chooses targets exactly
/// as an ability does (CR 601.2c). Every other action targets nothing.
pub(crate) fn action_target_specs(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<TargetSpec> {
    match action {
        Action::ActivateAbility {
            permanent, index, ..
        } => {
            let Some(perm) = state.battlefield.iter().find(|p| p.id == *permanent) else {
                return Vec::new();
            };
            let abilities = crate::card::abilities_of(db, perm.card);
            let Some(Ability::Activated { effects, .. }) = abilities.get(*index) else {
                return Vec::new();
            };
            effects.iter().filter_map(Effect::target_spec).collect()
        }
        Action::CastSpell { card, .. } => db
            .card(card.card)
            .map(crate::card::CardData::cast_target_specs)
            .unwrap_or_default(),
        _ => Vec::new(),
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
    db: &CardDatabase,
) -> Vec<Target> {
    let universe: Vec<Target> = match spec {
        TargetSpec::AnyPlayer => (0..state.players.len())
            .map(|seat| Target::Player(PlayerId(seat)))
            .collect(),
        TargetSpec::AnyPermanent | TargetSpec::AnyCreature => state
            .battlefield
            .iter()
            .map(|perm| Target::Permanent(perm.id))
            .collect(),
        // "Any target" (CR 115.4): players and battlefield permanents together;
        // the `target_is_legal` filter below keeps only creatures and in-game
        // players, so a non-creature permanent never survives it.
        TargetSpec::AnyTarget => (0..state.players.len())
            .map(|seat| Target::Player(PlayerId(seat)))
            .chain(
                state
                    .battlefield
                    .iter()
                    .map(|perm| Target::Permanent(perm.id)),
            )
            .collect(),
        // Only spells on the stack are candidates — abilities are not spells, and
        // mana abilities never use the stack (CR 605.3), so neither can be a
        // "counter target spell" candidate.
        TargetSpec::SpellOnStack => state
            .stack
            .iter()
            .filter(|o| matches!(o.kind, crate::stack::StackObjectKind::Spell { .. }))
            .map(|o| Target::Spell(o.id))
            .collect(),
    };
    universe
        .into_iter()
        .filter(|&target| target_is_legal(spec, target, state, db))
        .collect()
}
