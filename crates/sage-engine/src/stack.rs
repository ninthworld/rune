//! The stack: spells and non-mana abilities waiting to resolve.
//!
//! Plain data with no closures, so [`crate::GameState`] keeps its `Clone`/`Eq`
//! value semantics. Objects resolve top-first (last element is the top) when all
//! players pass priority in succession (see `crate::apply_action`).

use crate::ability::{Effect, Target};
use crate::id::{CardInstance, PermanentId, PlayerId};

/// Identity of an object on the stack, minted fresh when the object is put there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct StackId(pub u64);

/// One object on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackObject {
    /// This object's stack identity.
    pub id: StackId,
    /// The player who controls the object (chooses how it resolves).
    pub controller: PlayerId,
    /// What the object is.
    pub kind: StackObjectKind,
    /// The targets chosen for this object when it was put on the stack (CR
    /// 601.2c — targets are locked in on announcement, not on resolution), in
    /// the order of the targeting [`Effect`]s that consume them.
    ///
    /// Empty for an object that targets nothing. Recording the choice here keeps
    /// the stack a complete, inspectable record ("Lightning Bolt targeting that
    /// creature") and lets resolution re-check each target's legality against
    /// current state without any side lookup. Enumerating and choosing these
    /// values from `valid_actions` is issue #71; this field only stores them.
    pub targets: Vec<Target>,
}

/// The two things that can be on the stack: a spell or an ability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackObjectKind {
    /// A permanent spell cast from a hand; resolving it puts the permanent onto
    /// the battlefield.
    Spell {
        /// The physical card being cast. Carried as a [`CardInstance`] so the
        /// card's identity is preserved from hand, across the stack, onto the
        /// battlefield or into the graveyard.
        card: CardInstance,
    },
    /// A triggered or activated (non-mana) ability; resolving it applies its
    /// effects.
    Ability {
        /// The object whose ability this is — a permanent, or an emblem (CR 114).
        source: AbilitySource,
        /// How this ability got onto the stack (CR 113.3).
        origin: AbilityOrigin,
        /// The effects to apply on resolution.
        effects: Vec<Effect>,
    },
}

/// The object an ability on the stack came from (CR 113.3).
///
/// Until emblems existed this was always a [`PermanentId`], and the ability model could
/// assume its source was on the battlefield: a self-referential effect modified it, a
/// trigger condition observed it, and a source that had left simply did nothing. An
/// **emblem** breaks that assumption in a way no future variant will un-break — it is
/// not a permanent, it has no `PermanentId`, and it is in no zone — so the distinction
/// is stated here rather than smuggled through a sentinel id.
///
/// [`Self::permanent`] is the accessor every existing caller wants: it answers `None`
/// for an emblem, which is the same answer a permanent that has left the battlefield
/// effectively gave, so self-referential effects need no new case.
///
/// A **card in a graveyard** whose ability functions from there (CR 113.6) is the third
/// answer, and it is a third answer rather than a `PermanentId` for the emblem's reason:
/// there is no permanent, and there never was one for this activation. It carries the
/// physical [`CardInstance`] because that — not a [`crate::CardId`] — is what identifies
/// *which* copy in the graveyard the ability belongs to, and a self-referential effect
/// that moves the source out of the graveyard has to name exactly that copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbilitySource {
    /// A permanent on the battlefield.
    Permanent(PermanentId),
    /// An emblem (CR 114), by its object id
    /// ([`Emblem::id`](crate::Emblem::id)).
    Emblem(u64),
    /// A card in a graveyard whose ability functions from that zone (CR 113.6) — the
    /// source of an activation that never involved the battlefield.
    GraveyardCard(CardInstance),
}

impl AbilitySource {
    /// The permanent this ability came from, or `None` for an emblem or a card in a
    /// graveyard — neither of which is one.
    #[must_use]
    pub fn permanent(self) -> Option<PermanentId> {
        match self {
            Self::Permanent(id) => Some(id),
            Self::Emblem(_) | Self::GraveyardCard(_) => None,
        }
    }

    /// The graveyard card this ability came from, or `None` for every other source.
    ///
    /// The counterpart of [`Self::permanent`], and the accessor a self-referential
    /// effect that moves its own card out of a graveyard reads. A permanent's ability
    /// answers `None` here for the same reason a graveyard card's answers `None` there:
    /// the object is simply not that kind of thing.
    #[must_use]
    pub fn graveyard_card(self) -> Option<CardInstance> {
        match self {
            Self::GraveyardCard(card) => Some(card),
            Self::Permanent(_) | Self::Emblem(_) => None,
        }
    }
}

impl From<PermanentId> for AbilitySource {
    fn from(id: PermanentId) -> Self {
        Self::Permanent(id)
    }
}

/// How an ability came to be on the stack — its rules provenance (CR 113.3).
///
/// The two ways differ in rule, not only in flavour: an activated ability is put
/// on the stack by a player who paid its cost (CR 602.2), a triggered one by the
/// game itself the next time a player would receive priority (CR 603.3). Nothing
/// else on a [`StackObject`] separates them — the effects, the source, and the
/// composed description can all be identical — so the distinction is recorded at
/// the moment of the push or it is gone (issue #579).
///
/// Storing it here is what lets the server *state* the finer kind on the wire
/// instead of guessing. A client may never reconstruct this from `description`
/// prose or from when the object appeared: that is rules interpretation, which
/// ADR 0001 puts on the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbilityOrigin {
    /// A player activated it, paying its costs (CR 602.2).
    Activated,
    /// A trigger condition was met and the game put it on the stack (CR 603.3).
    Triggered,
}
