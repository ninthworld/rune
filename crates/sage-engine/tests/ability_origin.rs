//! An ability on the stack records **how it got there** (issue #579).
//!
//! Two code paths put a `StackObjectKind::Ability` on the stack — a player's
//! activation (CR 602.2, `apply_activate_ability`) and the game's own trigger
//! collection (CR 603.3, the `collect_triggers` loop in `apply_action`) — and the
//! objects they produce are otherwise indistinguishable: same source, same effects,
//! same composed description. Whichever it was is therefore recorded at the push or
//! it is lost, and the server can never state it (`docs/design/stack-and-relationships.md`
//! §11.1, gap G2).
//!
//! Both push sites are driven here through the public `apply_action` path in one
//! narrative — an artifact's activated ability kills a creature, whose dies trigger
//! then goes on the stack — so the assertions cover the real pushes rather than
//! hand-built stack objects.

#![allow(clippy::unwrap_used, clippy::panic)]

use sage_engine::{
    apply_action, AbilityOrigin, Action, CardDatabase, CardId, GameState, Permanent, PermanentId,
    PlayerId, StackObjectKind, Step, Target,
};

/// An inline catalog (ADR 0026: an unrepresented shape comes from its own catalog
/// rather than from a bent M19 card) with a "Sparker" artifact whose activated
/// ability is `{T}: deal 2 damage to any target`, and a 2/2 "Lurker" whose only
/// ability triggers on its own death. Two damage kills the Lurker, so one
/// activation produces one trigger.
fn origin_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_sparker","name":"Test Sparker",
         "types":["artifact"],"mana_cost":"{2}",
         "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                       "effects":[{"kind":"deal_damage","target":"any_target","amount":2}]}]},
        {"schema_version":1,"functional_id":"test_lurker","name":"Test Lurker",
         "types":["creature"],"subtypes":["Horror"],"mana_cost":"{1}{B}","colors":["black"],
         "power":2,"toughness":2,
         "abilities":[{"type":"triggered","event":"self_dies",
                       "effects":[{"kind":"draw_card","count":1}]}]}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// The [`CardId`] of `functional_id` in `db` — never written down (ADR 0018 §3).
fn cid(db: &CardDatabase, functional_id: &str) -> CardId {
    db.card_id(&functional_id.to_string().try_into().unwrap())
        .unwrap()
}

/// Put an untapped permanent of `card` onto the battlefield under player 0.
fn place(state: &mut GameState, card: CardId) -> PermanentId {
    let instance = state.new_instance(card);
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: instance.id,
        card,
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: None,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
    });
    id
}

/// A precombat-main two-player game with the Sparker and the Lurker on player 0's
/// battlefield and one card in the library for the dies trigger to draw.
fn sparker_and_lurker(db: &CardDatabase) -> (GameState, PermanentId, PermanentId) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let sparker = place(&mut state, cid(db, "test_sparker"));
    let lurker = place(&mut state, cid(db, "test_lurker"));
    let draw = state.new_instance(cid(db, "test_lurker"));
    state.players[0].library = vec![draw];
    (state, sparker, lurker)
}

/// The origin recorded on the single object on the stack.
fn only_ability(state: &GameState) -> (PermanentId, AbilityOrigin) {
    assert_eq!(state.stack.len(), 1, "exactly one object on the stack");
    match state.stack[0].kind {
        StackObjectKind::Ability { source, origin, .. } => (source, origin),
        StackObjectKind::Spell { .. } => panic!("expected an ability on the stack"),
    }
}

#[test]
fn issue_579_each_push_site_records_its_own_ability_origin() {
    let db = origin_db();
    let (state, sparker, lurker) = sparker_and_lurker(&db);

    // The activation push site (CR 602.2): a player chose this ability and paid its
    // `{T}` cost, and the object says so.
    let activated = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: sparker,
            index: 0,
            targets: vec![Target::Permanent(lurker)],
        },
        &db,
    );
    assert_eq!(
        only_ability(&activated),
        (sparker, AbilityOrigin::Activated),
        "an activation records Activated at its own push site",
    );

    // Resolving it kills the Lurker, whose dies trigger the *game* puts on the stack
    // — the trigger push site (CR 603.3). Nothing about the resulting object differs
    // from the activation above except the origin it recorded.
    let triggered = apply_action(&activated, &Action::PassPriority, &db);
    let triggered = apply_action(&triggered, &Action::PassPriority, &db);
    assert!(
        !triggered.battlefield.iter().any(|p| p.id == lurker),
        "two damage killed the 2/2, so its dies trigger fired",
    );
    assert_eq!(
        only_ability(&triggered),
        (lurker, AbilityOrigin::Triggered),
        "a trigger records Triggered at its own push site",
    );

    // The two are therefore distinguishable in engine state — the whole of the new
    // information, and the only thing a projection could read (issue #579).
    assert_ne!(only_ability(&activated).1, only_ability(&triggered).1);

    // The trigger still resolves like any other ability: nothing about carrying the
    // origin changes resolution.
    let resolved = apply_action(&triggered, &Action::PassPriority, &db);
    let resolved = apply_action(&resolved, &Action::PassPriority, &db);
    assert!(resolved.stack.is_empty());
    assert_eq!(
        resolved.players[0].hand.len(),
        1,
        "the dies trigger drew a card"
    );
}
