//! Delayed triggered abilities (CR 603.7): an ability created now that fires later.
//!
//! Every other triggered ability in the engine is printed on an object and read off it
//! while that object is there — the trigger collector walks the battlefield, the emblems,
//! and the graveyards, and asks each ability what it is watching for. A delayed trigger
//! is none of those. It is **created during a resolution** (CR 603.7a), it belongs to no
//! object anyone can point at, and CR 603.7e is explicit that it fires whether or not the
//! thing that created it still exists. `When you next cast an instant or sorcery spell
//! this turn, copy that spell` outlives the sorcery that said it by design: the sorcery
//! is in a graveyard before the ability has anything to do.
//!
//! So it is a **fourth source list**, stored on the state, in the shape
//! [`PendingReplacement`](crate::PendingReplacement) already established for the other
//! thing an ability can leave behind:
//!
//! - **One-shot.** `the next time` is spent by firing (CR 603.7b), so firing removes it.
//! - **One turn.** `this turn` lapses at the turn boundary, which clears the list — the
//!   same boundary that clears every other per-turn record, so neither half of
//!   `the next … this turn` is a duration vocabulary.
//! - **Its own timestamp is irrelevant.** Nothing orders delayed triggers against each
//!   other; they are collected after the printed ones and pushed in list order, which is
//!   the engine's deterministic default for simultaneous triggers.
//!
//! ## What "that spell" is
//!
//! The one condition this vocabulary can express watches a **cast**, and the ability it
//! creates acts on the spell it just watched. That spell is not chosen by anybody — the
//! trigger event fixed it — so the fired ability arrives on the stack with its slot
//! *already filled* ([`Trigger::targets`](crate::triggers::Trigger)) and its controller is
//! never asked. The engine models the reference as a filled slot rather than as a second
//! kind of reference because the two need identical treatment on resolution: CR 603.7c
//! says a delayed ability does not affect an object that has left the zone it was
//! expected in, and the CR 608.2b re-check every stack object already runs says exactly
//! that for a spell that has been countered or has resolved.

use serde::Deserialize;

use crate::ability::{Effect, ObservedSpell, Target};
use crate::id::PlayerId;
use crate::stack::StackObjectKind;
use crate::state::GameState;

/// A delayed triggered ability as a card **authors** it (CR 603.7) — the
/// `When you next … , …` an effect creates.
///
/// Plain data, exactly as [`Ability::Triggered`](crate::Ability) is, and deliberately its
/// own type rather than a reuse: the conditions a *delayed* ability can watch are a
/// different, much smaller set than the ones a printed ability watches, and letting a
/// card write `{"event":"self_dies"}` here would author an ability with no self to watch.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelayedTrigger {
    /// What it waits for.
    pub event: DelayedCondition,
    /// What it does when it fires.
    pub effects: Vec<Effect>,
}

/// What a [`DelayedTrigger`] waits for.
///
/// One variant, and it grows by adding more. Every member of this set is a *`the next
/// time`* condition — that is what makes the ability one-shot without any card saying so.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelayedCondition {
    /// The controller **casts their next spell** of the named class (CR 601), e.g.
    /// `When you next cast an instant or sorcery spell this turn`.
    ///
    /// Observed on the stack rather than in the event log, because the ability the
    /// condition creates needs the spell *object* and not merely the fact of a cast: an
    /// object pushed by this transition that is a spell its controller cast is exactly
    /// the thing "that spell" names.
    NextSpellCast(ObservedSpell),
}

/// A delayed triggered ability waiting for its event (CR 603.7a) — stored state for the
/// rest of the turn.
///
/// The shape of [`PendingReplacement`](crate::PendingReplacement) and for the same
/// reasons: raw stored state nothing else in [`GameState`] could recover (ADR 0005 §1),
/// kept as a list because two can wait at once, and carrying the [`turn`](Self::turn) it
/// was created on rather than a duration to tick down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingDelayedTrigger {
    /// Identity, minted from [`GameState::mint_id`](crate::GameState) — what firing
    /// removes, and stable across the transitions it waits through.
    pub id: u64,
    /// The player who controls it. For a delayed ability created by a resolving spell
    /// that is the player who controlled the spell as it resolved (CR 603.7d), and for
    /// one created by a resolving ability, its controller (CR 603.7e).
    pub controller: PlayerId,
    /// What it waits for, and what it does.
    pub trigger: DelayedTrigger,
    /// The turn it was created on; it lapses when that turn ends.
    pub turn: u32,
}

/// Every delayed trigger whose event happened across this transition, paired with the
/// [`Trigger`](crate::triggers::Trigger) it fires — and with its own id, so the caller
/// can spend it (CR 603.7b: `the next time`, once).
///
/// A sibling of [`collect_triggers`](crate::triggers::collect_triggers) rather than a
/// branch inside it, because the source list is not an object and the condition
/// vocabulary is not [`TriggerCondition`](crate::TriggerCondition). It stays pure — the
/// removal is the caller's, at the one place triggers reach the stack — which is the same
/// division the replacement layer makes between deriving what applies and applying it.
///
/// Read from `before`, so a delayed trigger created by *this* transition cannot fire on an
/// event the same transition already produced (CR 603.7a).
#[must_use]
pub(crate) fn delayed_triggers_fired(
    before: &GameState,
    after: &GameState,
    db: &crate::CardDatabase,
) -> Vec<(u64, crate::triggers::Trigger)> {
    let mut fired = Vec::new();
    for pending in &before.delayed_triggers {
        let DelayedCondition::NextSpellCast(observes) = pending.trigger.event;
        // CR 603.7b: only the *next* one. The first match spends the ability, and a
        // second spell cast by the same transition finds nothing left to fire.
        let Some(spell) = cast_spells(before, after).find(|object| {
            object.controller == pending.controller && matches(observes, object, db)
        }) else {
            continue;
        };
        fired.push((
            pending.id,
            crate::triggers::Trigger {
                source: crate::stack::AbilitySource::DelayedAbility,
                controller: pending.controller,
                effects: pending.trigger.effects.clone(),
                // "That spell", fixed by the event rather than chosen (CR 603.7c).
                targets: vec![Target::Spell(spell.id)],
            },
        ));
    }
    fired
}

/// The **spells** put on the stack by this transition: present in `after`, absent from
/// `before`. A [`StackId`](crate::StackId) is minted once and never reused, so its
/// presence is the whole test — the same diff the activation condition uses one variant
/// over.
fn cast_spells<'a>(
    before: &GameState,
    after: &'a GameState,
) -> impl Iterator<Item = &'a crate::stack::StackObject> {
    let ids: Vec<crate::stack::StackId> = before.stack.iter().map(|object| object.id).collect();
    after
        .stack
        .iter()
        .filter(move |object| !ids.contains(&object.id))
        .filter(|object| matches!(object.kind, StackObjectKind::Spell { .. }))
}

/// Whether the stack object `object` is a spell of the class `observes` names.
///
/// A **copy** of a spell is never one: it was not cast (CR 707.10), so it is not a
/// [`StackObjectKind::Spell`] at all and the filter above has already dropped it. That is
/// the whole of "a copy does not trigger a cast watcher", stated by the object model
/// rather than by a condition anyone has to remember.
fn matches(
    observes: ObservedSpell,
    object: &crate::stack::StackObject,
    db: &crate::CardDatabase,
) -> bool {
    let StackObjectKind::Spell { card, .. } = object.kind else {
        return false;
    };
    // The one predicate every cast watcher asks (`spell_matches_class`), so a delayed
    // ability and a printed cast trigger cannot disagree about what a class contains. A
    // delayed ability has no source permanent, so there is no colour it could have named
    // as it entered — `None` makes that class unsatisfiable rather than wrong.
    crate::card::spell_matches_class(db, card.card, observes, None)
}
