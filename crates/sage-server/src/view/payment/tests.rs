//! Tests for posing a cast's cost as slots and binding the answers back.
//!
//! Split out of the parent module for size (`docs/coding-standards.md`); pure code motion,
//! with the fixtures moving beside the tests that use them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::test_support::fixture;
use crate::view::test_support::{put_permanent, state_with_hand};
use crate::view::{projected_actions, resolve_action};

/// Player 0's main phase with `lands` on the battlefield and `spell` in hand.
fn table(lands: &[&str], spell: &str) -> (GameState, CardDatabase, Vec<PermanentId>, CardInstance) {
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
        .find(|view| view.kind == "cast_spell" && view.subject.contains(&card_entity_id(card.id)))
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

/// **"Any number" is bounds on the wire, not a rule the client learns** (issue #721).
/// A cost the player sizes is `min: 0` over every candidate — the same shape a scry
/// already poses — so a client that can render "pick between none and all of these"
/// renders Scapeshift without knowing what a land is.
#[test]
fn issue_721_an_any_number_sacrifice_is_posed_with_a_minimum_of_none() {
    let (mut state, db, lands, spell) =
        table(&["forest", "forest", "forest", "forest"], "scapeshift");
    // A creature is not a land, so it never reaches the list the client picks from.
    put_permanent(
        &mut state,
        fixture("centaur_courser"),
        PlayerId(0),
        false,
        false,
    );
    state.players[0].library = vec![state.new_instance(fixture("mountain"))];
    // Float the mana so the cast poses no pips and the sacrifice is the only slot the
    // answer has to fill — this test is about the bounds, not about paying for it.
    state.players[0].mana_pool.add(sage_engine::Color::Green, 2);
    state.players[0].mana_pool.add_colorless(2);

    let action = cast_action(&state, &db, spell);
    let slot = action
        .prompts
        .iter()
        .find_map(|prompt| match prompt {
            Prompt::SelectFromZone {
                slot,
                prompt,
                count,
                min,
                candidates,
                ..
            } if slot == SACRIFICE_SLOT => Some((prompt.clone(), *count, *min, candidates.clone())),
            _ => None,
        })
        .expect("the sacrifice is posed as a slot");
    assert_eq!(slot.0, "Sacrifice any number of lands");
    assert_eq!(slot.1, 4, "at most every land on the board");
    assert_eq!(slot.2, Some(0), "and at least none of them");
    assert_eq!(slot.3.len(), 4, "only the lands");

    // Two of them, chosen by the player, really are what the cast eats.
    let bound = resolve_action(
        &state,
        &db,
        PlayerId(0),
        &ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: SACRIFICE_SLOT.to_string(),
                chosen: vec![permanent_entity_id(lands[0]), permanent_entity_id(lands[1])],
            }],
            ..Default::default()
        },
    )
    .expect("the answer binds");
    let after = sage_engine::apply_action(&state, &bound, &db);
    assert!(!after.battlefield.iter().any(|perm| perm.id == lands[0]));
    assert!(!after.battlefield.iter().any(|perm| perm.id == lands[1]));
    assert!(after.battlefield.iter().any(|perm| perm.id == lands[2]));
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
