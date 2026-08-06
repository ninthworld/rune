//! Posing a cast's cost as slots, and binding the answers back into a payment.
//!
//! The engine says what is still owed and what could pay each unit
//! ([`sage_engine::payment_pips`]); this turns that into `pay_mana` slots, and turns the
//! ids that come back into [`CostPayment`] entries. Nothing here decides what anything
//! costs.
//!
//! A cost that is not mana is posed the same way, on the slot shape that already exists for
//! it: `As an additional cost, discard a card` is a `select_from_zone` over the hand
//! (CR 601.2b), which is the same prompt a cleanup discard and a mulligan bottoming use.
//! It rides in the same action and is taken back the same way, so abandoning a payment
//! half-made abandons all of it.
//!
//! **Anything the client leaves unanswered, the server pays** ([`bind_payment`]). That is
//! ADR 0010's line exactly: the engine answers *what a legal payment is* and the server
//! decides *whether to pay for the player instead of asking*. It is also what keeps every
//! existing consumer working — the terminal client, the deterministic agent, and the AI
//! all skip these slots, and all of them assume that an action on offer is one they can
//! take.

use super::*;

mod activation;

pub(crate) use activation::{activation_payment_prompts, bind_activation_payment};

/// The slot id for the `n`th pip of a cast's remaining cost. Recomputed (never parsed)
/// on resolution, like every other slot id.
fn pip_slot(index: usize) -> String {
    format!("pay_{index}")
}

/// The slot a discard cost is answered on (CR 601.2b) — a cast's additional cost and an
/// activation's alike, because it is the same question about the same zone.
const DISCARD_SLOT: &str = "cost_discard";

/// The slot a sacrifice cost is answered on (CR 601.2b / 701.17), for a cast and an
/// activation alike.
const SACRIFICE_SLOT: &str = "cost_sacrifice";

/// The slot an exile-from-graveyard cost is answered on (CR 601.2b / 701.19). The third
/// zone beside the hand and the battlefield, and no new prompt kind: `zone` is a free-form
/// string precisely so a new pile needs no new wire shape.
const EXILE_SLOT: &str = "cost_exile";

/// The opaque id for one way to pay a pip: which permanent, and which of its abilities.
///
/// Two options over one permanent differ only in the ability index, which is exactly what
/// makes "which color did you mean?" answerable — the client sends back the one the
/// player picked. Recomputed and matched, never parsed.
fn mana_option_id(source: sage_engine::ManaSource) -> String {
    format!("{}#{}", permanent_entity_id(source.permanent), source.index)
}

/// One `pay_mana` slot per unit of `card`'s cost that is still owed (CR 601.2f–g).
///
/// Empty when the pool already covers the cost — the CR 605.3 float-first path, where
/// there is nothing left to choose.
pub(crate) fn cast_payment_prompts(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    x: Option<u32>,
) -> Vec<Prompt> {
    let Some(pips) = sage_engine::payment_pips(state, db, card, x) else {
        return Vec::new();
    };
    let mut prompts = mana_prompts(pips, state, db);
    // The rest of the total cost (CR 601.2b), on the slot shape that already exists for
    // choosing cards out of a zone. It is exact — `min` equals `count` — because a cost is
    // paid for what it asks and not for less.
    if let Some(discard) = sage_engine::discard_cost(state, db, card) {
        prompts.push(Prompt::SelectFromZone {
            slot: DISCARD_SLOT.to_string(),
            prompt: format!(
                "Discard {}",
                if discard.count == 1 {
                    "a card".to_string()
                } else {
                    format!("{} cards", discard.count)
                }
            ),
            zone: "hand".to_string(),
            owner: player_id(state.priority),
            count: u32::from(discard.count),
            min: None,
            candidates: discard.candidates.into_iter().map(card_entity_id).collect(),
        });
    }
    // A sacrifice rides the same slot shape, over the battlefield instead of the hand.
    // `zone` is a free-form string precisely so a new zone does not need a new prompt
    // kind, and a client that renders "pick from this list" already renders this one.
    if let Some(sacrifice) = sage_engine::sacrifice_cost(state, db, card) {
        prompts.push(sacrifice_prompt(
            &sacrifice,
            additional_cost_prompt_for(db, card),
            state.priority,
        ));
    }
    prompts
}

/// The `select_from_zone` slot a sacrifice cost is answered on, for a cast and an
/// activation alike.
///
/// **An exact selection**, and every sacrifice cost is one: `count` with no `min`, the
/// shape every cost has. A sacrifice whose *size* the player picks is not a cost at all —
/// it is a resolution's question, posed on the choice queue rather than on a cast slot.
fn sacrifice_prompt(
    cost: &sage_engine::SacrificeCost,
    prompt: String,
    payer: sage_engine::PlayerId,
) -> Prompt {
    Prompt::SelectFromZone {
        slot: SACRIFICE_SLOT.to_string(),
        prompt,
        zone: "battlefield".to_string(),
        owner: player_id(payer),
        count: u32::from(cost.count.min()),
        min: None,
        candidates: cost
            .candidates
            .iter()
            .copied()
            .map(permanent_entity_id)
            .collect(),
    }
}

/// The words `card`'s additional cost asks its question in — the card's own, from the same
/// formatter that writes its rules text, so the prompt and the printed line are one
/// string. Empty for a card with no such cost, which never reaches a slot.
fn additional_cost_prompt_for(db: &CardDatabase, card: CardInstance) -> String {
    db.card(card.card)
        .and_then(|data| data.additional_cost)
        .map(crate::rules_text::additional_cost_prompt)
        .unwrap_or_default()
}

/// One `pay_mana` slot per unpaid pip.
fn mana_prompts(
    pips: Vec<sage_engine::PaymentPip>,
    state: &GameState,
    db: &CardDatabase,
) -> Vec<Prompt> {
    pips.into_iter()
        .enumerate()
        .map(|(index, pip)| Prompt::PayMana {
            slot: pip_slot(index),
            prompt: format!("Pay {}", pip.pip),
            candidates: pip
                .candidates
                .into_iter()
                .map(|source| ManaOption {
                    id: mana_option_id(source),
                    source: permanent_entity_id(source.permanent),
                    // What this activation produces, so a client asking *which half of
                    // this dual land* has something to put on the two answers. The
                    // engine already listed a source once per way it could pay this pip.
                    label: ability_pip_label(state, db, source),
                    // Whether spending it turns the card sideways (CR 602.2a) — the
                    // engine's read of the activation cost, so a client can draw a
                    // payment it has not sent yet without knowing what `{T}` means.
                    taps: ability_taps(state, db, source),
                })
                .collect(),
            pip: pip.pip,
        })
        .collect()
}

/// Whether activating `source` taps the permanent it names ([`activation_taps`]).
///
/// The other half of what a client needs to draw a payment being assembled: the pips it
/// still owes, and what tapping each source it picks would do to the board. `false` for an
/// activation that cannot be resolved, which is the direction that draws nothing.
fn ability_taps(state: &GameState, db: &CardDatabase, source: sage_engine::ManaSource) -> bool {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == source.permanent)
        .and_then(|perm| {
            abilities_of_permanent(state, db, perm)
                .get(source.index)
                .map(activation_taps)
        })
        .unwrap_or(false)
}

/// What activating `source` adds, as a mana symbol — the label on a disambiguating
/// question. Empty when the ability is not a plain mana source, which cannot happen for a
/// candidate the engine listed but is not worth an unwrap.
fn ability_pip_label(
    state: &GameState,
    db: &CardDatabase,
    source: sage_engine::ManaSource,
) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == source.permanent)
        .and_then(|perm| {
            abilities_of_permanent(state, db, perm)
                .get(source.index)
                .map(|ability| mana_ability_pips(ability).join(""))
        })
        .unwrap_or_default()
}

/// Bind a returned selection onto the payment for casting `card`.
///
/// `x` is the value the announcement fixed (CR 601.2b): the pips are recomputed for the
/// cost that value produces, so a payment is bound against what the spell actually costs
/// rather than against what it would have cost at zero.
///
/// Every pip the client answered is resolved against the **freshly recomputed**
/// candidates, so an id naming a source the board no longer offers — a land tapped since
/// the view went out, a permanent that left — is refused rather than smuggled through. A
/// pip left unanswered is one the server pays.
///
/// The two halves are deliberately all-or-nothing per submission: a partially answered
/// payment is completed by [`sage_engine::auto_payment`] over the *whole* cost rather
/// than by topping up the player's choices, because "what pays the rest, given these"
/// is a search the engine already does in one place and doing it twice is how the two
/// answers drift apart. A client that wants its own sources spent sends all of them.
pub(crate) fn bind_payment(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    x: Option<u32>,
    targets: &[TargetChoice],
) -> Vec<CostPayment> {
    let pips = sage_engine::payment_pips(state, db, card, x).unwrap_or_default();
    let mut chosen = Vec::new();
    let mut spent: Vec<PermanentId> = Vec::new();
    for (index, pip) in pips.iter().enumerate() {
        let answered = chosen_for(targets, &pip_slot(index));
        let Some(id) = answered.first() else {
            continue;
        };
        // The id must name one of *this* pip's current candidates, and a permanent can
        // be tapped once — so a payment naming the same source for two pips is refused
        // here rather than left for the engine to discover.
        let Some(source) = pip
            .candidates
            .iter()
            .find(|candidate| mana_option_id(**candidate) == *id)
        else {
            return fallback_payment(state, db, card, x);
        };
        if spent.contains(&source.permanent) {
            return fallback_payment(state, db, card, x);
        }
        spent.push(source.permanent);
        chosen.push(CostPayment::Mana(*source));
    }
    if chosen.len() < pips.len() {
        // A payment the player did not finish. The server pays the whole thing rather
        // than half of it — see the note above on why this is not a top-up. An announced
        // X above zero lands here by construction today: the pips were posed for the base
        // cost, so the mana the value adds is a slot no client was offered and therefore
        // none filled.
        return fallback_payment(state, db, card, x);
    }
    let (Some(discards), Some(sacrifices)) = (
        bind_discards(state, db, card, targets),
        bind_sacrifices(state, db, card, targets),
    ) else {
        // The non-mana half was owed and not answered (or answered with an object that has
        // since moved): the server pays the whole cost rather than half of it.
        return fallback_payment(state, db, card, x);
    };
    chosen.extend(discards);
    chosen.extend(sacrifices);
    chosen
}

/// The permanents a returned selection sacrifices to `card`'s additional cost, or `None`
/// when the cost is owed and the answer does not pay it.
///
/// The battlefield counterpart of [`bind_discards`], with the same two rules: a card with
/// no such cost accepts no answer at all, and one with a cost is paid by exactly what it
/// asks for — where "exactly" for an open count means anything from none to every
/// candidate, which is precisely what such a cost asks.
fn bind_sacrifices(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    targets: &[TargetChoice],
) -> Option<Vec<CostPayment>> {
    let answered = chosen_for(targets, SACRIFICE_SLOT);
    let Some(cost) = sage_engine::sacrifice_cost(state, db, card) else {
        return answered.is_empty().then(Vec::new);
    };
    bind_chosen_sacrifices(&cost, answered)
}

/// Resolve a sacrifice slot's answer against `cost`'s **freshly recomputed** candidates,
/// so a permanent that died, changed hands, or arrived since the view went out cannot be
/// named. Shared by the cast and the activation paths, which pose the slot identically.
///
/// `None` when the answer is not a legal payment of this cost — the wrong number of ids,
/// or an id naming nothing the cost accepts — which is the caller's signal to pay the
/// whole cost itself rather than half of it.
pub(super) fn bind_chosen_sacrifices(
    cost: &sage_engine::SacrificeCost,
    answered: &[String],
) -> Option<Vec<CostPayment>> {
    if !cost.count.is_paid_by(answered.len()) {
        return None;
    }
    answered
        .iter()
        .map(|id| {
            cost.candidates
                .iter()
                .find(|candidate| permanent_entity_id(**candidate) == *id)
                .map(|candidate| CostPayment::Sacrifice(*candidate))
        })
        .collect()
}

/// The cards a returned selection discards to `card`'s additional cost, or `None` when the
/// cost is owed and the answer does not pay it.
///
/// `Some(empty)` for the overwhelming majority of cards, which have no additional cost —
/// and, deliberately, for those an answer naming any discard at all is refused, because a
/// cost is paid for exactly what it asks.
fn bind_discards(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    targets: &[TargetChoice],
) -> Option<Vec<CostPayment>> {
    let answered = chosen_for(targets, DISCARD_SLOT);
    let Some(cost) = sage_engine::discard_cost(state, db, card) else {
        return answered.is_empty().then(Vec::new);
    };
    if answered.len() != usize::from(cost.count) {
        return None;
    }
    // Resolved against the **freshly recomputed** candidates, so a card discarded, drawn
    // into, or otherwise moved since the view went out cannot be named.
    answered
        .iter()
        .map(|id| {
            cost.candidates
                .iter()
                .find(|candidate| card_entity_id(**candidate) == *id)
                .map(|candidate| CostPayment::Discard(*candidate))
        })
        .collect()
}

/// The payment the server assembles when the client did not (ADR 0010).
fn fallback_payment(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    x: Option<u32>,
) -> Vec<CostPayment> {
    sage_engine::auto_payment(state, db, card, x).unwrap_or_default()
}

#[cfg(test)]
mod tests;
