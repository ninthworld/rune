//! Posing a cast's cost as slots, and binding the answers back into a payment.
//!
//! The engine says what is still owed and what could pay each unit
//! ([`sage_engine::payment_pips`]); this turns that into `pay_mana` slots, and turns the
//! ids that come back into [`CostPayment`] entries. Nothing here decides what anything
//! costs.
//!
//! **Any pip the client leaves unanswered, the server pays** ([`bind_payment`]). That is
//! ADR 0010's line exactly: the engine answers *what a legal payment is* and the server
//! decides *whether to tap for the player instead of asking*. It is also what keeps every
//! existing consumer working — the terminal client, the deterministic agent, and the AI
//! all skip these slots, and all of them assume that an action on offer is one they can
//! take.

use super::*;

/// The slot id for the `n`th pip of a cast's remaining cost. Recomputed (never parsed)
/// on resolution, like every other slot id.
fn pip_slot(index: usize) -> String {
    format!("pay_{index}")
}

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
) -> Vec<Prompt> {
    let Some(pips) = sage_engine::payment_pips(state, db, card) else {
        return Vec::new();
    };
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
                })
                .collect(),
            pip: pip.pip,
        })
        .collect()
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
            abilities_of_permanent(db, perm)
                .get(source.index)
                .map(|ability| mana_ability_pips(ability).join(""))
        })
        .unwrap_or_default()
}

/// Bind a returned selection onto the payment for casting `card`.
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
    targets: &[TargetChoice],
) -> Vec<CostPayment> {
    let pips = sage_engine::payment_pips(state, db, card).unwrap_or_default();
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
            return fallback_payment(state, db, card);
        };
        if spent.contains(&source.permanent) {
            return fallback_payment(state, db, card);
        }
        spent.push(source.permanent);
        chosen.push(CostPayment::Mana(*source));
    }
    if chosen.len() < pips.len() {
        // A payment the player did not finish. The server pays the whole thing rather
        // than half of it — see the note above on why this is not a top-up.
        return fallback_payment(state, db, card);
    }
    chosen.extend(auto_non_mana_costs(state, db, card));
    chosen
}

/// The payment the server assembles when the client did not (ADR 0010).
fn fallback_payment(state: &GameState, db: &CardDatabase, card: CardInstance) -> Vec<CostPayment> {
    sage_engine::auto_payment(state, db, card).unwrap_or_default()
}

/// The non-mana half of a cost, which no slot poses yet: the discards of an additional
/// cost (CR 601.2b). Taken from the engine's own legal payment.
///
/// **This is the server choosing a card out of a player's hand for them**, and it is a
/// placeholder rather than a design: the client has no cost slot for a discard, so the
/// alternative is refusing a cast the player was offered. It comes out the moment those
/// slots exist.
fn auto_non_mana_costs(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
) -> Vec<CostPayment> {
    sage_engine::auto_payment(state, db, card)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.discard().is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;
    use crate::view::test_support::{put_permanent, state_with_hand};
    use crate::view::{projected_actions, resolve_action};

    /// Player 0's main phase with `lands` on the battlefield and `spell` in hand.
    fn table(
        lands: &[&str],
        spell: &str,
    ) -> (GameState, CardDatabase, Vec<PermanentId>, CardInstance) {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = state_with_hand(&[fixture(spell)]);
        state.turn = 3;
        let placed = lands
            .iter()
            .map(|slug| put_permanent(&mut state, fixture(slug), PlayerId(0), false, false))
            .collect();
        (state, db, placed, hand[0])
    }

    /// The cast a client is offered for `card`.
    fn cast_action(state: &GameState, db: &CardDatabase, card: CardInstance) -> ValidAction {
        projected_actions(state, db)
            .into_iter()
            .map(|projected| projected.view)
            .find(|view| {
                view.kind == "cast_spell" && view.subject.contains(&card_entity_id(card.id))
            })
            .expect("the cast is on offer")
    }

    /// The scenario, on the wire: four Plains, a {1}{W} creature in hand, nothing
    /// floating. The cast is **offered** — that is the widened offer — and it carries its
    /// unpaid cost as two pips, so a client can name the card first and pay second.
    #[test]
    fn a_cast_is_offered_with_its_unpaid_cost_as_pips() {
        let (state, db, lands, spell) = table(
            &["plains", "plains", "plains", "plains"],
            "ajani_s_pridemate",
        );
        let action = cast_action(&state, &db, spell);

        let pips: Vec<&Prompt> = action
            .prompts
            .iter()
            .filter(|p| matches!(p, Prompt::PayMana { .. }))
            .collect();
        assert_eq!(pips.len(), 2, "{{1}}{{W}} is two pips");

        let Prompt::PayMana {
            pip, candidates, ..
        } = pips[0]
        else {
            panic!("a pay_mana slot")
        };
        assert_eq!(pip, "{W}");
        assert_eq!(candidates.len(), 4, "any Plains pays it");
        for candidate in candidates {
            assert!(lands
                .iter()
                .any(|land| permanent_entity_id(*land) == candidate.source));
        }
    }

    /// The payment a player assembles is the one that gets spent: the two Plains they
    /// named are tapped and no others.
    #[test]
    fn the_sources_a_client_names_are_the_ones_spent() {
        let (state, db, lands, spell) = table(
            &["plains", "plains", "plains", "plains"],
            "ajani_s_pridemate",
        );
        let action = cast_action(&state, &db, spell);
        // Answer both pips with the *last* two Plains, which auto-pay would not have
        // reached for — so the assertion is about the player's choice, not a coincidence.
        let answers: Vec<TargetChoice> = action
            .prompts
            .iter()
            .enumerate()
            .filter_map(|(index, prompt)| {
                let Prompt::PayMana {
                    slot, candidates, ..
                } = prompt
                else {
                    return None;
                };
                let wanted = permanent_entity_id(lands[3 - index]);
                let picked = candidates
                    .iter()
                    .find(|candidate| candidate.source == wanted)?;
                Some(TargetChoice {
                    slot: slot.clone(),
                    chosen: vec![picked.id.clone()],
                })
            })
            .collect();
        assert_eq!(answers.len(), 2);

        let bound = resolve_action(
            &state,
            &db,
            PlayerId(0),
            &ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets: answers,
                ..Default::default()
            },
        )
        .expect("the answer binds");

        let after = sage_engine::apply_action(&state, &bound, &db);
        assert_eq!(after.stack.len(), 1, "the spell was cast");
        let tapped: Vec<PermanentId> = after
            .battlefield
            .iter()
            .filter(|perm| perm.tapped)
            .map(|perm| perm.id)
            .collect();
        assert_eq!(tapped.len(), 2);
        assert!(tapped.contains(&lands[3]) && tapped.contains(&lands[2]));
    }

    /// A client that answers no pip is tapped out for (ADR 0010) — which is what keeps
    /// the terminal client and both automated players working against the same offer.
    #[test]
    fn an_unanswered_payment_is_paid_by_the_server() {
        let (state, db, _lands, spell) = table(
            &["plains", "plains", "plains", "plains"],
            "ajani_s_pridemate",
        );
        let action = cast_action(&state, &db, spell);
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
        assert_eq!(after.stack.len(), 1, "the server paid for it");
        assert_eq!(
            after.battlefield.iter().filter(|p| p.tapped).count(),
            2,
            "and tapped no more than the cost"
        );
    }

    /// **The dual-land question, as the client sees it.** Both halves of a River are
    /// offered for the generic pip? No — once, because the answer cannot matter. For a
    /// colored pip only the half that pays that color is offered. Either way the client's
    /// rule is the same: ask when one `source` appears twice.
    #[test]
    fn a_dual_land_is_offered_once_where_the_choice_cannot_matter() {
        let (state, db, lands, spell) = table(&["meandering_river", "plains"], "ajani_s_pridemate");
        let action = cast_action(&state, &db, spell);
        let river = permanent_entity_id(lands[0]);

        for prompt in &action.prompts {
            let Prompt::PayMana {
                pip, candidates, ..
            } = prompt
            else {
                continue;
            };
            let ways = candidates
                .iter()
                .filter(|candidate| candidate.source == river)
                .count();
            assert_eq!(ways, 1, "the River is offered once for {pip}");
        }
    }

    /// A payment naming one permanent for two pips is not a payment — a land taps once.
    /// The server refuses it rather than letting a double-spend through.
    #[test]
    fn one_permanent_cannot_pay_two_pips() {
        let (state, db, lands, spell) = table(&["plains", "plains"], "ajani_s_pridemate");
        let action = cast_action(&state, &db, spell);
        let wanted = permanent_entity_id(lands[0]);
        let answers: Vec<TargetChoice> = action
            .prompts
            .iter()
            .filter_map(|prompt| {
                let Prompt::PayMana {
                    slot, candidates, ..
                } = prompt
                else {
                    return None;
                };
                let picked = candidates
                    .iter()
                    .find(|candidate| candidate.source == wanted)?;
                Some(TargetChoice {
                    slot: slot.clone(),
                    chosen: vec![picked.id.clone()],
                })
            })
            .collect();
        assert_eq!(answers.len(), 2, "the same Plains named for both pips");

        let bound = resolve_action(
            &state,
            &db,
            PlayerId(0),
            &ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets: answers,
                ..Default::default()
            },
        )
        .expect("it binds — to the server's own payment");

        let after = sage_engine::apply_action(&state, &bound, &db);
        assert_eq!(after.stack.len(), 1);
        assert_eq!(
            after.battlefield.iter().filter(|p| p.tapped).count(),
            2,
            "two distinct lands paid, not one twice"
        );
    }
}
