//! Casting, activating, and the effects those produce.
//!
//! Three cohesive halves of one action family, split for size (issue #711) with no
//! behavior change:
//!
//! - [`announce`] — a land played, an ability activated, a spell put on the stack, and
//!   the additional cost paid alongside it (CR 116.2a, CR 601, CR 605.3).
//! - [`effects`] — applying one [`Effect`] that names its subject by class rather than
//!   by a chosen target, including the class-wide modification and damage forms.
//! - [`targeted`] — applying one [`Effect`] to the [`Target`] the caster chose, after
//!   the resolve path re-checked its legality (CR 608.2b).
//!
//! Everything is re-exported at this module's root, so `apply::*` sees exactly the set
//! of names it saw before the split.

use super::*;
use crate::ability::{
    is_mana_ability, Ability, Cost, DamageSubject, Effect, MassAffects, PlayerRef, Target,
};
use crate::card::{abilities_of_permanent, apply_enters_replacements};
use crate::commander::commander_tax_cost;
use crate::id::{CardInstance, PermanentId, PlayerId};
use crate::mana::parse_mana_cost;
use crate::stack::AbilityOrigin;
use crate::state::{Duration, EffectAffects, Modification, Permanent, StaticEffect};

mod announce;
mod effects;
mod targeted;

pub(crate) use announce::*;
pub(crate) use effects::*;
pub(crate) use targeted::*;
