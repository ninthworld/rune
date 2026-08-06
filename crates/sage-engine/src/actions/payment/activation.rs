//! Paying for an **activation** with something the player picks (CR 601.2b, 602.2b).
//!
//! The cast side of this seam is the rest of this module: a cost paid at announcement has
//! no resolution to ask during, so what pays it rides on the action. An activation is the
//! same problem with a different source — the permanent is on the battlefield rather than
//! on its way to the stack — and it is answered the same way, with the same
//! [`CostPayment`] entries on [`Action::ActivateAbility`].
//!
//! **Mana is deliberately not part of it.** An activation pays mana from its controller's
//! pool (CR 602.2b), floated by activating mana abilities as actions in their own right,
//! which is what the engine has always done and what every client already drives. Only the
//! components that require a *choice* are carried — [`Cost::Sacrifice`] and
//! [`Cost::Discard`] — and a mana entry submitted on an activation is refused rather than
//! dropped, because a payment the engine silently ignored is one the player believed they
//! had made.

use crate::ability::{Ability, Cost};
use crate::card::{abilities_of_permanent, CardDatabase};
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::state::{GameState, Permanent};

use super::super::definition::{discards_of, mana_of, sacrifices_of, CostPayment};
use super::pips::DiscardCost;

/// The activated ability `index` of `permanent`, its cost list, and the seat that would
/// pay for it — the one lookup every part of this module goes through, so the offer, the
/// candidate lists, the payment check, and the charge can never disagree about which
/// ability is being paid for.
///
/// `None` for a permanent that is not on the battlefield or an index naming nothing or a
/// non-activated ability: each is a way an action can be stale rather than merely
/// unpayable.
fn activated(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Option<(PlayerId, Vec<Cost>)> {
    let perm = state.battlefield.iter().find(|p| p.id == permanent)?;
    let seat = crate::characteristics::controller_of(state, perm);
    match abilities_of_permanent(state, db, perm).get(index) {
        Some(Ability::Activated { cost, .. }) => Some((seat, cost.clone())),
        _ => None,
    }
}

/// Every permanent `seat` could sacrifice to `component`, given that the ability's source
/// is `source` (CR 701.17b — a player may sacrifice only what they control).
///
/// The one enumeration behind the offer gate, the candidate list the server poses, the
/// payment check, and the auto-payment, so an ability is offered exactly when it can be
/// paid and the list a player picks from is the list the engine will accept. Empty for a
/// component that sacrifices nothing.
pub(crate) fn sacrifice_candidates_for(
    state: &GameState,
    db: &CardDatabase,
    source: PermanentId,
    seat: PlayerId,
    component: &Cost,
) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| accepts(state, db, source, seat, component, perm))
        .map(|perm| perm.id)
        .collect()
}

/// Whether `perm` is a permanent `seat` may sacrifice to `component` right now: theirs
/// (CR 701.17b), matching the component's filter, and not the ability's own source when
/// the cost says *another*.
fn accepts(
    state: &GameState,
    db: &CardDatabase,
    source: PermanentId,
    seat: PlayerId,
    component: &Cost,
    perm: &Permanent,
) -> bool {
    if component.excludes_source() && perm.id == source {
        return false;
    }
    crate::characteristics::controller_of(state, perm) == seat
        && perm
            .printed
            .face(db)
            .is_some_and(|face| component.accepts_sacrifice(&face))
}

/// Whether every component of `cost` that requires a *choice* has something to choose —
/// the half of the offer gate [`cost_payable`](super::super::utilities::cost_payable)
/// cannot answer from the source alone (CR 602.2b: a cost that cannot be paid makes the
/// ability unactivatable, not activatable-and-then-free).
pub(crate) fn chosen_costs_are_payable(
    state: &GameState,
    db: &CardDatabase,
    source: &Permanent,
    component: &Cost,
) -> bool {
    let seat = crate::characteristics::controller_of(state, source);
    match component {
        Cost::Sacrifice { .. } => {
            !sacrifice_candidates_for(state, db, source.id, seat, component).is_empty()
        }
        Cost::Discard { count } => hand_of(state, seat).len() >= usize::from(*count),
        Cost::Tap
        | Cost::Mana { .. }
        | Cost::Loyalty { .. }
        | Cost::SacrificeThis
        | Cost::RemoveCounters { .. } => true,
    }
}

/// The cards in `seat`'s hand, as instance ids. Unlike the cast side there is nothing to
/// exclude: the source of an activated ability is a permanent, never a card in the hand
/// that is paying for it.
fn hand_of(state: &GameState, seat: PlayerId) -> Vec<CardInstanceId> {
    state
        .players
        .get(seat.0)
        .map(|player| player.hand.iter().map(|card| card.id).collect())
        .unwrap_or_default()
}

/// The discard the activation of `permanent`'s ability `index` demands, and the cards that
/// could pay it (CR 601.2b) — the activation counterpart of
/// [`discard_cost`](super::discard_cost).
///
/// `None` for an ability that discards nothing, which is almost every ability. A cost that
/// names several discard components is summed into one question, because a player paying
/// two costs of one card is choosing once.
#[must_use]
pub fn activation_discard_cost(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Option<DiscardCost> {
    let (seat, cost) = activated(state, db, permanent, index)?;
    let count: u8 = cost
        .iter()
        .map(Cost::discard_count)
        .fold(0u8, u8::saturating_add);
    (count > 0).then(|| DiscardCost {
        count,
        candidates: hand_of(state, seat),
    })
}

/// The permanents the activator may sacrifice to `permanent`'s ability `index`, or `None`
/// for an ability that sacrifices nothing chosen.
///
/// The activation counterpart of [`sacrifice_cost`](super::sacrifice_cost), and a bare list
/// rather than a struct because there is nothing else to say: a discard states a count, and
/// a sacrifice is always exactly one. What the cost *reads as* is the rules-text
/// formatter's answer, from the same [`Cost`] this enumerates for — so the question a
/// player is asked and the line printed on the card cannot drift apart.
#[must_use]
pub fn activation_sacrifice_candidates(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Option<Vec<PermanentId>> {
    let (seat, cost) = activated(state, db, permanent, index)?;
    let component = cost.iter().find(|c| matches!(c, Cost::Sacrifice { .. }))?;
    Some(sacrifice_candidates_for(
        state, db, permanent, seat, component,
    ))
}

/// Whether `payment` is **exactly** what activating `permanent`'s ability `index` demands
/// (CR 601.2b) — the gate [`crate::apply_action`] runs before it charges anything.
///
/// Exact in both directions, like the cast side: an ability with no chosen cost accepts no
/// payment at all, and one with a cost is paid by precisely what it asks for — never fewer,
/// and never more, since over-paying a cost is not something a player may choose to do.
///
/// Every named object is re-derived from **current** state: a card must still be in the
/// activator's hand, a permanent must still be on the battlefield, still theirs, still
/// matching the filter, and still not the source when the cost says *another*. A stale or
/// forged id therefore names nothing rather than paying for something.
#[must_use]
pub(crate) fn payment_covers_activation(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    payment: &[CostPayment],
) -> bool {
    let Some((seat, cost)) = activated(state, db, permanent, index) else {
        return false;
    };
    // Mana is paid from the pool, not named on the action (CR 602.2b). Refused rather than
    // ignored: a payment the engine dropped is one the player thought they had made.
    if !mana_of(payment).is_empty() {
        return false;
    }
    discards_pay(state, seat, &cost, &discards_of(payment))
        && sacrifices_pay(state, db, permanent, seat, &cost, &sacrifices_of(payment))
}

/// Whether `discards` is exactly the cards `cost` demands, each a distinct card still in
/// `seat`'s hand.
fn discards_pay(
    state: &GameState,
    seat: PlayerId,
    cost: &[Cost],
    discards: &[CardInstanceId],
) -> bool {
    let owed: u8 = cost
        .iter()
        .map(Cost::discard_count)
        .fold(0u8, u8::saturating_add);
    if discards.len() != usize::from(owed) {
        return false;
    }
    let hand = hand_of(state, seat);
    discards
        .iter()
        .enumerate()
        .all(|(i, named)| !discards[..i].contains(named) && hand.contains(named))
}

/// Whether `sacrifices` is exactly the permanents `cost` demands: one per sacrifice
/// component, in that order, each distinct and each a permanent that component accepts.
fn sacrifices_pay(
    state: &GameState,
    db: &CardDatabase,
    source: PermanentId,
    seat: PlayerId,
    cost: &[Cost],
    sacrifices: &[PermanentId],
) -> bool {
    let owed: Vec<&Cost> = cost
        .iter()
        .filter(|c| matches!(c, Cost::Sacrifice { .. }))
        .collect();
    if sacrifices.len() != owed.len() {
        return false;
    }
    sacrifices
        .iter()
        .zip(owed)
        .enumerate()
        .all(|(i, (named, component))| {
            !sacrifices[..i].contains(named)
                && state.battlefield.iter().any(|perm| {
                    perm.id == *named && accepts(state, db, source, seat, component, perm)
                })
        })
}

/// A legal payment for activating `permanent`'s ability `index`, or `None` when its chosen
/// costs cannot be paid.
///
/// A **rules** question, not a policy one, exactly as [`auto_payment`](super::auto_payment)
/// is for a cast: this answers *what would be a legal payment*, and whether to use the
/// answer — pay for the player, or ask them — belongs to the caller (ADR 0010). It is
/// emphatically **a** legal payment and not a good one: the permanents and cards are taken
/// in board and hand order, which is a rule about a list rather than a decision about a
/// game, and a caller that hands one to a person without asking has chosen for them.
#[must_use]
pub fn auto_activation_payment(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Option<Vec<CostPayment>> {
    let (seat, cost) = activated(state, db, permanent, index)?;
    let mut payment = Vec::new();
    for component in &cost {
        match component {
            Cost::Sacrifice { .. } => {
                let chosen = sacrifice_candidates_for(state, db, permanent, seat, component)
                    .into_iter()
                    // One permanent may not pay two components of one cost.
                    .find(|id| !payment.contains(&CostPayment::Sacrifice(*id)))?;
                payment.push(CostPayment::Sacrifice(chosen));
            }
            Cost::Discard { count } => {
                let mut taken = 0usize;
                for card in hand_of(state, seat) {
                    if taken == usize::from(*count) {
                        break;
                    }
                    // A card already named for an earlier component pays that one, not
                    // this one.
                    if payment.contains(&CostPayment::Discard(card)) {
                        continue;
                    }
                    payment.push(CostPayment::Discard(card));
                    taken += 1;
                }
                if taken < usize::from(*count) {
                    return None;
                }
            }
            Cost::Tap
            | Cost::Mana { .. }
            | Cost::Loyalty { .. }
            | Cost::SacrificeThis
            | Cost::RemoveCounters { .. } => {}
        }
    }
    Some(payment)
}
