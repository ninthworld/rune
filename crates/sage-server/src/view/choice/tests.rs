//! The mid-resolution choice projection, checked against the views a client reads and
//! the actions its answers bind back to.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::test_support::fixture;

/// A two-player state, one turn in, with seat 0 holding priority in its main
/// phase and both pools stocked so payability decides nothing.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    for player in &mut state.players {
        for colour in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            player.mana_pool.add(colour, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    state
}

/// Cast `slug` from seat 0's hand at `targets` and let both players pass, so the
/// spell resolves and any choice it poses is owed.
fn cast_and_resolve(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(fixture(slug));
    state.players[0].hand.push(instance);
    let state = sage_engine::apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let state = sage_engine::apply_action(&state, &Action::PassPriority, db);
    sage_engine::apply_action(&state, &Action::PassPriority, db)
}

/// The `player_choice` action on a seat's view, if it is offered one.
fn choice_action(view: &GameView) -> Option<&ValidAction> {
    view.valid_actions
        .iter()
        .find(|a| a.kind == "player_choice")
}

#[test]
fn issue_604_the_choice_projects_as_one_select_from_zone_bound_to_its_chooser() {
    // Mind Rot: the targeted seat is offered a single `player_choice` action whose
    // one prompt carries the engine's own candidates and exact count. The caster is
    // offered nothing at all — the engine hands priority to the chooser, and a
    // non-priority seat's view carries no actions.
    let db = CardDatabase::bundled().unwrap();
    let mut state = main_phase();
    for slug in ["forest", "shock", "murder"] {
        let inst = state.new_instance(fixture(slug));
        state.players[1].hand.push(inst);
    }
    let state = cast_and_resolve(&state, &db, "mind_rot", vec![Target::Player(PlayerId(1))]);

    let chooser = personalized_view(&state, &db, PlayerId(1));
    let action = choice_action(&chooser).expect("the targeted seat is asked");
    assert!(!action.token.is_empty(), "a prompt action is token-bound");
    assert_eq!(action.prompts.len(), 1);
    let Prompt::SelectFromZone {
        slot,
        zone,
        owner,
        count,
        min,
        candidates,
        ..
    } = &action.prompts[0]
    else {
        panic!("the choice is a select_from_zone");
    };
    assert_eq!(slot, CHOICE_SLOT);
    assert_eq!(zone, "hand");
    assert_eq!(owner, &player_id(PlayerId(1)));
    assert_eq!(*count, 2);
    assert_eq!(*min, None, "an exact discard states no lower bound");
    assert_eq!(candidates.len(), 3);

    // The caster sees no decision surface while the other seat is answering.
    let caster = personalized_view(&state, &db, PlayerId(0));
    assert!(caster.valid_actions.is_empty());

    // The answer round-trips back to the engine action, preserving the order sent.
    let picked: Vec<String> = vec![candidates[2].clone(), candidates[0].clone()];
    let answer = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: picked.clone(),
        }],
        ..Default::default()
    };
    let Some(Action::AnswerChoice { chosen }) = resolve_action(&state, &db, PlayerId(1), &answer)
    else {
        panic!("the answer resolves to an AnswerChoice");
    };
    assert_eq!(
        chosen
            .iter()
            .map(|id| card_entity_id(*id))
            .collect::<Vec<_>>(),
        picked,
        "the chosen order is carried through, not re-sorted",
    );

    // An id the offer did not name is rejected rather than dropped.
    let forged = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: vec!["card_999999".to_string()],
        }],
        ..Default::default()
    };
    assert!(resolve_action(&state, &db, PlayerId(1), &forged).is_none());
}

#[test]
fn issue_604_a_searched_library_reaches_the_searcher_and_no_other_seat() {
    // The hidden-information rule, stated as a leak test: seat 0 searches its own
    // library, so seat 0's view carries those cards and seat 1's — and a
    // spectator's — carry no trace of them anywhere in the serialized payload.
    let db = CardDatabase::bundled().unwrap();
    let mut state = main_phase();
    let caller = PermanentId(state.mint_id());
    let instance = state.new_instance(fixture("elvish_clancaller"));
    state.battlefield.push(sage_engine::Permanent {
        id: caller,
        instance: instance.id,
        printed: fixture("elvish_clancaller").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    let library: Vec<CardInstance> = ["forest", "elvish_clancaller", "island"]
        .iter()
        .map(|slug| state.new_instance(fixture(slug)))
        .collect();
    state.players[0].library = library.clone();

    let state = sage_engine::apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: caller,
            index: 1,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    assert!(
        pending_player_choice(&state).is_some(),
        "the search is waiting on its controller",
    );

    // The searcher sees the one card the search may find.
    let searcher = personalized_view(&state, &db, PlayerId(0));
    assert_eq!(searcher.revealed.len(), 1);
    assert_eq!(searcher.revealed[0].name, "Elvish Clancaller");

    // Nobody else sees any of it — not the revealed card, and not any other card
    // of that library either.
    let opponent = personalized_view(&state, &db, PlayerId(1));
    assert!(opponent.revealed.is_empty());
    let spectator = serde_json::to_string(&spectator_view(&state, &db)).unwrap();
    let opponent_json = serde_json::to_string(&opponent).unwrap();
    for card in &library {
        let id = card_entity_id(card.id);
        assert!(
            !opponent_json.contains(&id),
            "a searched library card leaked into another seat's view",
        );
        assert!(
            !spectator.contains(&id),
            "a searched library card leaked to a spectator",
        );
    }
}

#[test]
fn issue_604_an_under_fillable_choice_states_its_lower_bound() {
    // A scry may legally bottom nothing, so the prompt carries `min` as well as the
    // maximum — the fact a client needs in order not to block a legal answer.
    let db = CardDatabase::bundled().unwrap();
    let mut state = main_phase();
    state.players[0].library = ["forest", "island", "swamp"]
        .iter()
        .map(|slug| state.new_instance(fixture(slug)))
        .collect();
    let state = cast_and_resolve(&state, &db, "omenspeaker", Vec::new());
    // The ETB trigger targets nothing, so a single pass round resolves it.
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);

    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("the scry is owed");
    let Prompt::SelectFromZone {
        count,
        min,
        zone,
        candidates,
        ..
    } = &action.prompts[0]
    else {
        panic!("the choice is a select_from_zone");
    };
    assert_eq!((*count, *min), (2, Some(0)), "any number of the top two");
    assert_eq!(zone, "library");
    assert_eq!(candidates.len(), 2);

    // The looked-at cards are shown to the scrying seat, and to that seat only.
    assert_eq!(view.revealed.len(), 2);
    assert!(personalized_view(&state, &db, PlayerId(1))
        .revealed
        .is_empty());

    // Bottoming nothing is a legal, resolvable answer.
    let decline = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: Vec::new(),
        ..Default::default()
    };
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &decline),
        Some(Action::AnswerChoice { chosen: Vec::new() }),
    );
}

/// A yes-or-no's `option` prompt, or a panic naming what was found instead.
fn option_prompt(action: &ValidAction) -> (&str, Vec<&str>) {
    let Prompt::Option {
        prompt, options, ..
    } = &action.prompts[0]
    else {
        panic!("the yes-or-no is an option prompt");
    };
    (prompt, options.iter().map(|o| o.id.as_str()).collect())
}

#[test]
fn issue_610_an_optional_cost_projects_as_an_option_whose_accept_follows_the_pool() {
    // No bundled card poses this yet, so the card is written here. What is under
    // test is the projection: the question reuses the `option` prompt the mulligan
    // already rides on, its accepting choice appears exactly when the engine would
    // accept it, and the mana ability that makes it payable is offered alongside.
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_offer","name":"Test Offer",
             "types":["sorcery"],"mana_cost":"",
             "spell_effects":[{"kind":"may","cost":"{G}",
                               "effects":[{"kind":"draw_card","count":1}]}]},
            {"schema_version":1,"functional_id":"test_wood","name":"Test Wood",
             "types":["land"],"mana_cost":"",
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                           "effects":[{"kind":"add_mana","color":"green","amount":1}]}]}
        ]"#,
    )
    .unwrap();
    let card = |slug: &str| {
        db.card_id(&sage_engine::FunctionalId::try_from(slug.to_string()).unwrap())
            .unwrap()
    };

    // An untapped land and an empty pool: the cost is payable in principle, which is
    // why the question is posed at all, and not payable yet.
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let land = PermanentId(state.mint_id());
    let instance = state.new_instance(card("test_wood"));
    state.battlefield.push(sage_engine::Permanent {
        id: land,
        instance: instance.id,
        printed: card("test_wood").into(),
        controller: PlayerId(0),
        ..Default::default()
    });
    state.players[0].library = vec![state.new_instance(card("test_offer"))];
    let spell = state.new_instance(card("test_offer"));
    state.players[0].hand.push(spell);
    let state = sage_engine::apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    assert!(pending_player_choice(&state).is_some(), "the offer is owed");

    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("the caster is asked");
    assert_eq!(action.label, "Pay {G} to draw a card?");
    assert_eq!(
        option_prompt(action),
        ("Pay {G} to draw a card?", vec![DECLINE_OPTION]),
        "with nothing floating there is only one answer to give",
    );
    assert!(
        view.revealed.is_empty(),
        "a yes-or-no shows nobody any cards",
    );
    assert!(
        view.valid_actions
            .iter()
            .any(|a| a.kind == "activate_ability" && a.mana_ability),
        "the mana ability that would pay for it is offered alongside (CR 605.3a)",
    );
    assert!(
        personalized_view(&state, &db, PlayerId(1))
            .valid_actions
            .is_empty(),
        "and no other seat may act",
    );

    // Declining round-trips; an acceptance the offer did not list is refused rather
    // than quietly read as one.
    let answer = |action: &ValidAction, chosen: &str| ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: vec![chosen.to_string()],
        }],
        ..Default::default()
    };
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &answer(action, DECLINE_OPTION)),
        Some(Action::AnswerConfirm { accept: false }),
    );
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &answer(action, ACCEPT_OPTION)),
        None,
        "an unpayable acceptance is not on offer, so it is not bound",
    );

    // Float the mana, and the acceptance appears — same question, same slot.
    let floated = sage_engine::apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: land,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let view = personalized_view(&floated, &db, PlayerId(0));
    let action = choice_action(&view).expect("still asked");
    assert_eq!(
        option_prompt(action).1,
        vec![ACCEPT_OPTION, DECLINE_OPTION],
        "with the mana in the pool, paying is one of the answers",
    );
    assert_eq!(
        resolve_action(&floated, &db, PlayerId(0), &answer(action, ACCEPT_OPTION)),
        Some(Action::AnswerConfirm { accept: true }),
    );
}

#[test]
fn issue_738_the_colour_a_permanent_enters_with_is_asked_by_name() {
    // Diamond Mare's colour is named as it enters (CR 614.12), so the question is
    // owed while the permanent is still on the battlefield's doorstep. Five answers,
    // always — and the sentence names the card, because five identical buttons are
    // all a player has to go on when two things are entering at once.
    let db = CardDatabase::bundled().unwrap();
    let state = cast_and_resolve(&main_phase(), &db, "diamond_mare", Vec::new());

    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("its controller is asked");
    let (prompt, options) = option_prompt(action);
    assert_eq!(
        prompt,
        "Choose a color as Diamond Mare enters the battlefield"
    );
    assert_eq!(options, vec!["white", "blue", "black", "red", "green"]);
    assert_eq!(
        action.label, prompt,
        "the dock button says what it is about to ask"
    );
    let answer = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: vec!["red".to_string()],
        }],
        ..Default::default()
    };
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &answer),
        Some(Action::AnswerColor { color: Color::Red }),
    );

    // The opponent is offered nothing: the game is frozen on someone else's answer.
    assert!(choice_action(&personalized_view(&state, &db, PlayerId(1))).is_none());
}

#[test]
fn issue_731_the_replacement_ordering_projects_as_an_option_whose_id_is_the_position() {
    // CR 616.1 reaches the client as the `option` prompt it already knows: one entry
    // per applicable replacement, labelled with what it would do, and answered with
    // the position the engine indexes into. No new wire shape, and no seat but the
    // affected permanent's controller is asked.
    let db = CardDatabase::from_json(
        r#"[
        {"schema_version":1,"functional_id":"test_ward","name":"Test Ward",
         "types":["instant"],"mana_cost":"{U}","colors":["blue"],
         "spell_effects":[{"kind":"create_replacement","replacement":{
           "kind":"exile_entering",
           "entering":{"card_type":"creature","nontoken":true,"not_cast":true}}}]},
        {"schema_version":1,"functional_id":"test_wraith","name":"Test Wraith",
         "types":["creature"],"subtypes":["Spirit"],"mana_cost":"{1}{B}","colors":["black"],
         "power":1,"toughness":1,
         "abilities":[
           {"type":"enters_tapped"},
           {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
            "effects":[{"kind":"return_self_from_graveyard","destination":"battlefield"}]}]}
    ]"#,
    )
    .unwrap();
    let ward = db
        .card_id(&sage_engine::FunctionalId::try_from("test_ward".to_string()).unwrap())
        .unwrap();
    let wraith = db
        .card_id(&sage_engine::FunctionalId::try_from("test_wraith".to_string()).unwrap())
        .unwrap();

    // Seat 1 arms the replacement; seat 0 brings a wraith back, which is both a
    // creature entering without being cast and a creature that enters tapped.
    let mut state = main_phase();
    state.priority = PlayerId(1);
    let ward_card = state.new_instance(ward);
    state.players[1].hand.push(ward_card);
    let state = sage_engine::apply_action(
        &state,
        &Action::CastSpell {
            card: ward_card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    let mut state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(state.replacements.len(), 1);

    let wraith_card = state.new_instance(wraith);
    state.players[0].graveyard.push(wraith_card);
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let state = sage_engine::apply_action(
        &state,
        &Action::ActivateAbilityFromGraveyard {
            card: wraith_card,
            index: 1,
            targets: Vec::new(),
        },
        &db,
    );
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);

    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("the affected controller is asked");
    let (prompt, options) = option_prompt(action);
    assert_eq!(prompt, "Choose which replacement effect applies first");
    assert_eq!(options, vec!["0", "1"], "the option id is the position");
    let Prompt::Option { options, .. } = &action.prompts[0] else {
        panic!("an option prompt");
    };
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, vec!["It enters tapped", "Exile it instead"]);

    let answer = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: vec!["1".to_string()],
        }],
        ..Default::default()
    };
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &answer),
        Some(Action::AnswerReplacement { index: 1 }),
    );
    // The seat whose ability created the replacement is not the one asked.
    assert!(choice_action(&personalized_view(&state, &db, PlayerId(1))).is_none());
}

#[test]
fn issue_738_the_card_a_permanent_names_is_offered_as_the_catalogs_own_cards() {
    // Alpine Moon names a card as it enters (CR 614.12), so the question is owed while
    // the permanent is still on the battlefield's doorstep. The answers are the cards the
    // *engine* derived from the catalog — the client composes no list and sends no name —
    // and each option id is a card's authored identity, never a per-build handle.
    let db = CardDatabase::bundled().unwrap();
    let state = cast_and_resolve(&main_phase(), &db, "alpine_moon", Vec::new());

    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("its controller is asked");
    let (prompt, options) = option_prompt(action);
    assert_eq!(
        prompt,
        "Choose a card name as Alpine Moon enters the battlefield"
    );
    assert!(options.contains(&"highland_lake"), "{options:?}");
    assert!(
        !options.contains(&"plains"),
        "a basic land is not a nonbasic one"
    );
    assert_eq!(
        action.label, prompt,
        "the dock button says what it is about to ask"
    );

    // The label a player reads is the card's name; the id they send back is its identity.
    let Prompt::Option { options, .. } = &action.prompts[0] else {
        panic!("the naming question rides the option prompt");
    };
    let lake = options
        .iter()
        .find(|option| option.id == "highland_lake")
        .expect("offered");
    assert_eq!(lake.label, "Highland Lake");

    let answer = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: vec!["highland_lake".to_string()],
        }],
        ..Default::default()
    };
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &answer),
        Some(Action::AnswerCardName {
            card: fixture("highland_lake")
        }),
    );

    // An id the offer did not list is refused rather than guessed at — including one
    // naming a real card outside the class the ability declared.
    for forged in ["plains", "not_a_card", ""] {
        let forged = ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: vec![forged.to_string()],
            }],
            ..Default::default()
        };
        assert_eq!(resolve_action(&state, &db, PlayerId(0), &forged), None);
    }

    // The opponent is offered nothing: the game is frozen on someone else's answer.
    assert!(choice_action(&personalized_view(&state, &db, PlayerId(1))).is_none());
}

#[test]
fn issue_746_the_library_ordering_projects_as_an_order_prompt_over_the_remainder() {
    // Anticipate asks twice, on one action kind and one slot: a `select_from_zone`
    // for the card taken, then an `order` for what is left. The second is the prompt
    // the combat-damage assignment already rides on — no new wire shape — and the
    // cards behind its item ids reach the chooser on `revealed` and nobody else.
    let db = CardDatabase::bundled().unwrap();
    let mut state = main_phase();
    state.players[0].library = ["mountain", "swamp", "island", "forest"]
        .iter()
        .map(|slug| state.new_instance(fixture(slug)))
        .collect();
    let state = cast_and_resolve(&state, &db, "anticipate", Vec::new());

    // Answer the look through the wire, taking the second card it showed.
    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("the look is owed");
    let Prompt::SelectFromZone { candidates, .. } = &action.prompts[0] else {
        panic!("the take is a select_from_zone");
    };
    let taken = resolve_action(
        &state,
        &db,
        PlayerId(0),
        &ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: vec![candidates[1].clone()],
            }],
            ..Default::default()
        },
    )
    .expect("the take binds");
    let state = sage_engine::apply_action(&state, &taken, &db);

    // Same action kind, same slot, a different prompt shape.
    let view = personalized_view(&state, &db, PlayerId(0));
    let action = choice_action(&view).expect("the arrangement is owed");
    let Prompt::Order {
        slot,
        prompt,
        items,
    } = &action.prompts[0]
    else {
        panic!("the arrangement is an order prompt");
    };
    assert_eq!(slot, CHOICE_SLOT);
    assert_eq!(
        prompt, "Choose the order these go on the bottom of your library, deepest first",
        "the sentence says which end is which",
    );
    assert_eq!(items.len(), 2, "the two cards the look did not take");
    assert_eq!(action.label, *prompt, "the dock button asks the question");

    let revealed: Vec<&str> = view.revealed.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(revealed, vec!["Forest", "Swamp"]);
    assert!(
        personalized_view(&state, &db, PlayerId(1))
            .revealed
            .is_empty(),
        "the top of a library reaches its owner and no other seat",
    );

    // The answer round-trips **in the order sent** — re-sorting it would bottom the
    // cards somewhere the player did not choose.
    let picked = vec![items[1].clone(), items[0].clone()];
    let answer = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: picked.clone(),
        }],
        ..Default::default()
    };
    let Some(Action::AnswerOrder { order }) = resolve_action(&state, &db, PlayerId(0), &answer)
    else {
        panic!("the answer resolves to an AnswerOrder");
    };
    assert_eq!(
        order
            .iter()
            .map(|id| card_entity_id(*id))
            .collect::<Vec<_>>(),
        picked,
    );

    // An id the offer did not name is rejected rather than dropped.
    let forged = ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: vec![TargetChoice {
            slot: CHOICE_SLOT.to_string(),
            chosen: vec!["card_999999".to_string()],
        }],
        ..Default::default()
    };
    assert!(resolve_action(&state, &db, PlayerId(0), &forged).is_none());
}
