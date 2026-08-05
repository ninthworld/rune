//! Attacking a planeswalker: the defender slot, and what a declaration may name.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use sage_engine::{Attack, AttackTarget, FunctionalId, PlayerId, Step};
use sage_protocol::TargetChoice;

/// A planeswalker and a creature, inline: no bundled M19 planeswalker is authorable
/// (each needs an emblem), so the projection is exercised against a definition of
/// the shape the shipped set cannot represent — ADR 0009's sanctioned pattern.
fn planeswalker_db() -> CardDatabase {
    CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_warden","name":"Test Warden",
             "supertypes":["legendary"],"types":["planeswalker"],"subtypes":["Warden"],
             "mana_cost":"{2}{W}{W}","colors":["white"],"loyalty":4},
            {"schema_version":1,"functional_id":"test_ogre","name":"Test Ogre",
             "types":["creature"],"subtypes":["Ogre"],"mana_cost":"{2}{R}",
             "colors":["red"],"power":4,"toughness":2}
        ]"#,
    )
    .unwrap()
}

fn card(db: &CardDatabase, slug: &str) -> sage_engine::CardId {
    db.card_id(&FunctionalId::try_from(slug.to_string()).unwrap())
        .unwrap()
}

/// A two-player declare-attackers state: seat 0 has an attacker, seat 1 the
/// planeswalker. Returns the attacker and the planeswalker.
fn combat(db: &CardDatabase) -> (GameState, PermanentId, PermanentId) {
    let mut state = GameState::new_two_player();
    state.turn = 3;
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    let ogre = crate::view::test_support::put_permanent(
        &mut state,
        card(db, "test_ogre"),
        PlayerId(0),
        false,
        false,
    );
    let warden = crate::view::test_support::put_permanent(
        &mut state,
        card(db, "test_warden"),
        PlayerId(1),
        false,
        false,
    );
    // CR 306.5b: a planeswalker on the battlefield carries its printed loyalty as
    // counters. The shared test helper builds a bare permanent, so they are placed
    // here — without them the very first state-based check would put the
    // planeswalker into its owner's graveyard (CR 704.5i) before anything could
    // attack it.
    state
        .battlefield
        .iter_mut()
        .find(|p| p.id == warden)
        .unwrap()
        .counters
        .insert(sage_engine::CounterKind::Loyalty, 4);
    (state, ogre, warden)
}

/// Issue #608: a **two-player** game gains a defender slot the moment an opponent
/// controls a planeswalker, because there are now two things to attack. The slot's
/// candidates mix a seat id and a permanent id in one list — which is exactly what
/// an attack may name (CR 508.1a).
///
/// The counter-case is what makes this a test of the widening rather than of the
/// fixture: the same board without the planeswalker offers no slot at all, so the
/// two-player wire is unchanged wherever nothing is there to choose between.
#[test]
fn issue_608_a_planeswalker_gives_a_two_player_game_a_defender_slot() {
    let db = planeswalker_db();
    let (state, ogre, warden) = combat(&db);

    let reqs = attacker_requirements(&state, &db);
    let slot = reqs
        .iter()
        .find(|r| r.slot == defender_slot(ogre))
        .expect("the attacker is offered a choice of what to attack");
    assert_eq!(
        slot.candidates,
        vec![player_id(PlayerId(1)), permanent_entity_id(warden)],
        "the opponent and their planeswalker, in that order"
    );
    assert!(slot.prompt.contains("Test Ogre"));
    // Issue #700: whose choice this is, stated rather than encoded in the slot id
    // — so a client asks one attacker at a time without ever parsing `defend_…`.
    assert_eq!(slot.subject, Some(permanent_entity_id(ogre)));
    let whole = reqs
        .iter()
        .find(|r| r.slot == "attackers")
        .expect("the declaration itself is always a slot");
    assert_eq!(
        whole.subject, None,
        "the multi-select is about the declaration, not about one attacker"
    );

    // Remove the planeswalker: one thing to attack, so no slot and no extra step.
    let mut plain = state.clone();
    plain.battlefield.retain(|p| p.id != warden);
    assert!(
        !attacker_requirements(&plain, &db)
            .iter()
            .any(|r| r.slot == defender_slot(ogre)),
        "with only the sole opponent to attack the wire is exactly as before"
    );
}

/// The returned answer binds to a concrete `Attack` naming the planeswalker — over
/// the *freshly recomputed* candidates, so a stale or forged id resolves to nothing
/// rather than to whatever happens to sit at that id now.
#[test]
fn issue_608_a_returned_planeswalker_choice_binds_to_the_engine_action() {
    let db = planeswalker_db();
    let (state, ogre, warden) = combat(&db);
    let offered = attacker_requirements(&state, &db);

    let answer = |chosen: &str| {
        vec![
            TargetChoice {
                slot: "attackers".to_string(),
                chosen: vec![permanent_entity_id(ogre)],
            },
            TargetChoice {
                slot: defender_slot(ogre),
                chosen: vec![chosen.to_string()],
            },
        ]
    };

    assert_eq!(
        bind_attackers(&state, &db, &offered, &answer(&permanent_entity_id(warden))),
        Some(sage_engine::Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Planeswalker(warden),
            }],
        }),
    );
    // The player is still bindable through the same slot…
    assert_eq!(
        bind_attackers(&state, &db, &offered, &answer(&player_id(PlayerId(1)))),
        Some(sage_engine::Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        }),
    );
    // …and an id naming nothing attackable binds to nothing at all.
    assert_eq!(
        bind_attackers(&state, &db, &offered, &answer("perm_9999")),
        None,
        "an id outside the fresh candidate set is rejected, not coerced"
    );
    assert_eq!(
        bind_attackers(&state, &db, &offered, &answer(&player_id(PlayerId(0)))),
        None,
        "you cannot attack yourself"
    );
}

/// The projection states both halves of an attack on a planeswalker: what is being
/// attacked, and which seat answers for it (its controller). A client needs both
/// told to it — deriving the second from the first is a rules lookup.
#[test]
fn issue_608_the_view_names_the_attacked_planeswalker_and_its_controller() {
    let db = planeswalker_db();
    let (state, ogre, warden) = combat(&db);
    let state = sage_engine::apply_action(
        &state,
        &sage_engine::Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Planeswalker(warden),
            }],
        },
        &db,
    );

    let view = crate::view::personalized_view(&state, &db, PlayerId(0));
    let attacker = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(ogre))
        .unwrap();
    assert!(attacker.attacking);
    assert_eq!(
        attacker.attacking_planeswalker.as_deref(),
        Some(permanent_entity_id(warden).as_str()),
        "the planeswalker being attacked is named outright"
    );
    assert_eq!(
        attacker.attacking_player.as_deref(),
        Some(player_id(PlayerId(1)).as_str()),
        "and so is the seat that answers for it — its controller"
    );

    // The planeswalker itself shows its printed loyalty on its face and its current
    // loyalty as a counter. Both are present and they are different channels.
    let pw = view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(warden))
        .unwrap();
    assert_eq!(pw.card.loyalty.as_deref(), Some("4"));
    assert!(!pw.attacking);
}
