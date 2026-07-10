//! Turn structure: the ordered phases and steps of a turn.

/// A single phase or step of the turn sequence, in play order.
///
/// The engine owns this type independently of `rune-protocol::Phase`. The two
/// align conceptually (same twelve variants, same order) but are deliberately
/// decoupled: the engine never depends on the protocol crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Step {
    /// Untap step: the active player untaps their permanents. Turn starts here.
    #[default]
    Untap,
    /// Upkeep step.
    Upkeep,
    /// Draw step.
    Draw,
    /// Precombat main phase.
    PrecombatMain,
    /// Beginning of combat step.
    BeginCombat,
    /// Declare attackers step.
    DeclareAttackers,
    /// Declare blockers step.
    DeclareBlockers,
    /// Combat damage step.
    CombatDamage,
    /// End of combat step.
    EndCombat,
    /// Postcombat main phase.
    PostcombatMain,
    /// End step.
    End,
    /// Cleanup step.
    Cleanup,
}

impl Step {
    /// The next step in turn order. Wraps from [`Step::Cleanup`] back to
    /// [`Step::Untap`]; advancing the turn number and active player is the
    /// caller's job, not this method's.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Untap => Self::Upkeep,
            Self::Upkeep => Self::Draw,
            Self::Draw => Self::PrecombatMain,
            Self::PrecombatMain => Self::BeginCombat,
            Self::BeginCombat => Self::DeclareAttackers,
            Self::DeclareAttackers => Self::DeclareBlockers,
            Self::DeclareBlockers => Self::CombatDamage,
            Self::CombatDamage => Self::EndCombat,
            Self::EndCombat => Self::PostcombatMain,
            Self::PostcombatMain => Self::End,
            Self::End => Self::Cleanup,
            Self::Cleanup => Self::Untap,
        }
    }
}
