//! Posing an **activation's** chosen costs as slots, and binding the answers back.
//!
//! The activation counterpart of its parent module, and deliberately the same two slots
//! over the same two zones: `Sacrifice another creature` is a `select_from_zone` over the
//! battlefield and `Discard a card` one over the hand, exactly as a cast's additional cost
//! poses them. A client that renders one renders the other, and there is no new wire shape
//! and nothing action-kind-specific on either side.
//!
//! One thing differs, and only one: an activation names **no mana** in its payment. Mana is
//! paid from the pool (CR 602.2b) by activating mana abilities as actions in their own
//! right, which is what every client already does and what the ability's offer is already
//! gated on. So there are no pips here, and what is unanswered the server pays for
//! ([`sage_engine::auto_activation_payment`] — ADR 0010's line, the engine says what a
//! legal payment is and the server decides whether to pay it for the player).

use sage_engine::{Ability, Cost};

use super::*;

/// One `select_from_zone` slot per component of an activation's cost that the player picks
/// the payment for (CR 601.2b) — none at all for the overwhelming majority of abilities,
/// whose cost is entirely about their own source and the pool.
///
/// The slot ids are the cast's, because the questions are: a client that already answers
/// `cost_sacrifice` on a cast answers it here without learning anything. Each is exact —
/// `count` with no `min` — because a cost is paid for what it asks and not for less.
pub(crate) fn activation_payment_prompts(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Vec<Prompt> {
    let mut prompts = Vec::new();
    if let Some(discard) = sage_engine::activation_discard_cost(state, db, permanent, index) {
        prompts.push(Prompt::SelectFromZone {
            slot: DISCARD_SLOT.to_string(),
            prompt: cost_prompt(state, db, permanent, index, |cost| {
                matches!(cost, Cost::Discard { .. })
            }),
            zone: "hand".to_string(),
            owner: player_id(activator(state, permanent)),
            count: u32::from(discard.count),
            min: None,
            candidates: discard.candidates.into_iter().map(card_entity_id).collect(),
        });
    }
    if let Some(candidates) =
        sage_engine::activation_sacrifice_candidates(state, db, permanent, index)
    {
        prompts.push(Prompt::SelectFromZone {
            slot: SACRIFICE_SLOT.to_string(),
            prompt: cost_prompt(state, db, permanent, index, |cost| {
                matches!(cost, Cost::Sacrifice { .. })
            }),
            zone: "battlefield".to_string(),
            owner: player_id(activator(state, permanent)),
            count: 1,
            min: None,
            candidates: candidates.into_iter().map(permanent_entity_id).collect(),
        });
    }
    prompts
}

/// The question a cost component asks, in the **card's own words** — the same
/// [`crate::rules_text::cost_symbol`] that writes the cost line, so "Sacrifice another
/// creature" on the card and in the prompt is one string rather than two that must be kept
/// in step. Empty for a component that is not there, which cannot happen for a slot the
/// engine posed.
fn cost_prompt(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    wanted: impl Fn(&Cost) -> bool,
) -> String {
    activation_cost(state, db, permanent, index)
        .iter()
        .find(|component| wanted(component))
        .map(crate::rules_text::cost_symbol)
        .unwrap_or_default()
}

/// The cost of `permanent`'s activated ability `index`, or empty for a permanent that has
/// left or an index naming no activated ability.
fn activation_cost(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> Vec<Cost> {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == permanent)
        .and_then(
            |perm| match abilities_of_permanent(state, db, perm).get(index) {
                Some(Ability::Activated { cost, .. }) => Some(cost.clone()),
                _ => None,
            },
        )
        .unwrap_or_default()
}

/// Who pays for an activation: the permanent's **computed** controller (CR 613 layer 2), so
/// a stolen creature's ability is paid for by whoever has it now. The priority holder for a
/// permanent that has left, which is the seat the slot would be shown to anyway.
fn activator(state: &GameState, permanent: PermanentId) -> PlayerId {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == permanent)
        .map_or(state.priority, |perm| {
            sage_engine::controller_of(state, perm)
        })
}

/// Bind a returned selection onto the payment for activating `permanent`'s ability `index`.
///
/// Resolved against the **freshly recomputed** candidates, so an id naming a permanent that
/// has died or changed hands, or a card that has left the hand, since the view went out is
/// refused rather than smuggled through. Anything the client left unanswered — which is
/// every client that has not learned these slots, including the terminal one and both
/// automated players — the server pays, over the whole cost rather than topping up a
/// partial answer, for the reason [`bind_payment`] does not top up either.
pub(crate) fn bind_activation_payment(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    targets: &[TargetChoice],
) -> Vec<CostPayment> {
    let fallback =
        || sage_engine::auto_activation_payment(state, db, permanent, index).unwrap_or_default();
    let mut chosen = Vec::new();

    match sage_engine::activation_sacrifice_candidates(state, db, permanent, index) {
        Some(candidates) => {
            let answered = chosen_for(targets, SACRIFICE_SLOT);
            let [id] = answered else {
                return fallback();
            };
            let Some(picked) = candidates
                .iter()
                .find(|candidate| permanent_entity_id(**candidate) == *id)
            else {
                return fallback();
            };
            chosen.push(CostPayment::Sacrifice(*picked));
        }
        // A cost that asks for no sacrifice is paid by none: an answer naming one is a
        // wrong answer, not a generous one.
        None if !chosen_for(targets, SACRIFICE_SLOT).is_empty() => return fallback(),
        None => {}
    }

    match sage_engine::activation_discard_cost(state, db, permanent, index) {
        Some(cost) => {
            let answered = chosen_for(targets, DISCARD_SLOT);
            if answered.len() != usize::from(cost.count) {
                return fallback();
            }
            for id in answered {
                let Some(picked) = cost
                    .candidates
                    .iter()
                    .find(|candidate| card_entity_id(**candidate) == *id)
                else {
                    return fallback();
                };
                chosen.push(CostPayment::Discard(*picked));
            }
        }
        None if !chosen_for(targets, DISCARD_SLOT).is_empty() => return fallback(),
        None => {}
    }

    chosen
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;
    use crate::view::test_support::{put_permanent, state_with_hand};
    use crate::view::{projected_actions, resolve_action};

    /// The activation of `permanent`'s ability the client is offered, whichever index it
    /// sits at — found by the words on the button rather than by an index this test would
    /// otherwise have to know.
    fn activation(
        state: &GameState,
        db: &CardDatabase,
        permanent: PermanentId,
        contains: &str,
    ) -> ValidAction {
        projected_actions(state, db)
            .into_iter()
            .map(|projected| projected.view)
            .find(|view| {
                view.kind == "activate_ability"
                    && view.subject.contains(&permanent_entity_id(permanent))
                    && view.label.contains(contains)
            })
            .expect("the activation is on offer")
    }

    /// The slot the sacrifice is asked on, with its zone, count, and candidates.
    fn zone_slot(action: &ValidAction, wanted: &str) -> (String, String, u32, Vec<String>) {
        action
            .prompts
            .iter()
            .find_map(|prompt| match prompt {
                Prompt::SelectFromZone {
                    slot,
                    prompt,
                    zone,
                    count,
                    candidates,
                    ..
                } if slot == wanted => {
                    Some((prompt.clone(), zone.clone(), *count, candidates.clone()))
                }
                _ => None,
            })
            .expect("the cost is posed as a slot")
    }

    /// An activation's sacrifice is posed over the battlefield, in the card's own words,
    /// and only over permanents the activator may actually sacrifice (CR 701.17b — and the
    /// *another* of the cost, which takes the source itself off the list).
    #[test]
    fn a_sacrifice_cost_is_posed_over_the_battlefield_and_bound_back() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, _hand) = state_with_hand(&[]);
        state.turn = 3;
        let harpy = put_permanent(
            &mut state,
            fixture("ravenous_harpy"),
            PlayerId(0),
            false,
            false,
        );
        let food = put_permanent(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(0),
            false,
            false,
        );
        // An opponent's creature is never a candidate.
        put_permanent(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(1),
            false,
            false,
        );
        state.players[0].mana_pool.add(sage_engine::Color::Black, 1);

        let action = activation(&state, &db, harpy, "Sacrifice");
        let (prompt, zone, count, candidates) = zone_slot(&action, SACRIFICE_SLOT);
        assert_eq!(prompt, "Sacrifice another creature");
        assert_eq!(zone, "battlefield");
        assert_eq!(count, 1, "a cost is paid for exactly what it asks");
        assert_eq!(
            candidates,
            vec![permanent_entity_id(food)],
            "only the activator's other creature can pay it"
        );

        let bound = resolve_action(
            &state,
            &db,
            PlayerId(0),
            &ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets: vec![TargetChoice {
                    slot: SACRIFICE_SLOT.to_string(),
                    chosen: vec![permanent_entity_id(food)],
                }],
                ..Default::default()
            },
        )
        .expect("the answer binds");
        let after = sage_engine::apply_action(&state, &bound, &db);
        assert!(
            !after.battlefield.iter().any(|perm| perm.id == food),
            "the named creature was sacrificed to the cost"
        );
    }

    /// A discard cost rides the same slot over the hand, and the card the player picks is
    /// the one that goes to the graveyard.
    #[test]
    fn a_discard_cost_is_posed_over_the_hand_and_binds_the_chosen_card() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = state_with_hand(&[fixture("murder"), fixture("shock")]);
        state.turn = 3;
        let pyromancer = put_permanent(
            &mut state,
            fixture("dismissive_pyromancer"),
            PlayerId(0),
            false,
            false,
        );

        let action = activation(&state, &db, pyromancer, "Discard");
        let (prompt, zone, count, candidates) = zone_slot(&action, DISCARD_SLOT);
        assert_eq!(prompt, "Discard a card");
        assert_eq!(zone, "hand");
        assert_eq!(count, 1);
        assert_eq!(candidates.len(), 2, "either card in hand pays it");

        let bound = resolve_action(
            &state,
            &db,
            PlayerId(0),
            &ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets: vec![TargetChoice {
                    slot: DISCARD_SLOT.to_string(),
                    chosen: vec![card_entity_id(hand[1].id)],
                }],
                ..Default::default()
            },
        )
        .expect("the answer binds");
        let after = sage_engine::apply_action(&state, &bound, &db);
        assert!(
            after.players[0]
                .graveyard
                .iter()
                .any(|card| card.id == hand[1].id),
            "the Shock the player chose is in the graveyard"
        );
        assert!(
            after.players[0]
                .hand
                .iter()
                .any(|card| card.id == hand[0].id),
            "and the Murder they kept is still in hand"
        );
    }

    /// A client that answers nothing is still paid for (ADR 0010) — which is what keeps the
    /// terminal client and both automated players activating the same abilities.
    #[test]
    fn an_unanswered_activation_cost_is_paid_by_the_server() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, _hand) = state_with_hand(&[fixture("murder")]);
        state.turn = 3;
        let pyromancer = put_permanent(
            &mut state,
            fixture("dismissive_pyromancer"),
            PlayerId(0),
            false,
            false,
        );

        let action = activation(&state, &db, pyromancer, "Discard");
        let bound = resolve_action(
            &state,
            &db,
            PlayerId(0),
            &ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets: Vec::new(),
                ..Default::default()
            },
        )
        .expect("the answer binds");
        let after = sage_engine::apply_action(&state, &bound, &db);
        assert_eq!(after.players[0].graveyard.len(), 1, "the server paid it");
        assert_eq!(after.stack.len(), 1, "and the ability is on the stack");
    }
}
