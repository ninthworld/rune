//! What a spell declares about its own **announcement** (CR 601.2): the modes it
//! chooses among, and the properties it gains on the stack once a chosen X is big
//! enough.
//!
//! Both live on the card rather than in [`Effect`](crate::Effect) for the same reason:
//! neither is something a resolution *does*. A mode decides which effects there are at
//! all — and therefore which target slots exist — before targets are chosen; a spell
//! trait is true of the object on the stack from the moment it is put there, and is read
//! by whoever tries to counter it or to prevent its damage, not by its own resolution.

use serde::Deserialize;

use crate::ability::Effect;

/// One mode of a **modal** spell (CR 700.2) — the `• Destroy all creatures.` of a
/// `Choose one —` sorcery.
///
/// A mode is a bundle of effects and nothing else. It carries no cost of its own, no
/// condition, and no name: everything that distinguishes it is what it does, which is
/// also what the generated text renders and what the target slots come from.
///
/// **The choice is made at announcement and it is made first** (CR 601.2b), ahead of
/// targets, because the chosen mode is what decides how many target slots the spell has
/// and what each may aim at. Nothing downstream ever sees the modes it did not choose:
/// [`CardData::spell_effects_for_mode`](super::CardData::spell_effects_for_mode) answers
/// with one mode's effects, and the resolution path has no way to reach another.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellMode {
    /// What this mode does on resolution — the same [`Effect`] vocabulary a
    /// non-modal spell's `spell_effects` uses, and read through the same paths.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// A property a spell has **while it is on the stack**, which no [`Effect`] produces
/// because it is not something the spell does — it is something that is true of it.
///
/// Both members are read by *somebody else's* resolution: a counterspell asks whether
/// its target can be countered (CR 701.5a), and the damage seam asks whether a shield
/// applies to what this spell is dealing (CR 615.1). That is exactly why they are not
/// effects: an effect happens when its own object resolves, and by then the question
/// has already been answered.
///
/// Each carries the threshold its printed sentence does — `If X is 5 or more, this
/// spell can't be countered` — read against the X its controller **announced**
/// (CR 601.2b) and therefore fixed from the moment the spell hit the stack. `None`
/// means the trait applies unconditionally, which is how a card that says so without
/// naming X writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpellTrait {
    /// CR 701.5a: this spell can't be countered. A counterspell may still *target* it —
    /// "can't be countered" is not hexproof and changes nothing about legality — it
    /// simply fails to remove it when it resolves.
    CantBeCountered {
        /// Only while the announced X is at least this much. Absent means always.
        #[serde(default)]
        if_x_at_least: Option<u32>,
    },
    /// CR 615.1: damage this spell deals can't be prevented. The shield is consulted at
    /// the one seam damage is dealt and is simply not allowed to apply, so the damage
    /// lands in full.
    DamageCantBePrevented {
        /// Only while the announced X is at least this much. Absent means always.
        #[serde(default)]
        if_x_at_least: Option<u32>,
    },
}

impl SpellTrait {
    /// Whether this trait is in force for a spell announced with `x`.
    ///
    /// An unconditional trait always is. A conditional one is measured against the
    /// announced value, and a spell that announced **no** X never meets a threshold —
    /// which is right rather than defensive: a card that says "if X is 5 or more" has an
    /// X, and one that has not got one has nothing for the sentence to be about.
    #[must_use]
    pub fn applies(self, x: Option<u32>) -> bool {
        let threshold = match self {
            Self::CantBeCountered { if_x_at_least }
            | Self::DamageCantBePrevented { if_x_at_least } => if_x_at_least,
        };
        match threshold {
            None => true,
            Some(least) => x.is_some_and(|announced| announced >= least),
        }
    }
}
