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
        /// The effects to apply on resolution.
        effects: Vec<Effect>,
    },
}
