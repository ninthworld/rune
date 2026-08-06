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
//! components that require a *choice* are carried — [`Cost::Sacrifice`], [`Cost::Discard`]
//! and [`Cost::ExileFromGraveyard`] — and a mana entry submitted on an activation is
//! refused rather than dropped, because a payment the engine silently ignored is one the
//! player believed they had made.

use crate::ability::{Ability, Cost};
use crate::card::{abilities_of_permanent, CardDatabase};
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::state::{GameState, Permanent};

use super::super::definition::{discards_of, exiles_of, mana_of, sacrifices_of, CostPayment};
use super::pips::{DiscardCost, ExileCost, SacrificeCost};

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

/// Every card `seat` could exile from their own graveyard to `component` (CR 601.2b) —
/// the graveyard counterpart of [`sacrifice_candidates_for`], and the one enumeration
/// behind the offer gate, the candidate list the server poses, the payment check, and the
/// auto-payment.
///
/// **Their own graveyard and no other**, which is what every printed cost of this shape
/// says. The class is read off the printed face, because a card in a graveyard has no
/// computed characteristics to read instead.
pub(crate) fn exile_candidates_for(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    component: &Cost,
) -> Vec<CardInstanceId> {
    let Some(player) = state.players.get(seat.0) else {
        return Vec::new();
    };
    player
        .graveyard
        .iter()
        .filter(|card| {
            db.card(card.card)
                .is_some_and(|data| component.accepts_exile(data))
        })
        .map(|card| card.id)
        .collect()
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
        // A sacrifice needs that many permanents to be there (CR 602.2b).
        Cost::Sacrifice { count, .. } => {
            sacrifice_candidates_for(state, db, source.id, seat, component).len()
                >= usize::from(count.min())
        }
        Cost::Discard { count } => hand_of(state, seat).len() >= usize::from(*count),
        Cost::ExileFromGraveyard { count, .. } => {
            exile_candidates_for(state, db, seat, component).len() >= usize::from(*count)
        }
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

/// The sacrifice `permanent`'s ability `index` demands, and the permanents that could pay
/// it — or `None` for an ability that sacrifices nothing chosen.
///
/// The activation counterpart of [`sacrifice_cost`](super::sacrifice_cost), and the same
/// [`SacrificeCost`] because it is the same question over the same zone. What the cost
/// *reads as* is the rules-text formatter's answer, from the same [`Cost`] this enumerates
/// for — so the question a player is asked and the line printed on the card cannot drift
/// apart.
#[must_use]
pub fn activation_sacrifice_cost(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Option<SacrificeCost> {
    let (seat, cost) = activated(state, db, permanent, index)?;
    let component = cost.iter().find(|c| matches!(c, Cost::Sacrifice { .. }))?;
    Some(SacrificeCost {
        count: component.sacrifice_count().unwrap_or_default(),
        candidates: sacrifice_candidates_for(state, db, permanent, seat, component),
    })
}

/// The exile `permanent`'s ability `index` demands, and the cards in the activator's
/// graveyard that could pay it (CR 601.2b) — or `None` for an ability that exiles nothing.
///
/// The third of the three chosen-cost enumerations, beside
/// [`activation_discard_cost`] and [`activation_sacrifice_cost`], and posed the same way:
/// a count and the candidates it may be paid from, so the server offers a list rather than
/// a rule. A cost naming several exile components is summed into one question, because a
/// player paying two components of one cost is choosing once.
#[must_use]
pub fn activation_exile_cost(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Option<ExileCost> {
    let (seat, cost) = activated(state, db, permanent, index)?;
    let count: u8 = cost
        .iter()
        .map(Cost::exile_count)
        .fold(0u8, u8::saturating_add);
    let component = cost
        .iter()
        .find(|c| matches!(c, Cost::ExileFromGraveyard { .. }))?;
    (count > 0).then(|| ExileCost {
        count,
        candidates: exile_candidates_for(state, db, seat, component),
    })
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
        && exiles_pay(state, db, seat, &cost, &exiles_of(payment))
}

/// Whether `exiles` is exactly the cards `cost` demands, each a distinct card still in
/// `seat`'s own graveyard and still of the class the cost names.
///
/// The gate a **forged** choice runs into: an id naming a card in someone else's
/// graveyard, a card that has already left it, or one of the wrong class matches no
/// candidate and pays nothing, so the whole activation is refused rather than partly
/// applied.
fn exiles_pay(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    cost: &[Cost],
    exiles: &[CardInstanceId],
) -> bool {
    let owed: u8 = cost
        .iter()
        .map(Cost::exile_count)
        .fold(0u8, u8::saturating_add);
    if exiles.len() != usize::from(owed) {
        return false;
    }
    let Some(component) = cost
        .iter()
        .find(|c| matches!(c, Cost::ExileFromGraveyard { .. }))
    else {
        return exiles.is_empty();
    };
    let candidates = exile_candidates_for(state, db, seat, component);
    exiles
        .iter()
        .enumerate()
        .all(|(i, named)| !exiles[..i].contains(named) && candidates.contains(named))
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

/// Whether `sacrifices` is exactly the permanents `cost` demands: as many as each
/// sacrifice component takes, each distinct and each a permanent that component accepts.
///
/// A cost naming more than one sacrifice component is summed rather than paired
/// positionally: no printed card has two, and a sum is the reading that stays right if one
/// ever prints. `Sacrifice two artifacts` is therefore paid by two distinct artifacts and
/// refused by one, which is the whole of that cost.
fn sacrifices_pay(
    state: &GameState,
    db: &CardDatabase,
    source: PermanentId,
    seat: PlayerId,
    cost: &[Cost],
    sacrifices: &[PermanentId],
) -> bool {
    let Some(component) = cost.iter().find(|c| matches!(c, Cost::Sacrifice { .. })) else {
        return sacrifices.is_empty();
    };
    let count = component.sacrifice_count().unwrap_or_default();
    let candidates = sacrifice_candidates_for(state, db, source, seat, component);
    if !count.is_paid_by(sacrifices.len()) {
        return false;
    }
    sacrifices
        .iter()
        .enumerate()
        .all(|(i, named)| !sacrifices[..i].contains(named) && candidates.contains(named))
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
            // A count takes that many off the front of the candidate list, and every count
            // a cost may name is exact: how many to sacrifice is a decision, and a decision
            // belongs to a resolution rather than to a payment.
            Cost::Sacrifice { count, .. } => {
                let wanted = usize::from(count.min());
                let mut taken = 0usize;
                for id in sacrifice_candidates_for(state, db, permanent, seat, component) {
                    if taken == wanted {
                        break;
                    }
                    // One permanent may not pay two components of one cost.
                    if payment.contains(&CostPayment::Sacrifice(id)) {
                        continue;
                    }
                    payment.push(CostPayment::Sacrifice(id));
                    taken += 1;
                }
                if taken < wanted {
                    return None;
                }
            }
            Cost::ExileFromGraveyard { count, .. } => {
                let mut taken = 0usize;
                for card in exile_candidates_for(state, db, seat, component) {
                    if taken == usize::from(*count) {
                        break;
                    }
                    if payment.contains(&CostPayment::Exile(card)) {
                        continue;
                    }
                    payment.push(CostPayment::Exile(card));
                    taken += 1;
                }
                if taken < usize::from(*count) {
                    return None;
                }
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
