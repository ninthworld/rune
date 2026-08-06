//! Cards with two faces (CR 712, issue #747), driven through the **real**
//! [`apply_action`] pipeline.
//!
//! Two things are being proved here, and they are not the same thing:
//!
//! - **A transform is one permanent turning over, not a new object** (CR 712.a). The
//!   counters, the marked damage, the Aura attached to it, and the combat it is in all
//!   survive, because turning it over changes which face is up and nothing else. Nicol
//!   Bolas does *not* exercise this — his ability exiles and returns him, which is two
//!   zone changes and therefore a new object (CR 400.7) — so an inline definition that
//!   transforms in place carries that half (ADR 0009, the sanctioned pattern for a shape
//!   the shipped set does not use).
//! - **The back face is never cast** (CR 712.4a). It has no mana cost, the catalog
//!   validator refuses one, and no road through the pipeline puts anything but the front
//!   face on the stack — asserted both by what is offered and by forging the action
//!   anyway.
//!
//! Cards are named by their authored `functional_id`, never by an interned handle
//! (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, valid_actions, Action, Attack, AttackTarget, CardDatabase,
    CardId, CardInstance, CardType, CatalogError, Color, CounterKind, Face, FunctionalId,
    GameState, Keyword, Permanent, PermanentId, PlayerId, Step, Violation,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, pools stocked so payability never
/// decides a test that is about a face, and libraries stocked so a draw is never the
/// thing that ends it.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            player.mana_pool.add(color, 20);
        }
        player.mana_pool.add_colorless(20);
    }
    let filler = cid(db, "forest");
    for seat in [PlayerId(0), PlayerId(1)] {
        for _ in 0..40 {
            let instance = state.new_instance(filler);
            state.players[seat.0].library.push(instance);
        }
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, front face up.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        ..Default::default()
    });
    id
}

/// Whether the priority holder is offered ability `index` of `permanent`.
fn offers(state: &GameState, db: &CardDatabase, permanent: PermanentId, index: usize) -> bool {
    valid_actions(state, db).contains(&Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
        payment: Vec::new(),
    })
}

/// Activate ability `index` of `permanent` and let it resolve, asserting the engine
/// offered it first.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> GameState {
    assert!(
        offers(state, db, permanent, index),
        "ability {index} was not offered"
    );
    let after = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    assert_ne!(&after, state, "the activation was rejected");
    let after = apply_action(&after, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

/// The permanent projecting card instance `instance`, whichever object it is now.
fn by_instance(state: &GameState, instance: sage_engine::CardInstanceId) -> &Permanent {
    state
        .battlefield
        .iter()
        .find(|p| p.instance == instance)
        .expect("the card is on the battlefield")
}

/// The name of the face `id` is currently showing.
fn face_name(state: &GameState, db: &CardDatabase, id: PermanentId) -> String {
    state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| p.printed.face(db))
        .map(|face| face.name().to_string())
        .expect("the permanent is on the battlefield")
}

// ----- the card model -------------------------------------------------------

#[test]
fn cr_712_2_a_card_has_an_ordered_list_of_faces_and_one_identity() {
    // The load-bearing half of the model: a two-faced card is *one* card. It has one
    // authored identity, one printing, and one row in the compatibility report — so
    // nothing about `data/sets/` had to learn that faces exist.
    let db = db();
    let bolas = db.card(cid(&db, "nicol_bolas_the_ravager")).unwrap();
    assert_eq!(bolas.faces(), vec![Face::Front, Face::Back]);
    assert!(bolas.has_back_face());
    assert_eq!(bolas.functional_id.as_str(), "nicol_bolas_the_ravager");

    // Every other card in the catalog is a one-face card, unchanged.
    let ogre = db.card(cid(&db, "onakke_ogre")).unwrap();
    assert_eq!(ogre.faces(), vec![Face::Front]);
    assert!(!ogre.has_back_face());
    assert!(ogre.face(Face::Back).is_none());

    // The printing names the card, not a face: M19 #218 is one record, resolving to the
    // one identity above.
    let printings = sage_engine::PrintingDatabase::bundled(&db).unwrap();
    let printing = printings
        .printing("M19", "218")
        .expect("M19 #218 is printed");
    assert_eq!(printing.oracle, cid(&db, "nicol_bolas_the_ravager"));
}

#[test]
fn cr_712_4a_a_back_face_has_no_mana_cost_and_takes_the_fronts_mana_value() {
    let db = db();
    let bolas = db.card(cid(&db, "nicol_bolas_the_ravager")).unwrap();
    let back = bolas.back_face.as_deref().unwrap();
    assert_eq!(back.mana_cost, "", "a back face has no mana cost");

    // CR 712.4d: it takes the *front* face's mana value all the same, so a transformed
    // Elder Dragon is still mana value 4 rather than 0.
    let front_face = bolas.face(Face::Front).unwrap();
    let back_face = bolas.face(Face::Back).unwrap();
    assert_eq!(front_face.mana_value(), 4);
    assert_eq!(back_face.mana_cost(), "");
    assert_eq!(back_face.mana_value(), 4);
}

#[test]
fn cr_712_4a_the_validator_refuses_a_back_face_that_carries_a_mana_cost() {
    // The rule is *enforced*, not assumed: the field exists so an author who writes one
    // is told which card is wrong.
    let json = r#"[{"schema_version":1,"functional_id":"test_castable_back","name":"Test Castable Back",
        "types":["creature"],"mana_cost":"{1}{U}","colors":["blue"],"power":1,"toughness":1,
        "back_face":{"name":"Test Back","types":["creature"],"mana_cost":"{2}{U}",
                     "colors":["blue"],"power":3,"toughness":3}}]"#;
    let err = CardDatabase::from_json(json).expect_err("a castable back face is refused");
    assert!(
        matches!(
            &err,
            CatalogError::Schema(Violation::BackFaceHasManaCost { functional_id })
                if functional_id == "test_castable_back"
        ),
        "the violation names the rule and the card: {err}"
    );
    assert!(format!("{err}").contains("test_castable_back"));

    // The same definition with the cost removed is fine, which is what makes the check a
    // rule about the cost rather than about having a back face at all.
    let ok = json.replace(r#""mana_cost":"{2}{U}","#, "");
    assert!(CardDatabase::from_json(&ok).is_ok());
}

#[test]
fn cr_701_28d_the_validator_refuses_a_transform_with_nowhere_to_turn() {
    // An ability that turns a permanent over on a card with one face can never do
    // anything, and the engine's honest answer to it is silence — so it is refused at
    // authoring time instead.
    let json = r#"[{"schema_version":1,"functional_id":"test_one_sided","name":"Test One Sided",
        "types":["creature"],"mana_cost":"{U}","colors":["blue"],"power":1,"toughness":1,
        "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
          "effects":[{"kind":"transform_self"}]}]}]"#;
    let err = CardDatabase::from_json(json).expect_err("a transform with no back face");
    assert!(
        matches!(
            &err,
            CatalogError::Schema(Violation::TransformWithoutABackFace { functional_id })
                if functional_id == "test_one_sided"
        ),
        "{err}"
    );
}

#[test]
fn cr_712_2_every_rule_a_face_obeys_is_asked_of_the_back_face_too() {
    // A back face that is a planeswalker needs its own starting loyalty, and a back face
    // that is a creature needs its own power and toughness — the same both-directions
    // pairing the front is held to.
    let missing_loyalty = r#"[{"schema_version":1,"functional_id":"test_no_loyalty","name":"Test No Loyalty",
        "types":["creature"],"mana_cost":"{U}","colors":["blue"],"power":1,"toughness":1,
        "back_face":{"name":"Test Walker","types":["planeswalker"],"colors":["blue"]}}]"#;
    assert!(CardDatabase::from_json(missing_loyalty).is_err());

    let half_a_body = r#"[{"schema_version":1,"functional_id":"test_half_body","name":"Test Half Body",
        "types":["creature"],"mana_cost":"{U}","colors":["blue"],"power":1,"toughness":1,
        "back_face":{"name":"Test Body","types":["creature"],"colors":["blue"],"power":3}}]"#;
    assert!(CardDatabase::from_json(half_a_body).is_err());
}

// ----- CR 712.a: a transform is the same object -----------------------------

/// A creature that turns over in place, and an Aura to hang on it. No M19 card
/// transforms without changing zones, so this shape is driven from an inline definition
/// (ADR 0009).
fn transform_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_turncoat","name":"Test Turncoat",
         "types":["creature"],"subtypes":["Human"],"mana_cost":"{1}{W}","colors":["white"],
         "power":2,"toughness":2,
         "abilities":[{"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
           "effects":[{"kind":"transform_self"}]}],
         "back_face":{"name":"Test Werewolf","types":["creature"],"subtypes":["Werewolf"],
           "colors":["white"],"power":4,"toughness":4,"keywords":["trample"]}},
        {"schema_version":1,"functional_id":"test_harness","name":"Test Harness",
         "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{W}","colors":["white"],
         "attachment":{"kind":"aura","attach_to":"any_creature","power":1,"toughness":1}},
        {"schema_version":1,"functional_id":"test_wall","name":"Test Wall",
         "types":["creature"],"subtypes":["Wall"],"mana_cost":"{W}","colors":["white"],
         "power":0,"toughness":6}
    ]"#;
    CardDatabase::from_json(json).expect("the inline transform catalog")
}

fn inline_main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        player.mana_pool.add_colorless(10);
        player.mana_pool.add(Color::White, 10);
    }
    state
}

#[test]
fn cr_712_a_a_transform_keeps_the_permanent_it_turns_over() {
    // The whole of CR 712.a in one activation: the object does not change, so its id,
    // its counters, its marked damage, and the Aura attached to it are all still there
    // afterwards — and what *has* changed is exactly the characteristics of the face.
    let db = transform_db();
    let mut state = inline_main_phase();

    let card = cid(&db, "test_turncoat");
    let instance = state.new_instance(card);
    let turncoat = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: turncoat,
        instance: instance.id,
        printed: card.into(),
        controller: PlayerId(0),
        counters: [(CounterKind::PlusOnePlusOne, 2)].into_iter().collect(),
        damage: 1,
        ..Default::default()
    });

    // An Aura really attached to it, so the surviving attachment is a permanent that has
    // to still find its host rather than a field nobody reads.
    let aura_card = cid(&db, "test_harness");
    let aura_instance = state.new_instance(aura_card);
    let aura = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: aura,
        instance: aura_instance.id,
        printed: aura_card.into(),
        controller: PlayerId(0),
        attached_to: Some(turncoat),
        ..Default::default()
    });

    // Front face up: a 2/2 with two +1/+1 counters and a +1/+1 Aura, so 5/5, no trample.
    let before = characteristics(&state, turncoat, &db);
    assert_eq!(face_name(&state, &db, turncoat), "Test Turncoat");
    assert_eq!((before.power, before.toughness), (Some(5), Some(5)));
    assert!(!before.keywords.contains(&Keyword::Trample));

    let after = activate(&state, &db, turncoat, 0);

    // The same object, by the one handle that says so.
    let perm = after
        .battlefield
        .iter()
        .find(|p| p.id == turncoat)
        .expect("the permanent did not leave the battlefield");
    assert_eq!(perm.instance, instance.id);
    assert_eq!(
        after
            .battlefield
            .iter()
            .filter(|p| p.id == turncoat)
            .count(),
        1,
        "no second permanent was created"
    );

    // Everything that lives beside the face survived it (CR 712.a).
    assert_eq!(perm.counter_count(CounterKind::PlusOnePlusOne), 2);
    assert_eq!(perm.damage, 1);
    assert_eq!(
        after
            .battlefield
            .iter()
            .find(|p| p.id == aura)
            .unwrap()
            .attached_to,
        Some(turncoat),
        "the Aura is still attached to the same object"
    );

    // And the face really did change: a 4/4 back face, plus the same two counters and the
    // same Aura, with the trample the other side prints.
    assert_eq!(face_name(&after, &db, turncoat), "Test Werewolf");
    let now = characteristics(&after, turncoat, &db);
    assert_eq!((now.power, now.toughness), (Some(7), Some(7)));
    assert!(now.keywords.contains(&Keyword::Trample));
}

#[test]
fn cr_712_a_a_transform_in_combat_leaves_the_combat_alone() {
    // The half of CR 712.a a snapshot of the permanent's fields would not prove: the
    // creature is really in a declared combat, and turning it over neither removes it
    // from the attack nor unblocks it.
    let db = transform_db();
    let mut state = inline_main_phase();

    let card = cid(&db, "test_turncoat");
    let instance = state.new_instance(card);
    let turncoat = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: turncoat,
        instance: instance.id,
        printed: card.into(),
        controller: PlayerId(0),
        entered_turn: 0,
        ..Default::default()
    });
    state.players[0].turn_began = 1;
    state.turn = 1;

    let wall_card = cid(&db, "test_wall");
    let wall_instance = state.new_instance(wall_card);
    let wall = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: wall,
        instance: wall_instance.id,
        printed: wall_card.into(),
        controller: PlayerId(1),
        ..Default::default()
    });

    // Declare the attack and the block for real.
    let mut combat = state.clone();
    combat.step = Step::DeclareAttackers;
    combat.priority = PlayerId(0);
    let combat = apply_action(
        &combat,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: turncoat,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    let mut blocking = combat.clone();
    blocking.step = Step::DeclareBlockers;
    blocking.priority = PlayerId(1);
    let blocking = apply_action(
        &blocking,
        &Action::DeclareBlockers {
            blocks: vec![sage_engine::Block {
                blocker: wall,
                attacker: turncoat,
            }],
        },
        &db,
    );
    let mut ready = blocking;
    ready.priority = PlayerId(0);
    assert_eq!(
        ready
            .battlefield
            .iter()
            .find(|p| p.id == turncoat)
            .unwrap()
            .attacking,
        Some(AttackTarget::Player(PlayerId(1)))
    );

    let after = activate(&ready, &db, turncoat, 0);

    let perm = after.battlefield.iter().find(|p| p.id == turncoat).unwrap();
    assert_eq!(
        perm.attacking,
        Some(AttackTarget::Player(PlayerId(1))),
        "the attacker is still attacking the same player"
    );
    assert_eq!(
        after
            .battlefield
            .iter()
            .find(|p| p.id == wall)
            .unwrap()
            .blocking,
        vec![turncoat],
        "the blocker is still blocking the same object"
    );
    assert_eq!(face_name(&after, &db, turncoat), "Test Werewolf");
}

// ----- Nicol Bolas ----------------------------------------------------------

#[test]
fn issue_747_nicol_bolas_is_cast_as_his_front_face() {
    // CR 712.4a: a card outside the battlefield has only its front face's
    // characteristics, so casting one can only ever produce that face.
    let db = db();
    let mut state = main_phase(&db);
    let instance = state.new_instance(cid(&db, "nicol_bolas_the_ravager"));
    state.players[0].hand.push(instance);

    let cast = Action::CastSpell {
        card: instance,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(
        valid_actions(&state, &db).contains(&cast),
        "the card is castable from hand"
    );

    let after = apply_action(&state, &cast, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    let mut resolved = apply_action(&after, &Action::PassPriority, &db);
    // The enters-the-battlefield trigger goes on the stack; let it resolve. Both hands
    // are empty, so the discard it asks for is applied outright rather than posed.
    for _ in 0..4 {
        if resolved.stack.is_empty() {
            break;
        }
        resolved = apply_action(&resolved, &Action::PassPriority, &db);
    }

    let perm = by_instance(&resolved, instance.id);
    assert_eq!(perm.printed.face_up(), Face::Front);
    let face = perm.printed.face(&db).unwrap();
    assert_eq!(face.name(), "Nicol Bolas, the Ravager");
    assert!(face.has_type(CardType::Creature));
    assert!(!face.has_type(CardType::Planeswalker));
    assert_eq!((face.power(), face.toughness()), (Some(4), Some(4)));
    assert!(face.has_keyword(Keyword::Flying));
    assert_eq!(
        perm.counter_count(CounterKind::Loyalty),
        0,
        "the front face is a creature and enters with no loyalty"
    );
}

#[test]
fn issue_747_the_back_face_can_never_be_cast() {
    // Two independent gates, because a rule that only withholds an offer is a rule a
    // forged action walks straight through.
    let db = db();
    let mut state = main_phase(&db);
    let bolas = place(&mut state, &db, "nicol_bolas_the_ravager", PlayerId(0));
    let instance = state
        .battlefield
        .iter()
        .find(|p| p.id == bolas)
        .unwrap()
        .instance;
    let card = cid(&db, "nicol_bolas_the_ravager");

    // Turn him over for real, so the back face is genuinely the face in play.
    let transformed = activate(&state, &db, bolas, 1);
    let arisen = by_instance(&transformed, instance);
    assert_eq!(arisen.printed.face_up(), Face::Back);

    // 1. Nothing offers a cast of it. The back face is not in any hand, has no mana
    //    cost, and no announcement road names a face at all.
    let forged = Action::CastSpell {
        card: CardInstance {
            id: arisen.instance,
            card,
        },
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(
        !valid_actions(&transformed, &db)
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.card == cid(&db, "nicol_bolas_the_ravager"))),
        "no cast of the transformed card is offered"
    );

    // 2. Forging it anyway is rejected: `apply_action` returns the state it was handed.
    assert_eq!(
        apply_action(&transformed, &forged, &db),
        transformed,
        "a forged cast of the back face changes nothing"
    );

    // And the reason it could never be paid for in the first place: no cost.
    assert_eq!(
        db.card(card)
            .unwrap()
            .back_face
            .as_deref()
            .unwrap()
            .mana_cost,
        ""
    );
}

#[test]
fn issue_747_the_activation_exiles_him_and_returns_him_transformed() {
    // CR 701.28 by the *other* road: two zone changes, so what comes back is a new
    // object (CR 400.7) — with the back face's own starting loyalty (CR 306.5b), which
    // is the whole reason the printed card works.
    let db = db();
    let mut state = main_phase(&db);
    let ravager = place(&mut state, &db, "nicol_bolas_the_ravager", PlayerId(0));
    let instance = state
        .battlefield
        .iter()
        .find(|p| p.id == ravager)
        .unwrap()
        .instance;

    let after = activate(&state, &db, ravager, 1);

    // The old object is gone; the same physical card is back as a different one.
    assert!(
        !after.battlefield.iter().any(|p| p.id == ravager),
        "the permanent that activated the ability is not on the battlefield"
    );
    let arisen = by_instance(&after, instance);
    assert_ne!(arisen.id, ravager, "CR 400.7: a new object");
    assert_eq!(arisen.controller, PlayerId(0), "under its owner's control");
    assert!(
        after.players.iter().all(|p| p.exile.is_empty()),
        "the exile was a stop, not a destination"
    );

    // Back face up, and it is a planeswalker with its printed loyalty.
    assert_eq!(arisen.printed.face_up(), Face::Back);
    let face = arisen.printed.face(&db).unwrap();
    assert_eq!(face.name(), "Nicol Bolas, the Arisen");
    assert!(face.has_type(CardType::Planeswalker));
    assert!(!face.has_type(CardType::Creature));
    assert_eq!(face.loyalty(), Some(7));
    assert_eq!(arisen.counter_count(CounterKind::Loyalty), 7);
    assert_eq!(face.power(), None, "a planeswalker has no power");

    // Its abilities are the back face's four, and nothing of the front's remains.
    let abilities = sage_engine::abilities_of_permanent(&after, &db, arisen);
    assert_eq!(abilities.len(), 4);
    assert!(abilities.iter().all(sage_engine::is_loyalty_ability));
}

#[test]
fn issue_747_the_arisen_walker_really_uses_its_loyalty_abilities() {
    // A face that is up is a face that works: the `+2` is offered off the back face,
    // costs loyalty the object has, and draws.
    let db = db();
    let mut state = main_phase(&db);
    let ravager = place(&mut state, &db, "nicol_bolas_the_ravager", PlayerId(0));
    let after = activate(&state, &db, ravager, 1);
    let arisen = after
        .battlefield
        .iter()
        .find(|p| p.printed.face_up() == Face::Back)
        .unwrap()
        .id;

    let hand_before = after.players[0].hand.len();
    let drawn = activate(&after, &db, arisen, 0);
    let walker = drawn.battlefield.iter().find(|p| p.id == arisen).unwrap();
    assert_eq!(walker.counter_count(CounterKind::Loyalty), 9);
    assert_eq!(drawn.players[0].hand.len(), hand_before + 2);
}

#[test]
fn cr_602_5d_the_transform_activation_is_sorcery_speed() {
    // "Activate only as a sorcery." — authored on the ability, gated in the offer, and
    // re-derived in `apply_action` so a forged action cannot slip it through a window
    // that is not its controller's.
    let db = db();
    let mut state = main_phase(&db);
    let ravager = place(&mut state, &db, "nicol_bolas_the_ravager", PlayerId(0));
    assert!(offers(&state, &db, ravager, 1), "offered in a main phase");

    let mut in_combat = state.clone();
    in_combat.step = Step::DeclareBlockers;
    assert!(
        !offers(&in_combat, &db, ravager, 1),
        "not offered during combat"
    );
    let forged = Action::ActivateAbility {
        permanent: ravager,
        index: 1,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert_eq!(
        apply_action(&in_combat, &forged, &db),
        in_combat,
        "a forged combat-step activation changes nothing"
    );

    // Not on an opponent's turn either, even in their main phase.
    let mut their_turn = state.clone();
    their_turn.active_player = PlayerId(1);
    assert!(
        !offers(&their_turn, &db, ravager, 1),
        "not offered on an opponent's turn"
    );
}

#[test]
fn issue_747_a_single_face_card_is_unaffected() {
    // The other half of the promise: nothing about a one-face card changed. It has one
    // face, it enters front-face up, and it can never be turned over (CR 701.28d).
    let db = db();
    let mut state = main_phase(&db);
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let perm = state.battlefield.iter().find(|p| p.id == ogre).unwrap();
    assert_eq!(perm.printed.face_up(), Face::Front);
    assert_eq!(perm.printed.face(&db).unwrap().name(), "Onakke Ogre");
    assert!(!db
        .card(perm.printed.card().unwrap())
        .unwrap()
        .has_back_face());
}
