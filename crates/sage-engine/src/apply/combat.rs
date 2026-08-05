//! Applying the combat actions and turn-based actions.
//!
//! Two cohesive halves, split for size (issue #711) with no behavior change:
//!
//! - [`declare`] — the declarations a player makes: attackers, blockers, and the order
//!   an attacker puts its blockers in.
//! - [`strike`] — the turn-based actions the game performs: dealing combat damage, and
//!   removing every creature from combat at end of combat.
//!
//! Everything is re-exported at this module's root, so `apply::*` sees exactly the set
//! of names it saw before the split.

use super::*;
use crate::actions::{Attack, Block, DamageOrder};
use crate::combat::{
    attacking_taps, blocked_attackers, combat_damage, combat_has_first_strike, defending_player,
    pending_blocker_declarer, CombatDamage, DamageStep,
};
use crate::id::{CardId, PermanentId};
use crate::state::{LoggedIdentity, LoggedPermanent};

mod declare;
mod strike;

pub(crate) use declare::*;
pub(crate) use strike::*;
