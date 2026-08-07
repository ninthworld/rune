//! The choices made **as a spell is announced** (CR 601.2b): its mode, and its X.
//!
//! Both come before targets and before payment, and both are enumerated the way ADR 0004
//! enumerates targets — the action is advertised **once** in its requirement form, and
//! the legal answers for each choice are a separate, freshly computed list. Nothing here
//! pre-expands one [`Action`] per mode or per value.
//!
//! The two are not symmetric and the asymmetry is the whole design:
//!
//! - **A mode changes which target slots exist.** So the mode is answered first, and
//!   [`target_requirements`](crate::target_requirements) reads it off the action before
//!   it can say anything at all — a modal cast with no mode declares no slots, which is
//!   why [`announcement_is_legal`] has to refuse it explicitly rather than let it pass
//!   as a cast that happened to need no targets.
//! - **X changes what the spell costs.** So this module states the legal values *and
//!   what each one costs*, in full, rather than handing anyone a cost with an `X` still
//!   in it. Working that multiplication out is deciding what a spell costs, and the one
//!   place that is decided is [`cast_cost`].

use crate::ability::Effect;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::Action;
use super::payment::ManaOptions;
use super::utilities::cast_cost;

/// One mode a modal spell offers (CR 700.2), as [`crate::valid_actions`] advertises it.
///
/// It carries the mode's **effects** rather than a rendered sentence, because generating
/// prose is presentation and belongs above the engine (ADR 0008 §7) — the same rule that
/// keeps every other piece of card text out of this crate. What a caller draws on a
/// numbered dock row is those effects put through the ordinary formatter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeOption {
    /// This mode's position in the card's printed list, and the value an announcement
    /// carries in [`Action::CastSpell::mode`].
    pub index: u8,
    /// What choosing it makes the spell do.
    pub effects: Vec<Effect>,
}

/// One legal value of X, and what announcing it costs (CR 601.2b).
///
/// The cost is stated because nobody above the engine may work it out. `{X}{R}` at X = 3
/// is `{3}{R}`, and a caller that multiplied to get there would be deciding what a spell
/// costs — which is the one thing a client must never do (`docs/client-design.md` §6.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XOption {
    /// The announced value.
    pub value: u32,
    /// The whole cost of the cast at this value, in `{...}` notation — never a cost with
    /// an `X` left in it.
    pub cost: String,
}

/// The most X a single announcement may be enumerated up to.
///
/// A bound rather than a rule: X is limited by what the board could pay, and the search
/// below stops the moment a value becomes unaffordable. This only stops a pathological
/// board — one making hundreds of mana — from turning the offer into a list nobody could
/// read, and it is well past any mana a real game produces.
const MAX_ANNOUNCED_X: u32 = 64;

/// The modes `action` offers, in printed order — empty for every action that is not a
/// cast of a modal card.
///
/// A mode whose target slots **cannot be filled** is left out (CR 601.2c): a spell may be
/// cast only with a mode whose targets are legally choosable, so offering one that is not
/// would be offering an announcement [`crate::apply_action`] must then refuse. That check
/// is the same per-slot enumeration every other offer is gated on, so the modes advertised
/// are exactly the modes that can be announced.
#[must_use]
pub fn mode_options(state: &GameState, db: &CardDatabase, action: &Action) -> Vec<ModeOption> {
    let Action::CastSpell { card, .. } = action else {
        return Vec::new();
    };
    let Some(data) = db.card(card.card) else {
        return Vec::new();
    };
    data.modes
        .iter()
        .enumerate()
        .filter_map(|(index, mode)| Some((u8::try_from(index).ok()?, mode)))
        .filter(|(index, _)| {
            super::generation::groups_are_fillable(
                &data.cast_target_groups(Some(*index)),
                state,
                state.priority,
                // A mode belongs to a spell, and a spell is not a permanent.
                None,
                db,
            )
        })
        .map(|(index, mode)| ModeOption {
            index,
            effects: mode.effects.clone(),
        })
        .collect()
}

/// The values of X `action` may be announced at, cheapest first, each with the whole cost
/// of announcing it — empty for every action whose card prints no `{X}`.
///
/// Affordability is judged by the same search the offer itself is
/// ([`ManaOptions::covers`]): a value is listed exactly when a payment for it exists out
/// of the pool plus everything the seat could still tap for. Because raising X only ever
/// adds generic mana, affordability is monotone, so the walk stops at the first value the
/// board cannot pay for and the list is a contiguous range from zero.
///
/// A value being unlisted is **not** what makes it illegal. [`apply_action`] re-derives
/// the whole payment against the announced value, so an X the pool cannot cover is
/// refused there too — the offer and the gate are two independent answers to the same
/// question, which is the discipline every other choice in this module keeps.
#[must_use]
pub fn x_options(state: &GameState, db: &CardDatabase, action: &Action) -> Vec<XOption> {
    let Action::CastSpell { card, .. } = action else {
        return Vec::new();
    };
    let Some(data) = db.card(card.card) else {
        return Vec::new();
    };
    if !data.announces_x() {
        return Vec::new();
    }
    let payable = ManaOptions::of(state, db, state.priority);
    let purpose = crate::mana::SpendPurpose::CastingSpell {
        subtypes: &data.subtypes,
    };
    let mut out = Vec::new();
    for value in 0..=MAX_ANNOUNCED_X {
        let Some((cost, _)) = cast_cost(state, db, *card, Some(value)) else {
            break;
        };
        if !payable.covers(&cost, purpose) {
            break;
        }
        out.push(XOption {
            value,
            cost: cost.printed(),
        });
    }
    out
}

/// Whether the **announcement choices** carried by `action` are ones this card actually
/// asks for (CR 601.2b) — the gate [`crate::apply_action`] runs before it looks at a
/// single target.
///
/// Exact in both directions, and that is what makes it a gate rather than a filter:
///
/// - a modal card must carry a mode, and one that names a printed bullet — so a forged
///   index is refused, and so is the announcement that skipped the question, which
///   would otherwise slip through as a cast with no target slots;
/// - a non-modal card must carry none, because a mode nothing would read is a choice the
///   player thinks they made;
/// - a card whose cost prints `{X}` must announce a value, and one whose cost does not
///   must announce none — for the same reason in both directions.
///
/// Whether the announced X is *payable* is a separate question, asked where every other
/// cost question is ([`payment_covers_cast`](super::payment_covers_cast)), against the
/// cost this value produces.
#[must_use]
pub(crate) fn announcement_is_legal(db: &CardDatabase, action: &Action) -> bool {
    let Action::CastSpell { card, mode, x, .. } = action else {
        // No other action announces either choice, so any value on one is a forgery.
        return true;
    };
    let Some(data) = db.card(card.card) else {
        return false;
    };
    let mode_ok = match mode {
        Some(index) => data.modes.get(usize::from(*index)).is_some(),
        None => !data.is_modal(),
    };
    mode_ok && x.is_some() == data.announces_x()
}
