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

/// The slot id for the `n`th pip of a cast's remaining cost. Recomputed (never parsed)
/// on resolution, like every other slot id.
fn pip_slot(index: usize) -> String {
    format!("pay_{index}")
}

/// The slot a cast's additional-cost discard is answered on (CR 601.2b).
const DISCARD_SLOT: &str = "cost_discard";

/// The slot a cast's additional-cost sacrifice is answered on (CR 601.2b / 701.17).
const SACRIFICE_SLOT: &str = "cost_sacrifice";

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
        prompts.push(Prompt::SelectFromZone {
            slot: SACRIFICE_SLOT.to_string(),
            prompt: format!(
                "Sacrifice a {}",
                crate::rules_text::card_type_word(sacrifice.card_type)
            ),
            zone: "battlefield".to_string(),
            owner: player_id(state.priority),
            count: 1,
            min: None,
            candidates: sacrifice
                .candidates
                .into_iter()
                .map(permanent_entity_id)
                .collect(),
        });
    }
    prompts
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
    let (Some(discards), Some(sacrifices)) = (
        bind_discards(state, db, card, targets),
        bind_sacrifices(state, db, card, targets),
    ) else {
        // The non-mana half was owed and not answered (or answered with an object that has
        // since moved): the server pays the whole cost rather than half of it.
        return fallback_payment(state, db, card);
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
/// asks for.
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
    if answered.len() != 1 {
        return None;
    }
    // Resolved against the **freshly recomputed** candidates, so a permanent that died,
    // changed hands, or arrived since the view went out cannot be named.
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
fn fallback_payment(state: &GameState, db: &CardDatabase, card: CardInstance) -> Vec<CostPayment> {
    sage_engine::auto_payment(state, db, card).unwrap_or_default()
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

    /// Each way to pay a pip says whether spending it turns the card, so a client can
    /// draw a payment it is still assembling — the board the player is looking at while
    /// they pick sources is one the server has not been told about yet.
    #[test]
    fn every_way_to_pay_states_whether_it_taps_its_source() {
        let (state, db, _lands, spell) = table(&["plains", "plains"], "ajani_s_pridemate");
        let action = cast_action(&state, &db, spell);
        let Some(Prompt::PayMana { candidates, .. }) = action
            .prompts
            .iter()
            .find(|p| matches!(p, Prompt::PayMana { .. }))
        else {
            panic!("a pay_mana slot")
        };
        assert!(!candidates.is_empty(), "the Plains can pay it");
        assert!(
            candidates.iter().all(|option| option.taps),
            "a land's `{{T}}: Add {{W}}` taps it (CR 602.2a): {candidates:?}"
        );
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

    /// A sacrifice cost is posed on the slot shape that already existed for choosing
    /// objects out of a zone — over the battlefield rather than the hand. No new prompt
    /// kind, because `zone` was always a free-form string for exactly this.
    #[test]
    fn a_sacrifice_cost_is_posed_over_the_battlefield_and_bound_back() {
        let (mut state, db, _lands, spell) =
            table(&["swamp", "swamp", "swamp", "swamp"], "blood_divination");
        let mine = put_permanent(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(0),
            false,
            false,
        );
        // An opponent's creature is not a candidate: CR 701.17b lets a player sacrifice
        // only what they control, and the candidate list is where that is enforced for the
        // client rather than left for it to know.
        put_permanent(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(1),
            false,
            false,
        );

        let action = cast_action(&state, &db, spell);
        let slot = action
            .prompts
            .iter()
            .find_map(|prompt| match prompt {
                Prompt::SelectFromZone {
                    slot,
                    zone,
                    count,
                    candidates,
                    ..
                } if slot == SACRIFICE_SLOT => Some((zone.clone(), *count, candidates.clone())),
                _ => None,
            })
            .expect("the sacrifice is posed as a slot");
        assert_eq!(slot.0, "battlefield");
        assert_eq!(slot.1, 1, "a cost is paid for exactly what it asks");
        assert_eq!(
            slot.2,
            vec![permanent_entity_id(mine)],
            "only the caster's own creature is a candidate"
        );

        // The answer binds back into the payment, and the cast really spends it.
        let bound = resolve_action(
            &state,
            &db,
            PlayerId(0),
            &ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets: vec![TargetChoice {
                    slot: SACRIFICE_SLOT.to_string(),
                    chosen: vec![permanent_entity_id(mine)],
                }],
                ..Default::default()
            },
        )
        .expect("the answer binds");
        let after = sage_engine::apply_action(&state, &bound, &db);
        assert!(
            !after.battlefield.iter().any(|perm| perm.id == mine),
            "the named creature was sacrificed to the cost"
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

    /// **The additional cost is a slot, not something done to the player.** Tormenting
    /// Voice discards a card as part of casting it, so the cast carries a select-from-zone
    /// over the hand — and the spell itself is not in it, because it is on its way to the
    /// stack.
    #[test]
    fn an_additional_cost_is_posed_as_a_choice_over_the_hand() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = state_with_hand(&[fixture("tormenting_voice"), fixture("murder")]);
        state.turn = 3;
        for _ in 0..2 {
            put_permanent(&mut state, fixture("mountain"), PlayerId(0), false, false);
        }
        let (voice, murder) = (hand[0], hand[1]);

        let action = cast_action(&state, &db, voice);
        let discard = action
            .prompts
            .iter()
            .find_map(|prompt| match prompt {
                Prompt::SelectFromZone {
                    slot,
                    zone,
                    count,
                    candidates,
                    ..
                } if slot == DISCARD_SLOT => Some((zone, *count, candidates)),
                _ => None,
            })
            .expect("the discard is asked for");
        assert_eq!(discard.0, "hand");
        assert_eq!(discard.1, 1, "one card, exactly");
        assert_eq!(
            discard.2,
            &vec![card_entity_id(murder.id)],
            "the Voice cannot pay for itself"
        );
    }

    /// And the card the player picks is the card that goes to the graveyard.
    #[test]
    fn the_card_the_player_picks_is_the_one_discarded() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = state_with_hand(&[
            fixture("tormenting_voice"),
            fixture("murder"),
            fixture("shock"),
        ]);
        state.turn = 3;
        for _ in 0..2 {
            put_permanent(&mut state, fixture("mountain"), PlayerId(0), false, false);
        }
        let (voice, shock) = (hand[0], hand[2]);

        let action = cast_action(&state, &db, voice);
        let mut answers = vec![TargetChoice {
            slot: DISCARD_SLOT.to_string(),
            chosen: vec![card_entity_id(shock.id)],
        }];
        // Pay the mana by hand too, so this is one whole assembled payment.
        for prompt in &action.prompts {
            let Prompt::PayMana {
                slot, candidates, ..
            } = prompt
            else {
                continue;
            };
            let taken: Vec<&String> = answers.iter().flat_map(|a| a.chosen.iter()).collect();
            let picked = candidates
                .iter()
                .find(|candidate| !taken.contains(&&candidate.id))
                .expect("a source for this pip");
            answers.push(TargetChoice {
                slot: slot.clone(),
                chosen: vec![picked.id.clone()],
            });
        }

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
        assert_eq!(after.stack.len(), 1, "the Voice was cast");
        assert!(
            after.players[0].graveyard.iter().any(|c| c.id == shock.id),
            "the Shock the player chose is in the graveyard"
        );
        assert!(
            after.players[0].hand.iter().any(|c| c.id == hand[1].id),
            "and the Murder they kept is still in hand"
        );
        assert!(
            sage_engine::pending_player_choice(&after).is_none(),
            "nothing is owed afterwards — the cost was paid as part of casting"
        );
    }

    /// A client that answers nothing is still paid for, discard included — which is what
    /// keeps the terminal client and both automated players casting the same cards.
    #[test]
    fn an_unanswered_additional_cost_is_paid_by_the_server() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = state_with_hand(&[fixture("tormenting_voice"), fixture("murder")]);
        state.turn = 3;
        for _ in 0..2 {
            put_permanent(&mut state, fixture("mountain"), PlayerId(0), false, false);
        }
        let action = cast_action(&state, &db, hand[0]);
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
        assert_eq!(after.stack.len(), 1);
        assert!(after.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == hand[1].id));
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
