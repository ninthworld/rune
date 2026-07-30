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
        /// The permanent whose ability this is.
        source: PermanentId,
        /// How this ability got onto the stack (CR 113.3).
        origin: AbilityOrigin,
        /// The effects to apply on resolution.
        effects: Vec<Effect>,
    },
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
