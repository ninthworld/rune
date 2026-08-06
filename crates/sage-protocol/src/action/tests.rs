//! Round-trip and compatibility tests for the interactivity contract.
//!
//! Split out of `action.rs` for size (issue #711). Pure code motion — every test is
//! unchanged and moves with the code it exercises.

#![allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests

use crate::*;

#[test]
fn valid_action_serializes_type_and_omits_empty_subject() {
    let pass = ValidAction {
        mana_ability: false,
        id: "a1".into(),
        kind: "pass_priority".into(),
        label: "Pass".into(),
        subject: vec![],
        requirements: vec![],
        prompts: vec![],
        cost: None,
        destinations: vec![],
        token: String::new(),
    };
    let json = serde_json::to_value(&pass).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "id": "a1", "type": "pass_priority", "label": "Pass" })
    );
}

#[test]
fn cr_605_mana_ability_flag_round_trips_and_defaults_off() {
    // `mana_ability` rides the wire only when true; a legacy
    // payload without the key deserializes to `false`.
    let tap = ValidAction {
        mana_ability: true,
        id: "a2".into(),
        kind: "activate_ability".into(),
        label: "{T}: Add {G}.".into(),
        subject: vec!["perm_1".into()],
        requirements: vec![],
        prompts: vec![],
        cost: None,
        destinations: vec![],
        token: "h:1".into(),
    };
    let json = serde_json::to_value(&tap).unwrap();
    assert_eq!(json.get("mana_ability"), Some(&serde_json::json!(true)));
    let back: ValidAction = serde_json::from_value(json).unwrap();
    assert_eq!(back, tap);

    let legacy: ValidAction = serde_json::from_value(serde_json::json!({
        "id": "a1", "type": "activate_ability", "label": "x"
    }))
    .unwrap();
    assert!(!legacy.mana_ability);
}

#[test]
fn valid_action_carries_requirements_and_token() {
    // A targeted spell: subject is the hand card, requirements advertise the
    // one target slot's legal candidates, and a content-binding token is
    // present for the client to echo back.
    let bolt = ValidAction {
        mana_ability: false,
        id: "a3".into(),
        kind: "cast_spell".into(),
        label: "Cast Lightning Bolt".into(),
        subject: vec!["c3".into()],
        requirements: vec![TargetRequirement {
            slot: "t0".into(),
            prompt: "target creature or player".into(),
            // A bolt's one slot is mandatory, so the flag elides from the wire —
            // the assertion below is the proof that an older client sees no change.
            optional: false,
            candidates: vec!["perm_bear".into(), "p1".into(), "p2".into()],
            // Choosing a target for a bolt taps nothing, so the field elides too.
            taps: vec![],
            subject: None,
        }],
        prompts: vec![],
        // A cast states what it costs, printed and as the game has it (CR 601.2f). The
        // two agree here — nothing is modifying this bolt's cost — and the field still
        // rides the wire, because "unchanged" is an answer a client draws rather than
        // one it infers from an absence.
        cost: Some(ActionCost {
            printed: "{R}".into(),
            modified: "{R}".into(),
        }),
        destinations: vec![],
        token: "h:9f2c".into(),
    };
    let json = serde_json::to_value(&bolt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "id": "a3",
            "type": "cast_spell",
            "label": "Cast Lightning Bolt",
            "subject": ["c3"],
            "requirements": [{
                "slot": "t0",
                "prompt": "target creature or player",
                "candidates": ["perm_bear", "p1", "p2"]
            }],
            "cost": {"printed": "{R}", "modified": "{R}"},
            "token": "h:9f2c"
        })
    );
    let back: ValidAction = serde_json::from_value(json).unwrap();
    assert_eq!(back, bolt);
}

#[test]
fn a_declaration_states_which_of_its_candidates_choosing_taps() {
    // The attackers slot: choosing a creature taps it (CR 508.1f) unless it is
    // vigilant (CR 702.20b), and which is which is a keyword judgment a client must
    // not make. So the server names the subset, and a client turns those cards as
    // they go into the slot and back as they come out — nothing has been sent yet.
    let declare = TargetRequirement {
        slot: "attackers".into(),
        prompt: "Choose which creatures attack".into(),
        optional: true,
        candidates: vec!["perm_bear".into(), "perm_angel".into()],
        taps: vec!["perm_bear".into()],
        subject: None,
    };
    let json = serde_json::to_value(&declare).unwrap();
    assert_eq!(json["taps"], serde_json::json!(["perm_bear"]));
    assert_eq!(
        serde_json::from_value::<TargetRequirement>(json).unwrap(),
        declare
    );

    // A slot whose answer taps nothing says nothing: the key is absent, and a
    // payload from a server that predates the field reads the same way.
    let legacy: TargetRequirement = serde_json::from_str(
        r#"{"slot":"t0","prompt":"target creature","candidates":["perm_bear"]}"#,
    )
    .unwrap();
    assert!(legacy.taps.is_empty());
}

#[test]
fn option_prompt_round_trips_and_tags_its_kind() {
    // `option` (mulligan keep/take-another): a slot listing named choices, tagged
    // `kind: "option"` on the wire, answered with the chosen option id. A choice
    // that owes another slot names it in `requires` (issue #451); one that owes
    // nothing omits the field entirely.
    let prompt = Prompt::Option {
        slot: "decision".into(),
        prompt: "Keep this hand or take a mulligan?".into(),
        options: vec![
            PromptOption {
                id: "keep".into(),
                label: "Keep this hand".into(),
                requires: vec!["bottom".into()],
            },
            PromptOption {
                id: "mulligan".into(),
                label: "Mulligan".into(),
                requires: vec![],
            },
        ],
    };
    let json = serde_json::to_value(&prompt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "option",
            "slot": "decision",
            "prompt": "Keep this hand or take a mulligan?",
            "options": [
                { "id": "keep", "label": "Keep this hand", "requires": ["bottom"] },
                { "id": "mulligan", "label": "Mulligan" }
            ]
        })
    );
    let back: Prompt = serde_json::from_value(json).unwrap();
    assert_eq!(back, prompt);
}

#[test]
fn legacy_option_without_requires_deserializes_as_self_contained() {
    // A payload from a server that predates the coupling field omits it; the
    // choice must default to "owes no other slot" rather than failing to decode.
    let json = r#"{ "id": "keep", "label": "Keep this hand" }"#;
    let option: PromptOption = serde_json::from_str(json).unwrap();
    assert!(option.requires.is_empty());
}

#[test]
fn select_from_zone_prompt_round_trips() {
    // `select_from_zone` (cleanup discard / mulligan bottoming): carries the zone,
    // its owner, how many to pick, and the candidate entity ids.
    let prompt = Prompt::SelectFromZone {
        slot: "discard".into(),
        prompt: "Choose a card to discard".into(),
        zone: "hand".into(),
        owner: "p0".into(),
        count: 1,
        min: None,
        candidates: vec!["card_1".into(), "card_2".into(), "card_3".into()],
    };
    let json = serde_json::to_value(&prompt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "select_from_zone",
            "slot": "discard",
            "prompt": "Choose a card to discard",
            "zone": "hand",
            "owner": "p0",
            "count": 1,
            "candidates": ["card_1", "card_2", "card_3"]
        })
    );
    let back: Prompt = serde_json::from_value(json).unwrap();
    assert_eq!(back, prompt);
}

#[test]
fn issue_604_a_select_from_zone_states_a_lower_bound_only_when_it_has_one() {
    // A choice a player may legally under-fill (scry any number, take up to one,
    // fail to find) carries `min` alongside the maximum...
    let scry = Prompt::SelectFromZone {
        slot: "choice".into(),
        prompt: "Choose up to 2 cards to put on the bottom of your library".into(),
        zone: "library".into(),
        owner: "p0".into(),
        count: 2,
        min: Some(0),
        candidates: vec!["card_1".into(), "card_2".into()],
    };
    let json = serde_json::to_value(&scry).unwrap();
    assert_eq!(json["min"], serde_json::json!(0));
    assert_eq!(json["count"], serde_json::json!(2));
    assert_eq!(serde_json::from_value::<Prompt>(json).unwrap(), scry);

    // ...and an exact one elides it, so an existing bottoming or cleanup discard
    // serializes byte-for-byte as it always did.
    let exact = Prompt::SelectFromZone {
        slot: "discard".into(),
        prompt: "Choose a card to discard".into(),
        zone: "hand".into(),
        owner: "p0".into(),
        count: 1,
        min: None,
        candidates: vec!["card_1".into()],
    };
    assert!(serde_json::to_value(&exact).unwrap().get("min").is_none());

    // A payload from a server that predates the field reads as exact, not as
    // "at least zero" — the safe direction, since it is the shape that was meant.
    let legacy: Prompt = serde_json::from_str(
        r#"{"kind":"select_from_zone","slot":"discard","prompt":"x","zone":"hand","owner":"p0","count":1}"#,
    )
    .unwrap();
    let Prompt::SelectFromZone { min, count, .. } = legacy else {
        panic!("a select_from_zone");
    };
    assert_eq!((min, count), (None, 1));
}

#[test]
fn order_prompt_round_trips() {
    // `order` (ordering simultaneous triggers / scry): the items to arrange, whose
    // answer is a permutation of exactly these ids.
    let prompt = Prompt::Order {
        slot: "triggers".into(),
        prompt: "Order these triggered abilities".into(),
        items: vec!["stack_1".into(), "stack_2".into()],
    };
    let json = serde_json::to_value(&prompt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "order",
            "slot": "triggers",
            "prompt": "Order these triggered abilities",
            "items": ["stack_1", "stack_2"]
        })
    );
    let back: Prompt = serde_json::from_value(json).unwrap();
    assert_eq!(back, prompt);
}

#[test]
fn pay_mana_prompt_round_trips_and_names_one_pip() {
    // One slot pays one pip. A dual land appears twice — once per ability it could
    // pay *this* pip with — which is the whole signal a client needs to know it must
    // ask which color, without knowing what a color is.
    let prompt = Prompt::PayMana {
        slot: "m0".into(),
        prompt: "Pay {W}".into(),
        pip: "{W}".into(),
        candidates: vec![
            ManaOption {
                id: "perm_7#1".into(),
                source: "perm_7".into(),
                label: "{W}".into(),
                taps: true,
            },
            ManaOption {
                id: "perm_9#1".into(),
                source: "perm_9".into(),
                label: "{W}".into(),
                taps: true,
            },
        ],
    };
    let json = serde_json::to_value(&prompt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "pay_mana",
            "slot": "m0",
            "prompt": "Pay {W}",
            "pip": "{W}",
            "candidates": [
                { "id": "perm_7#1", "source": "perm_7", "label": "{W}", "taps": true },
                { "id": "perm_9#1", "source": "perm_9", "label": "{W}", "taps": true }
            ]
        })
    );
    assert_eq!(serde_json::from_value::<Prompt>(json).unwrap(), prompt);

    // A pip nothing can pay serializes without the key at all, so a client that sees
    // no candidates offers no way to fill it rather than guessing.
    let empty = serde_json::to_value(Prompt::PayMana {
        slot: "m1".into(),
        prompt: "Pay {1}".into(),
        pip: "{1}".into(),
        candidates: Vec::new(),
    })
    .unwrap();
    assert!(empty.get("candidates").is_none());
}

#[test]
fn a_mana_option_separates_what_is_clicked_from_what_is_sent() {
    // The `source` is the board entity a player clicks; the `id` names the activation
    // and is what comes back. Two options over one permanent is how "which color?"
    // becomes visible to a client that knows no rules.
    let white = ManaOption {
        id: "perm_4#1".into(),
        source: "perm_4".into(),
        label: "{W}".into(),
        taps: true,
    };
    let blue = ManaOption {
        id: "perm_4#2".into(),
        source: "perm_4".into(),
        label: "{U}".into(),
        taps: true,
    };
    assert_eq!(white.source, blue.source, "one permanent");
    assert_ne!(white.id, blue.id, "two activations");

    // An unlabelled option elides the key — the case where a permanent offers exactly
    // one way to pay a pip and there is nothing to disambiguate.
    let plain = serde_json::to_value(ManaOption {
        id: "perm_2#0".into(),
        source: "perm_2".into(),
        label: String::new(),
        taps: false,
    })
    .unwrap();
    assert_eq!(
        plain,
        serde_json::json!({ "id": "perm_2#0", "source": "perm_2" })
    );
}

#[test]
fn a_mana_option_states_whether_spending_it_taps_its_source() {
    // What a payment *does to the board* is a rules fact, and a client draws the
    // board: a land turns sideways as it is spent and a source that pays some other
    // way does not. Additive in both directions — omitted when it taps nothing, and a
    // payload from a server that predates the field reads as "taps nothing", which is
    // the shape that existed before it.
    let tapper = ManaOption {
        id: "perm_2#0".into(),
        source: "perm_2".into(),
        label: "{G}".into(),
        taps: true,
    };
    let json = serde_json::to_value(&tapper).unwrap();
    assert_eq!(json["taps"], serde_json::json!(true));
    assert_eq!(serde_json::from_value::<ManaOption>(json).unwrap(), tapper);

    let legacy: ManaOption =
        serde_json::from_str(r#"{"id":"perm_2#0","source":"perm_2"}"#).unwrap();
    assert!(!legacy.taps);
}

#[test]
fn issue_554_number_prompt_round_trips_and_tags_its_kind() {
    // `number` (X, a divided value): a slot carrying the server's inclusive
    // bounds, answered with the chosen value as a decimal string in the same
    // `TargetChoice` shape every other slot kind uses.
    let prompt = Prompt::Number {
        slot: "x".into(),
        prompt: "Choose a value for X".into(),
        min: 0,
        max: 4,
        values: Vec::new(),
    };
    let json = serde_json::to_value(&prompt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "kind": "number",
            "slot": "x",
            "prompt": "Choose a value for X",
            "min": 0,
            "max": 4
        })
    );
    assert_eq!(serde_json::from_value::<Prompt>(json).unwrap(), prompt);

    // Both bounds always ride the wire — a zero `min` is not elided, so the range
    // reads completely rather than by inference.
    let one_only = serde_json::to_value(Prompt::Number {
        slot: "n".into(),
        prompt: "How many?".into(),
        min: 1,
        max: 1,
        values: Vec::new(),
    })
    .unwrap();
    assert_eq!(one_only["min"], 1);
    assert_eq!(one_only["max"], 1);

    // An X in a mana cost additionally states **what each value costs**, because a
    // client may not multiply a cost out for itself (issue #733). The list rides beside
    // the range and is omitted entirely for a number that costs nothing, so the shape
    // above is unchanged for every prompt that predates it.
    let announced = Prompt::Number {
        slot: "x".into(),
        prompt: "Choose a value for X".into(),
        min: 0,
        max: 2,
        values: vec![
            NumberValue {
                value: 0,
                cost: "{R}".into(),
            },
            NumberValue {
                value: 1,
                cost: "{1}{R}".into(),
            },
            NumberValue {
                value: 2,
                cost: "{2}{R}".into(),
            },
        ],
    };
    let json = serde_json::to_value(&announced).unwrap();
    assert_eq!(
        json["values"],
        serde_json::json!([
            { "value": 0, "cost": "{R}" },
            { "value": 1, "cost": "{1}{R}" },
            { "value": 2, "cost": "{2}{R}" }
        ])
    );
    assert_eq!(serde_json::from_value::<Prompt>(json).unwrap(), announced);

    // The answer is the numeral as a string, in the shared slot-answer shape.
    let answer = TargetChoice {
        slot: "x".into(),
        chosen: vec!["3".into()],
    };
    assert_eq!(
        serde_json::to_value(&answer).unwrap(),
        serde_json::json!({ "slot": "x", "chosen": ["3"] })
    );
}

#[test]
fn issue_554_destinations_ride_the_action_and_elide_when_it_names_none() {
    // A cast names the stack; the client derives its drop region from this alone.
    let cast = ValidAction {
        id: "a3".into(),
        kind: "cast_spell".into(),
        label: "Cast Lightning Bolt".into(),
        subject: vec!["c3".into()],
        destinations: vec![ActionDestination {
            kind: "zone".into(),
            id: "stack".into(),
            owner: String::new(),
            label: "Stack".into(),
        }],
        ..Default::default()
    };
    let json = serde_json::to_value(&cast).unwrap();
    assert_eq!(
        json["destinations"],
        serde_json::json!([{ "type": "zone", "id": "stack", "label": "Stack" }])
    );
    assert_eq!(serde_json::from_value::<ValidAction>(json).unwrap(), cast);

    // An action with nowhere to go elides the field entirely, so a client has no
    // drop target at all for it — the fail-closed default.
    let pass = ValidAction {
        id: "a1".into(),
        kind: "pass_priority".into(),
        label: "Pass".into(),
        ..Default::default()
    };
    assert!(serde_json::to_value(&pass)
        .unwrap()
        .get("destinations")
        .is_none());

    // A payload from a server that predates the field decodes to no destinations,
    // which reads the same way: no drop target.
    let legacy: ValidAction =
        serde_json::from_str(r#"{ "id": "a1", "type": "pass_priority", "label": "Pass" }"#)
            .unwrap();
    assert!(legacy.destinations.is_empty());
}

#[test]
fn issue_554_submission_correlates_an_answer_with_its_acknowledgement() {
    // The client puts an opaque id on the message...
    let msg = ChooseAction {
        action_id: "a2".into(),
        submission: "s:17".into(),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&msg).unwrap(),
        serde_json::json!({ "action_id": "a2", "submission": "s:17" })
    );

    // ...and the server echoes it back verbatim, with its verdict.
    let ack = ActionAck {
        submission: "s:17".into(),
        accepted: true,
    };
    assert_eq!(ack.submission, msg.submission);

    // A client that does not correlate sends exactly the message it always sent,
    // and an older client's message still decodes (to no correlation id).
    let plain = ChooseAction {
        action_id: "a2".into(),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        serde_json::json!({ "action_id": "a2" })
    );
    let legacy: ChooseAction = serde_json::from_str(r#"{"action_id":"a2"}"#).unwrap();
    assert!(legacy.submission.is_empty());
}

#[test]
fn valid_action_carries_prompts_and_is_answered_by_target_choice() {
    // A prompt-bearing action rides on `valid_actions` exactly like a targeted
    // one: it carries its prompt slots and a content-binding token, and the client
    // answers each slot with a `TargetChoice` keyed by `slot`.
    let action = ValidAction {
        mana_ability: false,
        id: "a0".into(),
        kind: "mulligan_decision".into(),
        label: "Mulligan decision".into(),
        subject: vec![],
        requirements: vec![],
        prompts: vec![
            Prompt::Option {
                slot: "decision".into(),
                prompt: "Keep or mulligan?".into(),
                options: vec![
                    PromptOption {
                        id: "keep".into(),
                        label: "Keep".into(),
                        requires: vec!["bottom".into()],
                    },
                    PromptOption {
                        id: "mulligan".into(),
                        label: "Mulligan".into(),
                        requires: vec![],
                    },
                ],
            },
            Prompt::SelectFromZone {
                slot: "bottom".into(),
                prompt: "Bottom 1 card".into(),
                zone: "hand".into(),
                owner: "p0".into(),
                count: 1,
                min: None,
                candidates: vec!["card_1".into(), "card_2".into()],
            },
        ],
        cost: None,
        destinations: vec![],
        token: "t0123456789abcdef".into(),
    };
    let json = serde_json::to_value(&action).unwrap();
    // `prompts` sits alongside `requirements` in the same wire object.
    assert!(json.get("prompts").is_some());
    assert_eq!(json["prompts"][0]["kind"], serde_json::json!("option"));
    assert_eq!(
        json["prompts"][1]["kind"],
        serde_json::json!("select_from_zone")
    );
    let back: ValidAction = serde_json::from_value(json).unwrap();
    assert_eq!(back, action);

    // The answer keys each slot with a `TargetChoice` (option id + selected ids).
    let answer = ChooseAction {
        submission: String::new(),
        action_id: "a0".into(),
        token: "t0123456789abcdef".into(),
        targets: vec![
            TargetChoice {
                slot: "decision".into(),
                chosen: vec!["keep".into()],
            },
            TargetChoice {
                slot: "bottom".into(),
                chosen: vec!["card_1".into()],
            },
        ],
    };
    let back: ChooseAction =
        serde_json::from_value(serde_json::to_value(&answer).unwrap()).unwrap();
    assert_eq!(back, answer);
}

#[test]
fn valid_action_without_prompts_omits_the_field() {
    // Backward-compat wire shape: an action with no prompts elides the field, so
    // existing (targeting/plain) actions serialize exactly as before.
    let pass = ValidAction {
        id: "a1".into(),
        kind: "pass_priority".into(),
        label: "Pass".into(),
        ..Default::default()
    };
    let json = serde_json::to_value(&pass).unwrap();
    assert!(json.get("prompts").is_none());
}

#[test]
fn legacy_valid_action_without_token_or_requirements_deserializes() {
    // A payload from a server that predates this shape omits both new fields;
    // they must default (empty requirements, empty token) rather than fail.
    let json = r#"{ "id": "a1", "type": "pass_priority", "label": "Pass" }"#;
    let action: ValidAction = serde_json::from_str(json).unwrap();
    assert!(action.requirements.is_empty());
    assert_eq!(action.token, "");
}

#[test]
fn issue_604_choice_contract_fixture_round_trips_and_matches_typed_fields() {
    // Cross-language contract fixture: a mid-resolution scry. Its `player_choice`
    // action carries one `select_from_zone` whose bounds are a *range*, the cards
    // it asks about ride the receiver-only `revealed` channel, and the log carries
    // the two count-only events this work adds. The web client's `protocol.test.ts`
    // consumes these exact bytes.
    let json = include_str!("../../fixtures/gameview-choice.json");
    let view: GameView = serde_json::from_str(json).unwrap();
    let reencoded = serde_json::to_string(&view).unwrap();
    assert_eq!(serde_json::from_str::<GameView>(&reencoded).unwrap(), view);

    let choice = &view.valid_actions[0];
    assert_eq!(choice.kind, "player_choice");
    assert!(!choice.token.is_empty(), "a prompt action is token-bound");
    let Prompt::SelectFromZone {
        slot,
        zone,
        owner,
        count,
        min,
        candidates,
        ..
    } = &choice.prompts[0]
    else {
        panic!("the choice is a select_from_zone");
    };
    assert_eq!(slot, "choice");
    assert_eq!(zone, "library");
    assert_eq!(owner, "p0");
    assert_eq!((*count, *min), (2, Some(0)), "any number of the two");
    assert_eq!(candidates, &["card_20".to_string(), "card_21".to_string()]);

    // The cards the choice is about are shown to this receiver alone, by ids the
    // prompt's candidates name — so a client can render what it is being asked.
    assert_eq!(
        view.revealed
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        candidates.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    // Both new log events carry counts and seats, never card identities.
    assert!(matches!(
        view.log[0].event,
        GameLogEvent::CardsDiscarded { count: 2, .. }
    ));
    assert!(matches!(
        view.log[1].event,
        GameLogEvent::LibrarySearched { .. }
    ));
}

#[test]
fn issue_610_optional_effect_contract_fixture_round_trips_and_matches_typed_fields() {
    // Cross-language contract fixture: the yes-or-no of an optional effect. It adds
    // no wire shape — the question rides the `option` prompt the mulligan decision
    // already uses — so what this pins is the *composition*: a `player_choice`
    // carrying an option slot, the mana ability offered beside it (CR 605.3a), and
    // the two seat-only log events. The web client's `protocol.test.ts` consumes
    // these exact bytes.
    let json = include_str!("../../fixtures/gameview-optional.json");
    let view: GameView = serde_json::from_str(json).unwrap();
    let reencoded = serde_json::to_string(&view).unwrap();
    assert_eq!(serde_json::from_str::<GameView>(&reencoded).unwrap(), view);

    let choice = &view.valid_actions[0];
    assert_eq!(choice.kind, "player_choice");
    assert!(!choice.token.is_empty(), "a prompt action is token-bound");
    let Prompt::Option {
        slot,
        prompt,
        options,
    } = &choice.prompts[0]
    else {
        panic!("the yes-or-no is an option prompt");
    };
    assert_eq!(slot, "choice");
    assert_eq!(prompt, "Pay {1} to draw a card?");
    assert_eq!(
        options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["accept", "decline"],
    );
    // Neither choice owes another slot: a yes-or-no is self-contained.
    assert!(options.iter().all(|option| option.requires.is_empty()));

    // The seat is asked to pay, so the mana it could pay with is on offer too.
    assert!(view.valid_actions[1].mana_ability);

    // Both new log events name a seat and nothing else — never what was offered,
    // and never the pool that could or could not afford it.
    assert!(matches!(
        view.log[0].event,
        GameLogEvent::OptionalApplied { .. }
    ));
    assert!(matches!(
        view.log[1].event,
        GameLogEvent::OptionalDeclined { .. }
    ));
    // A yes-or-no shows nobody any cards: there is no revealed channel on this view.
    assert!(view.revealed.is_empty());
}

#[test]
fn prompts_contract_fixture_round_trips_and_matches_typed_fields() {
    // Cross-language contract fixture (issue #56/#156): a pre-game mulligan frame
    // whose `mulligan_decision` action carries an `option` prompt (keep/mulligan)
    // and a `select_from_zone` bottoming prompt. The web client's `wire.test.ts`
    // consumes these exact bytes; a rename/retype here (or there) fails a test.
    let json = include_str!("../../fixtures/gameview-prompts.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    // Round-trips through serde JSON without loss.
    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    let decision = &view.valid_actions[0];
    assert_eq!(decision.kind, "mulligan_decision");
    assert!(!decision.token.is_empty(), "a prompt action is token-bound");
    assert_eq!(decision.prompts.len(), 2);

    // First slot: the `option` keep/mulligan decision.
    let Prompt::Option { slot, options, .. } = &decision.prompts[0] else {
        panic!("first prompt is an option");
    };
    assert_eq!(slot, "decision");
    assert_eq!(
        options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["keep", "mulligan"],
    );
    // Keeping owes the bottoming slot; taking another hand owes nothing (#451).
    assert_eq!(options[0].requires, ["bottom".to_string()]);
    assert!(options[1].requires.is_empty());

    // Second slot: the `select_from_zone` bottoming over the hand.
    let Prompt::SelectFromZone {
        slot,
        zone,
        owner,
        count,
        candidates,
        ..
    } = &decision.prompts[1]
    else {
        panic!("second prompt is a select_from_zone");
    };
    assert_eq!(slot, "bottom");
    assert_eq!(zone, "hand");
    assert_eq!(owner, "p0");
    assert_eq!(*count, 1);
    assert_eq!(candidates, &["card_10".to_string(), "card_11".to_string()]);
}
