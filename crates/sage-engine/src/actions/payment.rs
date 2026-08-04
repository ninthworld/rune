//! Paying for a cast as part of casting it (CR 601.2).
//!
//! CR 601.2 walks the casting process in order: announce the spell, choose modes and
//! targets, determine the total cost, **activate mana abilities**, pay. It is one
//! process, and if it cannot be completed the game returns to where it was before the
//! process started — so the taps that paid for a spell that never got cast are undone
//! by the rules themselves, not by anybody's undo feature.
//!
//! The engine used to model only the *other* thing: floating mana with a standalone
//! activation, then casting out of a pool that already covered the cost. That is real
//! and stays (CR 605.3 — a player may activate a mana ability whenever they have
//! priority, and holding mana across a cast needs it), but it is the special case. This
//! module is the general one, and everything about it follows from being **one action**:
//!
//! - a payment is applied on the way into the cast, in the same `apply_action` call, so
//!   an insufficient one is a no-op and the state comes back untouched;
//! - a player assembling a payment has therefore sent nothing at all, which is what
//!   makes taking a source back out free — no client draft to reconcile and no server
//!   draft to invalidate;
//! - and the ordering questions that a two-step model has to invent answers for — what
//!   happens if the board moves between the tap and the cast, what an opponent sees
//!   mid-payment — do not arise, because there is no between.
//!
//! **Nothing here decides what a card produces.** A [`ManaSource`](super::ManaSource)
//! names an activation and the card's own ability says what it adds; whether the pool
//! that results covers the cost is [`ManaPool::pay_for`](crate::ManaPool::pay_for)'s
//! answer, the same one the pool-first path has always used. This module decides only
//! **which activations to consider**, never what any one of them is worth.
//!
//! ## A source is a permanent with a choice, not an ability
//!
//! The one structural thing worth stating up front, because it is what the first version
//! of this module got wrong. A permanent with two mana abilities — every dual land in the
//! catalog is `{T}: Add {W}` *and* `{T}: Add {U}` — is **one** source offering **two**
//! options, not two sources. Tapping it takes one of them and retires the other.
//!
//! Modelling those as independent sources broke in both directions at once: the offer
//! credited a dual land with both halves of its mana and so announced casts no payment
//! could pay for, and the search for a payment tried to spend both halves and so could
//! not pay with a dual land at all. [`sources`] is where the distinction is drawn and
//! [`solve`] is where the constraint is enforced.
//!
//! ## What this does not yet do
//!
//! A mana ability that **asks a question** — `Add one mana of any color`, which CR
//! 605.3b expressly permits — is refused as a payment source rather than answered. Its
//! activation suspends into a pending choice, and a choice posed *inside* a casting
//! process is a second suspension point the casting process has not got. Such a source
//! is still activatable on its own, so the pool-first path pays with it; it simply
//! cannot ride inside the cast. That is the honest boundary of this change and it is
//! checked rather than assumed ([`is_plain_mana_source`]) — and, importantly, it is
//! subtracted from what the generator announces against, so the boundary costs a player
//! a convenience and never leaves them an offer they cannot take.

mod apply;
mod pips;
mod solve;
mod sources;

pub(crate) use apply::{apply_payment, payment_covers_cast};
pub use pips::{payment_pips, remaining_cost_pips, PaymentPip};
pub use solve::auto_payment;
pub(crate) use solve::ManaOptions;
pub use sources::{is_plain_mana_source, mana_ability_pips, payment_sources};
