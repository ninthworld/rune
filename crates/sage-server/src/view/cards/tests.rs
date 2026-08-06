//! Card, permanent, and zone projection, checked against the views a client reads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::test_support::{fixture, id_in};
use crate::view::test_support::put_permanent;

/// A keyword the permanent has and its printed card does not is stated separately
/// (CR 613.1f), because the card's own rules text is the *printed* card's: without
/// this, a creature an Aura or a pump gave trample to says nothing about trample
/// anywhere a player looks.
#[test]
fn a_granted_keyword_is_stated_apart_from_the_printed_ones() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    // Onakke Ogre is a vanilla 4/2; Prodigious Growth grants +7/+7 and trample.
    let host = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(0),
        false,
        false,
    );
    let aura = put_permanent(
        &mut state,
        fixture("prodigious_growth"),
        PlayerId(0),
        false,
        false,
    );
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == aura) {
        perm.attached_to = Some(host);
    }

    let enchanted = state
        .battlefield
        .iter()
        .find(|perm| perm.id == host)
        .expect("the host");
    assert_eq!(
        granted_keywords(&state, enchanted, &db),
        vec!["Trample".to_string()],
        "the word the card would print, so a granted keyword reads like a printed one"
    );

    // A permanent with nothing granted states nothing, so the wire is unchanged for
    // every ordinary board.
    let plain = put_permanent(
        &mut state,
        fixture("walking_corpse"),
        PlayerId(0),
        false,
        false,
    );
    let plain = state
        .battlefield
        .iter()
        .find(|perm| perm.id == plain)
        .expect("the creature");
    assert!(granted_keywords(&state, plain, &db).is_empty());
}

/// A **token** projects onto the wire as a complete, playable object with no card
/// identity behind it (issue #605): its characteristics come from the token itself,
/// its `functional_id` is empty because there is no card to name, and `token: true`
/// says so outright rather than leaving a client to infer it from that absence.
#[test]
fn issue_605_a_token_projects_with_characteristics_and_no_card_identity() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();

    let id = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id,
        instance: CardInstanceId(0),
        printed: sage_engine::Printed::Token(Box::new(sage_engine::TokenData {
            name: "Thopter".to_string(),
            types: vec![
                sage_engine::CardType::Artifact,
                sage_engine::CardType::Creature,
            ],
            subtypes: vec!["Thopter".to_string()],
            colors: Vec::new(),
            power: Some(1),
            toughness: Some(1),
            keywords: vec![Keyword::Flying],
            ..Default::default()
        })),
        controller: PlayerId(0),
        ..Default::default()
    });
    // A +1/+1 counter, to prove the projection is the *computed* face and not a
    // second, token-only read path.
    state.battlefield[0]
        .counters
        .insert(sage_engine::CounterKind::PlusOnePlusOne, 1);

    let view = personalized_view(&state, &db, PlayerId(0));
    let permanent = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(id))
        .expect("the token appears on the battlefield");
    let card = &permanent.card;

    assert_eq!(card.name, "Thopter");
    assert_eq!(card.type_line, "Artifact Creature — Thopter");
    assert_eq!(card.power.as_deref(), Some("2"), "1/1 plus a +1/+1 counter");
    assert_eq!(card.toughness.as_deref(), Some("2"));
    assert_eq!(card.keywords, vec!["flying".to_string()]);
    assert_eq!(card.rules_text, "Flying");
    assert!(card.token, "the client is told it is a token");
    assert!(
        card.functional_id.is_empty(),
        "a token has no card identity to cache or look presentation up by"
    );
    assert!(card.mana_cost.is_none(), "a token has no mana cost");
    // A token has types like anything else: having no card behind it says nothing
    // about what it is on the battlefield.
    assert_eq!(
        card.card_types,
        vec![CardType::Artifact, CardType::Creature]
    );
}

/// The type **line** and the type **set** are one projection, so a client never has
/// to parse the sentence to learn what a permanent is (issue #641).
///
/// Asserted together on purpose: the two fields come from the same `types`, and the
/// failure this guards against is one of them being sourced from somewhere else
/// later and quietly disagreeing with the other on the same card.
#[test]
fn issue_641_card_types_are_stated_beside_the_type_line_they_render() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    put_permanent(
        &mut state,
        id_in(&db, "llanowar_elves"),
        PlayerId(0),
        false,
        false,
    );
    put_permanent(&mut state, id_in(&db, "forest"), PlayerId(0), false, false);

    let view = personalized_view(&state, &db, PlayerId(0));
    let of = |name: &str| {
        view.battlefield
            .iter()
            .find(|p| p.card.name == name)
            .unwrap_or_else(|| panic!("{name} is on the battlefield"))
            .card
            .clone()
    };

    let elves = of("Llanowar Elves");
    assert_eq!(elves.card_types, vec![CardType::Creature]);
    assert!(elves.type_line.starts_with("Creature"));

    let forest = of("Forest");
    assert_eq!(forest.card_types, vec![CardType::Land]);
    assert!(forest.type_line.contains("Land"));
}

/// A battlefield permanent enchanted with an Aura projects its **current**
/// (computed) power/toughness on the wire, so the host's P/T reflects the Aura's
/// layer-7c grant (CR 303.4 / 613.7c, issue #152) rather than the printed value.
#[test]
fn issue_152_aura_boosted_host_projects_current_pt() {
    // P/T Auras have no clean M19 card, so this is exercised inline (ADR 0009):
    // a 1/1 host enchanted with a +2/+2 Aura.
    let json = r#"[
        {"schema_version":1,"functional_id":"test_scout","name":"Test Scout",
         "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1},
        {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
         "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
         "attachment":{"kind":"aura","attach_to":"any_creature","power":2,"toughness":2}}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = GameState::new_two_player();

    let host = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: host,
        instance: CardInstanceId(0),
        printed: id_in(&db, "test_scout").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });
    let aura = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: aura,
        instance: CardInstanceId(1),
        printed: id_in(&db, "test_aegis").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: Some(host),
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let host_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(host))
        .expect("the enchanted host must appear in the view");
    assert_eq!(
        host_view.card.power.as_deref(),
        Some("3"),
        "printed 1 + Aura's +2 projects as current power 3"
    );
    assert_eq!(host_view.card.toughness.as_deref(), Some("3"));
}

/// A battlefield permanent projects its stored engine counters into
/// [`PermanentView::counters`] as `{ kind, count }` wire entries, in a
/// deterministic order (sorted by [`CounterKind`], the map's key order), and
/// a permanent with no counters projects to an empty list — which
/// `skip_serializing_if` then drops from the JSON entirely (issue #68).
#[test]
fn issue_68_permanent_counters_project_into_the_view() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();

    // Seat 0 holds priority so the state is a valid, viewable snapshot.
    let with_counters = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: with_counters,
        instance: CardInstanceId(0),
        printed: fixture("forest").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        // Insertion order is deliberately reversed from the expected wire
        // order to prove the projection sorts by kind, not by insertion.
        counters: [
            (CounterKind::MinusOneMinusOne, 1),
            (CounterKind::PlusOnePlusOne, 2),
        ]
        .into_iter()
        .collect(),
        attached_to: None,
    });
    let without_counters = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: without_counters,
        instance: CardInstanceId(1),
        printed: fixture("forest").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });

    let view = personalized_view(&state, &db, PlayerId(0));

    let counted = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(with_counters))
        .expect("permanent with counters must appear in the view");
    assert_eq!(
        counted.counters,
        vec![
            Counter {
                kind: "+1/+1".into(),
                count: 2,
            },
            Counter {
                kind: "-1/-1".into(),
                count: 1,
            },
        ],
        "counters must be sorted by kind (+1/+1 before -1/-1), not by insertion order",
    );

    let bare = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(without_counters))
        .expect("permanent without counters must appear in the view");
    assert!(
        bare.counters.is_empty(),
        "a permanent with no counters projects to an empty list",
    );

    // The empty list is dropped from the wire via `skip_serializing_if`, so
    // the serialized shape is unchanged from the always-empty placeholder.
    let json = serde_json::to_value(bare).unwrap();
    assert!(
        json.get("counters").is_none(),
        "empty counters must not be serialized (skip_serializing_if wire shape)",
    );
    let counted_json = serde_json::to_value(counted).unwrap();
    assert!(
        counted_json.get("counters").is_some(),
        "non-empty counters must be serialized",
    );
}

/// Combat declaration state is visible in the projected view (issue #117): an
/// attacking permanent reports `attacking: true`, and a blocker reports the
/// entity id of the attacker it is blocking. A permanent not in combat reports
/// neither.
#[test]
fn issue_117_attack_and_block_state_project_into_the_view() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();

    let attacker = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: attacker,
        instance: CardInstanceId(0),
        printed: fixture("walking_corpse").into(),
        controller: PlayerId(0),
        tapped: true,
        entered_turn: 0,
        attacking: Some(AttackTarget::Player(PlayerId(1))),
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });
    let blocker = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: blocker,
        instance: CardInstanceId(1),
        printed: fixture("walking_corpse").into(),
        controller: PlayerId(1),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: vec![attacker],
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let attacker_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(attacker))
        .expect("attacker in view");
    assert!(attacker_view.attacking);
    assert!(attacker_view.blocking.is_empty());

    let blocker_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(blocker))
        .expect("blocker in view");
    assert!(!blocker_view.attacking);
    assert_eq!(blocker_view.blocking, vec![permanent_entity_id(attacker)]);
}

/// A blocker assigned to more than one attacker (CR 509.1a, issue #739) projects
/// **every** assignment, in the engine's order — which is the order it will assign its
/// combat damage in (CR 509.3). A client draws one relationship per entry and derives
/// neither the count nor the order.
#[test]
fn issue_739_a_blocker_on_two_attackers_projects_both_in_order() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();

    let mut attack = |instance: u64| {
        let id = PermanentId(state.mint_id());
        state.battlefield.push(sage_engine::Permanent {
            id,
            instance: CardInstanceId(instance),
            printed: fixture("walking_corpse").into(),
            controller: PlayerId(0),
            tapped: true,
            entered_turn: 0,
            attacking: Some(AttackTarget::Player(PlayerId(1))),
            blocking: Vec::new(),
            skips_untap: false,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        id
    };
    let first = attack(0);
    let second = attack(1);

    let blocker = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: blocker,
        instance: CardInstanceId(2),
        printed: fixture("ghastbark_twins").into(),
        controller: PlayerId(1),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: vec![second, first],
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let blocker_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(blocker))
        .expect("blocker in view");
    assert_eq!(
        blocker_view.blocking,
        vec![permanent_entity_id(second), permanent_entity_id(first)],
        "both assignments, in the order the engine holds them"
    );
}

/// Marked combat damage (issue #118) projects onto [`PermanentView::damage`]:
/// a damaged permanent reports its marked damage, and an undamaged one reports
/// `0`, which `skip_serializing_if` then drops from the wire.
#[test]
fn issue_118_marked_damage_projects_into_the_view() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();

    let damaged = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: damaged,
        instance: CardInstanceId(0),
        printed: fixture("onakke_ogre").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 2,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let projected = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(damaged))
        .expect("damaged permanent in view");
    assert_eq!(projected.damage, 2);

    // Zero marked damage elides from the JSON (skip_serializing_if wire shape).
    let mut undamaged = projected.clone();
    undamaged.damage = 0;
    let json = serde_json::to_value(&undamaged).unwrap();
    assert!(json.get("damage").is_none());
}

/// Aura attachment (issue #333) projects onto [`PermanentView::attached_to`]: an
/// Aura resolved onto the battlefield through the real engine path reports the
/// entity id of the host it enchants, while its host (and any unattached
/// permanent) reports no attachment and elides the field from the wire.
#[test]
fn issue_333_aura_attachment_projects_into_the_view() {
    use std::collections::BTreeMap;

    // P/T Auras have no clean M19 card, so this is exercised inline (ADR 0009).
    let json = r#"[
        {"schema_version":1,"functional_id":"test_scout","name":"Test Scout",
         "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1},
        {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
         "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
         "attachment":{"kind":"aura","attach_to":"any_creature","power":2,"toughness":2}}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;

    // A host creature already on the battlefield.
    let host = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: host,
        instance: CardInstanceId(0),
        printed: id_in(&db, "test_scout").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: BTreeMap::new(),
        attached_to: None,
    });

    // The Aura spell resolves off the stack attached to the host (CR 303.4d),
    // exactly as the engine's aura-resolution path produces it — no shortcut of
    // hand-populating `attached_to`.
    let aura = state.new_instance(id_in(&db, "test_aegis"));
    let sid = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Spell { card: aura },
        targets: vec![Target::Permanent(host)],
    });
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
    let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);

    let view = personalized_view(&state, &db, PlayerId(0));

    // The Aura's view entry names its host as an entity id.
    let aura_view = view
        .battlefield
        .iter()
        .find(|p| p.attached_to.is_some())
        .expect("the resolved Aura must appear in the view, attached");
    assert_eq!(
        aura_view.attached_to.as_deref(),
        Some(permanent_entity_id(host).as_str()),
        "the Aura names the host it enchants (CR 303.4)",
    );

    // The host itself carries no attachment, and the empty field elides.
    let host_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(host))
        .expect("host in view");
    assert_eq!(host_view.attached_to, None);
    let json = serde_json::to_value(host_view).unwrap();
    assert!(json.get("attached_to").is_none());
}

/// A permanent's printed keywords (issue #153) project onto its card view as
/// lowercase wire names for the client to render, and a keyword-less card omits
/// the field. Snapping Drake has flying; Onakke Ogre has none.
#[test]
fn issue_153_keywords_project_onto_the_card_view() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();

    let flyer = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: flyer,
        instance: CardInstanceId(0),
        printed: fixture("snapping_drake").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });
    let vanilla = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: vanilla,
        instance: CardInstanceId(1),
        printed: fixture("onakke_ogre").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let flyer_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(flyer))
        .expect("flyer in view");
    assert_eq!(flyer_view.card.keywords, vec!["flying".to_string()]);

    let vanilla_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(vanilla))
        .expect("vanilla in view");
    assert!(vanilla_view.card.keywords.is_empty());
    // The empty list elides from the JSON (skip_serializing_if wire shape).
    let json = serde_json::to_value(&vanilla_view.card).unwrap();
    assert!(json.get("keywords").is_none());
}

/// A keyword granted by continuous effect (issue #374) projects onto the
/// permanent's card view exactly like a printed one: a plain body (no printed
/// keyword) enchanted with an Aura granting flying shows `flying` on the wire,
/// and a second, unenchanted body shows none.
#[test]
fn issue_374_granted_keyword_projects_onto_the_card_view() {
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_flight","name":"Test Flight",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{U}","colors":["blue"],
             "attachment":{"kind":"aura","attach_to":"any_creature","keywords":["flying"]}},
            {"schema_version":1,"functional_id":"test_ogre","name":"Test Ogre",
             "types":["creature"],"subtypes":["Ogre"],"mana_cost":"{2}{R}","colors":["red"],
             "power":4,"toughness":2}
        ]"#,
    )
    .unwrap();
    let mut state = GameState::new_two_player();

    let host = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: host,
        instance: CardInstanceId(0),
        printed: id_in(&db, "test_ogre").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });
    let bystander = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: bystander,
        instance: CardInstanceId(1),
        printed: id_in(&db, "test_ogre").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: None,
    });
    // A keyword-only Aura granting flying, attached to the host. M19 prints no
    // such Aura (Prodigious Growth grants trample alongside +7/+7), so the shape
    // is exercised by an inline definition rather than a shipped card (ADR 0009).
    let aura = PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: aura,
        instance: CardInstanceId(2),
        printed: id_in(&db, "test_flight").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: std::collections::BTreeMap::new(),
        attached_to: Some(host),
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let host_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(host))
        .expect("host in view");
    assert_eq!(host_view.card.keywords, vec!["flying".to_string()]);

    let bystander_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(bystander))
        .expect("bystander in view");
    assert!(bystander_view.card.keywords.is_empty());
}

/// The ability-target `requirements` projection (ADR 0004 deferral #73, folded
/// into issue #140): a `{T}: Tap target creature` activation advertises its one
/// target slot with the legal creature candidates, and a returned target
/// resolves to an `ActivateAbility` carrying exactly that chosen target.
#[test]
fn issue_194_cards_project_generated_rules_text_and_their_stable_identity() {
    // ADR 0008 §7-§8: the catalog stores no prose, so what the player reads is
    // composed from the card's IR at projection time — and rides the same view as
    // the card's authored identity, which a future client-local cache could key on.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;

    // Skyscanner (a flying ETB-draw creature) in hand, a Forest on the battlefield.
    let scout = state.new_instance(fixture("skyscanner"));
    state.players[0].hand = vec![scout];
    let forest = put_permanent(&mut state, fixture("forest"), PlayerId(0), false, false);

    let view = personalized_view(&state, &db, PlayerId(0));

    let scout_view = view
        .my_hand
        .iter()
        .find(|c| c.name == "Skyscanner")
        .expect("the skyscanner is in hand");
    assert_eq!(
        scout_view.rules_text, "Flying\nWhen Skyscanner enters the battlefield, draw a card.",
        "the keyword and trigger words are generated from its IR, not stored"
    );
    assert_eq!(scout_view.functional_id, "skyscanner");

    let forest_view = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(forest))
        .map(|p| &p.card)
        .expect("the forest is on the battlefield");
    assert_eq!(forest_view.rules_text, "{T}: Add {G}.");
    assert_eq!(forest_view.functional_id, "forest");

    // A vanilla card claims no rules — and the field is omitted from the wire
    // rather than sent as an empty string.
    let boar = full_card_view(
        "c9".to_string(),
        db.card(fixture("onakke_ogre")).unwrap(),
        &db,
    );
    assert_eq!(boar.rules_text, "");
    let json = serde_json::to_string(&boar).expect("a card view serializes");
    assert!(!json.contains("rules_text"), "{json}");
    assert!(json.contains(r#""functional_id":"onakke_ogre""#), "{json}");
}

/// A basic Forest carries a **green** colour identity (CR 903.4) even though it
/// costs nothing and prints no coloured pip — the case that made a client reading
/// the cost alone draw every land the same shade of grey.
#[test]
fn issue_700_a_basic_land_projects_the_colour_identity_of_the_mana_it_makes() {
    let db = CardDatabase::bundled().unwrap();
    let forest = full_card_view("c1".to_string(), db.card(fixture("forest")).unwrap(), &db);
    assert!(forest.mana_cost.is_none(), "a basic land has no mana cost");
    assert_eq!(forest.color_identity, vec![sage_protocol::Color::Green]);

    // And a colourless card states nothing rather than guessing, so the field
    // elides from the wire exactly as every other additive one does.
    let ogre = full_card_view(
        "c2".to_string(),
        db.card(fixture("onakke_ogre")).unwrap(),
        &db,
    );
    assert_eq!(ogre.color_identity, vec![sage_protocol::Color::Red]);
    let unknown = card_view("c3".to_string(), CardId(9999), &db);
    assert!(unknown.color_identity.is_empty());
    let json = serde_json::to_string(&unknown).expect("a card view serializes");
    assert!(!json.contains("color_identity"), "{json}");
}

/// CR 302.6, on the wire: a creature that entered this turn is restricted, one
/// that entered earlier is not, and haste (CR 702.10b) lifts the restriction — so
/// the flag reports the *restriction* rather than the age of the permanent.
#[test]
fn issue_700_summoning_sickness_rides_the_wire_as_the_restriction_it_is() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.turn = 3;
    state.step = Step::PrecombatMain;

    let settled = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(0),
        false,
        false,
    );
    let fresh = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(0),
        false,
        false,
    );
    let hasty = put_permanent(
        &mut state,
        fixture("hostile_minotaur"),
        PlayerId(0),
        false,
        false,
    );
    for perm in &mut state.battlefield {
        if perm.id == fresh || perm.id == hasty {
            perm.entered_turn = state.turn;
        }
    }
    // A land that entered this turn still taps: only creatures are ever sick.
    let land = put_permanent(&mut state, fixture("forest"), PlayerId(0), false, false);
    for perm in &mut state.battlefield {
        if perm.id == land {
            perm.entered_turn = state.turn;
        }
    }

    let view = personalized_view(&state, &db, PlayerId(0));
    let sick = |id: PermanentId| {
        view.battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(id))
            .expect("the permanent is on the battlefield")
            .summoning_sick
    };
    assert!(
        sick(fresh),
        "a creature that entered this turn is restricted"
    );
    assert!(!sick(settled), "one that entered earlier is not");
    assert!(!sick(hasty), "haste lifts the restriction (CR 702.10b)");
    assert!(!sick(land), "only creatures are ever summoning sick");
}

#[test]
fn issue_194_an_unresolvable_card_projects_no_text_and_no_identity() {
    // The defensive placeholder: an id the catalog does not hold has nothing to
    // generate from and no authored identity to claim — it must not invent either.
    let db = CardDatabase::bundled().unwrap();
    let view = card_view("c1".to_string(), CardId(9999), &db);
    assert_eq!(view.name, "Unknown card 9999");
    assert_eq!(view.rules_text, "");
    assert_eq!(view.functional_id, "");
}

#[test]
fn issue_730_a_skipped_untap_step_rides_the_wire() {
    // The spell that imposes this is in a graveyard by the time anyone looks at the board,
    // and the permanent's own printed text says nothing about it, so a client that was not
    // told would be showing a creature that stays tapped for no stated reason. Projected
    // from the engine's stored flag rather than derived — there is nothing to derive it
    // from.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;

    let held = put_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, false);
    let free = put_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, false);
    for perm in &mut state.battlefield {
        if perm.id == held {
            perm.skips_untap = true;
        }
    }

    let view = personalized_view(&state, &db, PlayerId(0));
    let skips = |id: PermanentId| {
        view.battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(id))
            .expect("the permanent is on the battlefield")
            .skips_next_untap
    };
    assert!(skips(held));
    assert!(!skips(free), "its neighbour untaps normally and says so");
}

#[test]
fn issue_745_the_maximum_hand_size_rides_the_wire_as_two_states() {
    // The cleanup discard is the one turn-based action a player performs on their own
    // hand, so a client that assumed seven would tell a player holding nine that they are
    // about to lose two when a land on the battlefield says otherwise. "No maximum" is a
    // state of its own rather than a large number nobody printed.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;

    let ordinary = personalized_view(&state, &db, PlayerId(0));
    assert_eq!(
        ordinary.me.maximum_hand_size,
        sage_protocol::MaximumHandSize::Cards(7),
        "the default rule, stated"
    );

    put_permanent(
        &mut state,
        fixture("reliquary_tower"),
        PlayerId(0),
        false,
        false,
    );
    let towered = personalized_view(&state, &db, PlayerId(0));
    assert_eq!(
        towered.me.maximum_hand_size,
        sage_protocol::MaximumHandSize::Unlimited
    );
    assert_eq!(
        personalized_view(&state, &db, PlayerId(1))
            .me
            .maximum_hand_size,
        sage_protocol::MaximumHandSize::Cards(7),
        "the other seat's own record is unchanged — the ability says \"you\""
    );
}

#[test]
fn issue_729_a_control_change_files_the_permanent_under_its_new_seat() {
    // The client computes nothing: it draws each permanent in the row of the seat the
    // view files it under. So a control change has to arrive already applied, and it has
    // to leave `owner` behind — that is the field a player reads to know whose card is
    // being borrowed, and the seat CR 400.7 will send it back to.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let borrowed = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(1),
        false,
        false,
    );

    let filed = |view: &sage_protocol::GameView| {
        view.battlefield
            .iter()
            .find(|perm| perm.id == format!("perm_{}", borrowed.0))
            .map(|perm| (perm.controller.clone(), perm.owner.clone()))
            .expect("the permanent is on the projected battlefield")
    };
    assert_eq!(
        filed(&personalized_view(&state, &db, PlayerId(0))),
        ("p1".to_string(), "p1".to_string())
    );

    let source = state.mint_id();
    state.static_effects.push(sage_engine::StaticEffect {
        source,
        affects: sage_engine::EffectAffects::SpecificPermanent(borrowed),
        modification: sage_engine::Modification::GainControl(PlayerId(0)),
        duration: sage_engine::Duration::UntilEndOfTurn,
    });

    assert_eq!(
        filed(&personalized_view(&state, &db, PlayerId(0))),
        ("p0".to_string(), "p1".to_string()),
        "controlled by the thief, still owned by the seat it came from"
    );
    assert_eq!(
        filed(&personalized_view(&state, &db, PlayerId(1))),
        ("p0".to_string(), "p1".to_string()),
        "and the board is the same board from either seat"
    );
}
