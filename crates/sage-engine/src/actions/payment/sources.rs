//! What a seat could tap for, and what each tap would add.
//!
//! One permanent, one entry — see the module note on why that is the load-bearing
//! decision. A permanent that taps for either of two colors is a [`SourceOptions`] with
//! two [`SourceOption`]s, and the fact that taking one retires the other is a property
//! of the *shape* rather than something the search has to remember.

use crate::ability::{Ability, Effect};
use crate::card::{abilities_of_permanent, CardDatabase};
use crate::id::{PermanentId, PlayerId};
use crate::mana::ManaPool;
use crate::state::{GameState, Permanent};

use super::super::definition::ManaSource;
use super::super::utilities::{cost_payable, tap_cost_is_summoning_sick};

/// One way to tap one permanent: which ability, and what activating it adds.
#[derive(Clone, Debug)]
pub(crate) struct SourceOption {
    /// Index into [`abilities_of_permanent`], as [`ManaSource::index`] addresses it.
    pub index: usize,
    /// What this activation puts in the pool. Read from the card's own ability — this
    /// module never decides what a source is worth, only that it is available.
    pub adds: ManaPool,
}

/// One permanent's mana options: **at most one of them can be taken.**
///
/// The constraint is the reason this type exists rather than a flat list of activations.
/// Every mana ability in the catalog costs `{T}`, so tapping the permanent for one
/// option spends the cost the other one also needed.
#[derive(Clone, Debug)]
pub(crate) struct SourceOptions {
    /// The permanent whose abilities these are.
    pub permanent: PermanentId,
    /// The options, in ability order. Never empty — a permanent with no usable mana
    /// ability is simply not listed.
    pub options: Vec<SourceOption>,
}

impl SourceOptions {
    /// The most mana any one of these options could add — the admissible bound the
    /// search prunes with. An over-estimate is required here and a under-estimate would
    /// be a bug: pruning on a bound lower than the truth would discard a payment that
    /// exists.
    pub(crate) fn best_yield(&self) -> u16 {
        self.options
            .iter()
            .map(|option| option.adds.total())
            .max()
            .unwrap_or(0)
    }

    /// The option activating `index`, if this permanent offers it.
    pub(crate) fn option(&self, index: usize) -> Option<&SourceOption> {
        self.options.iter().find(|option| option.index == index)
    }
}

/// Whether this permanent's ability may ride inside a cast as a payment source.
///
/// Two conditions, and the second is the one worth stating: it must be a mana ability
/// (CR 605.1a — no stack, no priority, so it can happen inside another process), and it
/// must not *pose a question*. `Add one mana of any color` is a mana ability that asks
/// which color, and answering that mid-cast needs a suspension point the casting
/// process has not got. Refused here rather than half-applied.
#[must_use]
pub fn is_plain_mana_source(state: &GameState, db: &CardDatabase, source: ManaSource) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == source.permanent) else {
        return false;
    };
    let Some(ability) = abilities_of_permanent(db, perm)
        .into_iter()
        .nth(source.index)
    else {
        return false;
    };
    plain_mana_yield(&ability).is_some()
}

/// What one activation of `ability` adds to a pool, or `None` if it is not an ability a
/// payment may carry.
///
/// The single place the two questions — *is this usable inside a cast* and *what is it
/// worth* — are answered together, so a source can never be offered by one rule and
/// valued by another.
pub(crate) fn plain_mana_yield(ability: &Ability) -> Option<ManaPool> {
    if !crate::ability::is_mana_ability(ability) {
        return None;
    }
    let Ability::Activated { effects, .. } = ability else {
        return None;
    };
    let mut adds = ManaPool::default();
    for effect in effects {
        match effect {
            Effect::AddMana { color, amount } => adds.add(*color, *amount),
            Effect::AddColorlessMana { amount } => adds.add_colorless(*amount),
            Effect::AddRestrictedMana {
                color,
                amount,
                restriction,
            } => adds.add_restricted(*color, *amount, restriction.clone()),
            // `Add one mana of any color` and anything else that would need an answer
            // before its yield is known. Refusing the whole ability rather than the one
            // effect is deliberate: a partially-valued ability is a wrong valuation.
            _ => return None,
        }
    }
    Some(adds)
}

/// Whether `perm`'s ability at `index` could be activated **right now**, judged by
/// exactly the conditions [`offer_activations`](super::super::generation) applies.
///
/// It deliberately does not ask [`crate::valid_actions`]: the generator calls this, so
/// that would be asking itself. The duplication is therefore structural rather than
/// careless, and it is kept honest by
/// [`payment_sources_are_all_activatable`](super::solve::tests) — a test over the real
/// catalog asserting that everything offered here is something `valid_actions` also
/// offers.
fn activation_is_available(
    state: &GameState,
    db: &CardDatabase,
    perm: &Permanent,
    ability: &Ability,
) -> bool {
    let Ability::Activated { cost, .. } = ability else {
        return false;
    };
    // CR 302.6 exempts nothing, mana abilities included: a `{T}` cost on a creature that
    // entered this turn is not activatable, so its mana is not mana this seat can make.
    !tap_cost_is_summoning_sick(state, perm, cost, db) && cost_payable(state, cost, perm)
}

/// Every permanent `player` could tap for mana right now, with the options each offers.
///
/// O(permanents × abilities) and no search (ADR 0004): it says what is *available*, and
/// deliberately not which combination would cover a given cost. That is [`super::solve`]'s
/// question, asked about a cost.
pub(crate) fn mana_options(
    state: &GameState,
    db: &CardDatabase,
    player: PlayerId,
) -> Vec<SourceOptions> {
    let mut sources = Vec::new();
    for perm in &state.battlefield {
        if perm.controller != player {
            continue;
        }
        let mut options = Vec::new();
        for (index, ability) in abilities_of_permanent(db, perm).iter().enumerate() {
            let Some(adds) = plain_mana_yield(ability) else {
                continue;
            };
            if !activation_is_available(state, db, perm, ability) {
                continue;
            }
            options.push(SourceOption { index, adds });
        }
        if !options.is_empty() {
            sources.push(SourceOptions {
                permanent: perm.id,
                options,
            });
        }
    }
    sources
}

/// Every mana source the priority holder could name in a payment right now, flattened to
/// one entry per *activation*.
///
/// The candidate set a presentation offers. Note that a permanent with two mana
/// abilities appears **twice** here, once per option, which is exactly what a client
/// needs in order to ask *which color* when a player clicks a dual land — and exactly
/// what a payment must not do twice, which [`super::apply_payment`] enforces.
#[must_use]
pub fn payment_sources(state: &GameState, db: &CardDatabase) -> Vec<ManaSource> {
    mana_options(state, db, state.priority)
        .into_iter()
        .flat_map(|source| {
            source.options.into_iter().map(move |option| ManaSource {
                permanent: source.permanent,
                index: option.index,
            })
        })
        .collect()
}
