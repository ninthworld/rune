//! Reflexive triggered abilities (CR 603.11): an ability created *by* a resolution that
//! fires on something that resolution just did.
//!
//! `You may put a creature card from among them onto the battlefield. … **When a creature
//! is put onto the battlefield this way**, it deals damage equal to its power to target
//! creature an opponent controls.`
//!
//! It is a near sibling of the delayed triggered ability ([`crate::delayed`]) and differs
//! in the two ways that matter:
//!
//! - **It fires now, or never.** A delayed ability waits for a later event and outlives
//!   the object that made it (CR 603.7e); a reflexive one watches something inside the
//!   resolution that created it, so it is created, fires, and is gone inside a single
//!   transition. That is why the pending list here is *drained* where the delayed one is
//!   searched — nothing about a reflexive ability survives to the next action.
//! - **It arrives unaimed.** "That spell" is fixed by a delayed trigger's event
//!   (CR 603.7c), but a reflexive ability's target is chosen as it is put on the stack,
//!   like any other trigger (CR 603.3d). So it goes through the ordinary aiming path and
//!   nothing here fills a slot.
//!
//! ## What "it" is
//!
//! The sentence's subject is the permanent the resolution just put onto the battlefield,
//! and every reading the fired ability does is about *that* permanent — its power, and
//! the fact that it is the one dealing the damage. So the ability is created with the
//! **entered permanent as its source** rather than with the spell that created it, which
//! is what makes `it deals damage equal to its power` a self-referential effect
//! ([`Effect::SelfDealsDamage`](crate::Effect)) and not a new kind of reference.
//!
//! Its power is *also* recorded here, into the same
//! [`PaidCost::source_power`](crate::stack::PaidCost) slot an activation writes: a
//! creature that is killed in response to the trigger still deals its damage from last
//! known information (CR 608.2h), and by then there is nothing on the battlefield left to
//! read.

use serde::Deserialize;

use crate::ability::Effect;
use crate::id::{PermanentId, PlayerId};

/// A reflexive triggered ability as a card **authors** it (CR 603.11) — the `when you
/// do`/`when a creature is put onto the battlefield this way` clause an effect creates.
///
/// Plain data, and its own type rather than a reuse of [`Ability::Triggered`](crate::Ability)
/// for the reason [`DelayedTrigger`](crate::delayed::DelayedTrigger) is: the things a
/// reflexive ability can watch are the things a resolution can *do*, which is a different
/// and much smaller set than the events a printed ability watches.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReflexiveTrigger {
    /// What, within the resolution that creates it, it fires on.
    pub event: ReflexiveCondition,
    /// What it does when it fires.
    pub effects: Vec<Effect>,
}

/// What a [`ReflexiveTrigger`] fires on.
///
/// One variant, and it grows by adding more. Every member of this set is a question about
/// what the *current resolution* has already done, answered off the facts the resolution
/// keeps about itself ([`Resolution`](crate::Resolution)) — never off the board, which by
/// now cannot tell what put a permanent there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReflexiveCondition {
    /// A **creature was put onto the battlefield by this resolution** — the `when a
    /// creature is put onto the battlefield this way` of a look that may put one there.
    ///
    /// Read from [`Resolution::entered`](crate::Resolution), written by the answer that
    /// placed it. It is a creature that is asked about because that is what the printed
    /// clause says; a resolution that put a land there answers no, and one that put
    /// nothing there — the player declined, or nothing among the cards matched — asks
    /// nothing at all.
    CreaturePutOntoBattlefieldThisWay,
}

/// A reflexive ability that has fired and is waiting to be put on the stack.
///
/// Written by the effect that created it, during a resolution, and drained at the one
/// place triggers reach the stack — the same transition, always. It is on
/// [`GameState`](crate::GameState) rather than passed along because the effect applying
/// it and the seam collecting it are separated by the rest of `apply_action`, exactly as
/// [`deathtouch_struck`](crate::GameState::deathtouch_struck) is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReflexive {
    /// The player who controls it — the controller of the resolution that created it
    /// (CR 603.11a).
    pub controller: PlayerId,
    /// The permanent the ability's sentence is about, and its source: "**it** deals
    /// damage equal to **its** power".
    pub source: PermanentId,
    /// That permanent's power when the ability was created, kept as last known
    /// information for a source that is dead before the ability resolves (CR 608.2h).
    pub source_power: Option<i32>,
    /// What it does.
    pub effects: Vec<Effect>,
}
