//! Optional costs paid with something other than mana (issue #744): `you may sacrifice
//! another creature. If you do, …`.
//!
//! The `may` wrapper's half of the chosen costs a cast and an activation already carry
//! (#721). The difference is *when* the payment is picked. A cast or an activation is
//! announced by a player holding priority, so the permanent they spend rides on the
//! action; a `may` is answered in the middle of somebody's resolution, where there is no
//! action to hang it on — so accepting poses the sacrifice as its own question and the
//! effects it bought wait behind it.
//!
//! What every test here is really about is that widening the cost widened nothing else:
//! a cost with nothing to pay it is still never *posed*, a decline still skips the
//! wrapped effects and nothing more, and a chooser owed the question may still activate
//! mana abilities and still nothing else (CR 605.3a).
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008
//! §3); the shapes no bundled card prints are driven from an inline definition (ADR 0009).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, valid_actions, Action, Attack, AttackTarget, CardDatabase,
    CardId, ChoiceQuestion, FunctionalId, GameEvent, GameState, Permanent, PermanentId, PlayerId,
    Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main with **empty pools**: the costs under
/// test are not mana, so any mana in a test is mana that test put there on purpose.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`.
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

/// Walk the pipeline to the declare-attackers step, attack player 1 with `attacker`, and
/// let both seats pass so the attack trigger resolves — which is the moment the offer is
/// either posed or silently declined.
fn attack_with(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let mut state = state.clone();
    for _ in 0..40 {
        if state.step == Step::DeclareAttackers {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    assert_eq!(state.step, Step::DeclareAttackers, "combat was reached");
    state.priority = PlayerId(0);
    let declared = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    // The trigger is on the stack; two passes resolve it.
    let passed = apply_action(&declared, &Action::PassPriority, db);
    apply_action(&passed, &Action::PassPriority, db)
}

/// Answer the pending yes-or-no.
fn answer(state: &GameState, db: &CardDatabase, accept: bool) -> GameState {
    apply_action(state, &Action::AnswerConfirm { accept }, db)
}

/// Pay a pending sacrifice with `chosen`.
fn sacrifice(state: &GameState, db: &CardDatabase, chosen: Vec<PermanentId>) -> GameState {
    apply_action(state, &Action::AnswerPermanents { chosen }, db)
}

fn pt(state: &GameState, db: &CardDatabase, id: PermanentId) -> (i32, i32) {
    let c = characteristics(state, id, db);
    (c.power.unwrap_or(0), c.toughness.unwrap_or(0))
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// The permanents the pending sacrifice offers, or an empty list when the pending
/// question is not one.
fn sacrifice_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    match sage_engine::pending_player_choice(state).map(|pending| &pending.question) {
        Some(ChoiceQuestion::Permanents(request)) => {
            sage_engine::permanent_choice_candidates(state, request, db)
        }
        _ => Vec::new(),
    }
}

fn declined(state: &GameState) -> bool {
    state
        .log
        .iter()
        .any(|entry| matches!(entry.event, GameEvent::OptionalDeclined { .. }))
}

// ----- the card -------------------------------------------------------------

#[test]
fn brawl_bash_ogre_eats_a_friend_for_the_pump() {
    // The whole card, in order: the attack trigger asks, accepting owes a sacrifice, the
    // sacrifice is answered, and only then does the Ogre grow. The payment precedes what
    // it bought because it is posed as a question the remainder waits behind.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "brawl_bash_ogre", PlayerId(0));
    let friend = place(&mut state, &db, "centaur_courser", PlayerId(0));

    let asked = attack_with(&state, &db, ogre);
    assert!(
        sage_engine::pending_player_choice(&asked)
            .and_then(|pending| pending.question.confirm())
            .is_some(),
        "the attack trigger posed the offer",
    );
    assert_eq!(
        pt(&asked, &db, ogre),
        (3, 3),
        "and nothing has happened yet"
    );

    let owed = answer(&asked, &db, true);
    assert_eq!(
        sacrifice_candidates(&owed, &db),
        vec![friend],
        "accepting owes the sacrifice, and the Ogre is not another creature",
    );
    assert!(
        on_battlefield(&owed, friend),
        "the cost is not charged until it is answered",
    );
    assert_eq!(
        pt(&owed, &db, ogre),
        (3, 3),
        "nor is the pump applied early"
    );

    let paid = sacrifice(&owed, &db, vec![friend]);
    assert!(!on_battlefield(&paid, friend), "the friend was sacrificed");
    assert_eq!(pt(&paid, &db, ogre), (5, 5), "and the Ogre got +2/+2");
    assert!(
        sage_engine::pending_player_choice(&paid).is_none(),
        "nothing is left owed",
    );
    assert_eq!(paid.priority, PlayerId(0), "priority is handed back");
}

#[test]
fn declining_the_cost_sacrifices_nothing_and_pumps_nothing() {
    // A decline is the absence of an event: measured against the same attack made with
    // no creature to spend, the two boards agree on everything.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "brawl_bash_ogre", PlayerId(0));
    let friend = place(&mut state, &db, "centaur_courser", PlayerId(0));

    let refused = answer(&attack_with(&state, &db, ogre), &db, false);
    assert!(on_battlefield(&refused, friend), "nothing was sacrificed");
    assert_eq!(pt(&refused, &db, ogre), (3, 3), "and nothing was gained");
    assert!(
        sage_engine::pending_player_choice(&refused).is_none(),
        "declining owes no payment",
    );
    assert!(declined(&refused), "the decline is on the record");
}

#[test]
fn a_board_with_no_other_creature_is_never_asked() {
    // The never-stall rule, unchanged by the cost being a sacrifice: a question with no
    // yes in it is not posed and then auto-answered, it is not posed. The Ogre attacking
    // alone is exactly that board — it cannot pay with itself (CR 701.17: "another").
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "brawl_bash_ogre", PlayerId(0));
    // An opponent's creature is not payment either (CR 701.17b), and neither is a land.
    place(&mut state, &db, "centaur_courser", PlayerId(1));
    place(&mut state, &db, "forest", PlayerId(0));

    let after = attack_with(&state, &db, ogre);
    assert!(
        sage_engine::pending_player_choice(&after).is_none(),
        "no question was posed at all",
    );
    assert!(
        !valid_actions(&after, &db).contains(&Action::AnswerConfirm { accept: false }),
        "and no answer is on offer to give",
    );
    assert_eq!(pt(&after, &db, ogre), (3, 3), "the effect did not happen");
    assert!(
        declined(&after),
        "it is recorded as declined rather than omitted",
    );
    assert!(
        on_battlefield(&after, ogre),
        "and the Ogre is still attacking"
    );
}

#[test]
fn the_sacrifice_is_a_real_death_that_dies_watchers_see() {
    // CR 701.17b, and the reason the payment goes through the one leaves-battlefield seam
    // rather than quietly removing a permanent from a vector. Doomed Dissenter's own dies
    // trigger fires for a creature sacrificed to an optional cost exactly as it does for
    // one destroyed in combat.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "brawl_bash_ogre", PlayerId(0));
    let dissenter = place(&mut state, &db, "doomed_dissenter", PlayerId(0));

    let owed = answer(&attack_with(&state, &db, ogre), &db, true);
    let paid = sacrifice(&owed, &db, vec![dissenter]);
    // The dies trigger goes on the stack behind the resumed resolution; two passes let it
    // resolve.
    let settled = apply_action(&paid, &Action::PassPriority, &db);
    let settled = apply_action(&settled, &Action::PassPriority, &db);

    assert!(!on_battlefield(&settled, dissenter));
    assert!(
        settled
            .battlefield
            .iter()
            .any(|perm| perm.printed.face(&db).map(|face| face.name()) == Some("Zombie")),
        "the dies trigger fired and left a Zombie behind",
    );
    assert_eq!(
        pt(&settled, &db, ogre),
        (5, 5),
        "and the pump still happened"
    );
}

#[test]
fn cr_605_3a_a_mana_ability_stays_legal_while_the_offer_is_owed_and_nothing_else_does() {
    // The freeze is unchanged by the cost being a sacrifice: the chooser answers, taps for
    // mana, or concedes. Nothing else — not a pass, not a cast, not the activated ability
    // of a creature they control — and no other seat is offered anything at all.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "brawl_bash_ogre", PlayerId(0));
    let friend = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let forest = place(&mut state, &db, "forest", PlayerId(0));

    let owed = attack_with(&state, &db, ogre);
    let tap = Action::ActivateAbility {
        permanent: forest,
        index: 0,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert_eq!(
        valid_actions(&owed, &db),
        vec![
            Action::AnswerConfirm { accept: false },
            tap.clone(),
            Action::Concede
        ],
        "the answer, the mana ability, and concede — nothing else",
    );

    let mut other_seat = owed.clone();
    other_seat.priority = PlayerId(1);
    assert!(
        valid_actions(&other_seat, &db).is_empty(),
        "no other seat may act while the question is owed",
    );

    // Tapping the land answers nothing, and the question survives it.
    let floated = apply_action(&owed, &tap, &db);
    assert_eq!(floated.players[0].mana_pool.green, 1);
    assert!(sage_engine::pending_player_choice(&floated).is_some());

    // And once the offer is accepted, the payment's own question freezes even that: a
    // sacrifice needs no mana, so nothing is offered but the answer.
    let paying = answer(&floated, &db, true);
    assert_eq!(
        valid_actions(&paying, &db),
        vec![
            Action::AnswerPermanents { chosen: Vec::new() },
            Action::Concede
        ],
        "the payment is answered and nothing else is legal",
    );
    assert_eq!(sacrifice_candidates(&paying, &db), vec![friend]);
}

#[test]
fn a_payment_naming_a_permanent_the_cost_did_not_ask_for_is_refused() {
    // The regenerate-and-check discipline, over the widened filter: the source is excluded
    // by `another`, an opponent's creature was never theirs to spend (CR 701.17b), and a
    // land is not a creature. Each is a submission a client can really send.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "brawl_bash_ogre", PlayerId(0));
    place(&mut state, &db, "centaur_courser", PlayerId(0));
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let land = place(&mut state, &db, "forest", PlayerId(0));

    let owed = answer(&attack_with(&state, &db, ogre), &db, true);
    for wrong in [ogre, theirs, land, PermanentId(9999)] {
        assert_eq!(
            sacrifice(&owed, &db, vec![wrong]),
            owed,
            "a payment naming {wrong:?} changes nothing",
        );
    }
    assert_eq!(
        sacrifice(&owed, &db, Vec::new()),
        owed,
        "and a cost is not paid by paying nothing",
    );
}

// ----- the wrapper, in isolation --------------------------------------------

/// A card whose optional sacrifice sits **between** two mandatory effects, so "a decline
/// skips the wrapped effects and nothing else" has something on both sides of it to be
/// measured against. No printed card in the set has this shape; the mechanism does
/// (ADR 0009).
const DEFINITIONS: &str = r#"[
    {"schema_version":1,"functional_id":"test_bargain","name":"Test Bargain",
     "types":["sorcery"],"mana_cost":"",
     "spell_effects":[{"kind":"gain_life","player_ref":"controller","amount":1},
                      {"kind":"may",
                       "cost":{"kind":"sacrifice","card_type":"creature"},
                       "effects":[{"kind":"draw_card","count":1}]},
                      {"kind":"gain_life","player_ref":"controller","amount":2}]},
    {"schema_version":1,"functional_id":"test_rummage","name":"Test Rummage",
     "types":["sorcery"],"mana_cost":"",
     "spell_effects":[{"kind":"may","cost":{"kind":"discard","count":1},
                       "effects":[{"kind":"draw_card","count":2}]}]},
    {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
     "types":["creature"],"mana_cost":"","power":2,"toughness":2}
]"#;

fn inline_db() -> CardDatabase {
    CardDatabase::from_json(DEFINITIONS).expect("the test definitions load")
}

/// Cast `slug` from player 0's hand and let both players pass, so it resolves.
fn cast(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

#[test]
fn a_declined_cost_leaves_every_other_effect_of_the_ability_untouched() {
    // Three effects, the middle one optional and costed. Declining takes the middle one
    // out and touches neither neighbour: both life gains happen, in printed order, and
    // the spell still reaches its graveyard (CR 608.3).
    let db = inline_db();
    let mut state = main_phase();
    let bear = place(&mut state, &db, "test_bear", PlayerId(0));
    state.players[0].library = (0..3)
        .map(|_| state.new_instance(cid(&db, "test_bear")))
        .collect();

    let refused = answer(&cast(&state, &db, "test_bargain"), &db, false);
    assert_eq!(refused.players[0].life, 23, "both mandatory effects ran");
    assert!(on_battlefield(&refused, bear), "nothing was sacrificed");
    assert!(refused.players[0].hand.is_empty(), "and nothing was drawn");
    assert_eq!(
        refused.players[0].graveyard.len(),
        1,
        "the spell reached its final zone",
    );

    // The same card accepted: the cost is paid, the draw happens, and the effect *after*
    // the offer still runs once the payment is answered.
    let owed = answer(&cast(&state, &db, "test_bargain"), &db, true);
    assert_eq!(owed.players[0].life, 21, "only the first gain so far");
    let paid = sacrifice(&owed, &db, vec![bear]);
    assert!(!on_battlefield(&paid, bear));
    assert_eq!(paid.players[0].hand.len(), 1, "the draw happened");
    assert_eq!(paid.players[0].life, 23, "and so did the effect after it");
}

#[test]
fn an_optional_discard_is_the_same_mechanism_over_a_hand() {
    // The third payment in the vocabulary, to prove the shape is the cost's rather than
    // the sacrifice's: accepting poses the ordinary card selection, and an empty hand is
    // a cost nobody could pay, so the question is never posed.
    let db = inline_db();
    let mut state = main_phase();
    state.players[0].library = (0..4)
        .map(|_| state.new_instance(cid(&db, "test_bear")))
        .collect();

    let empty_handed = cast(&state, &db, "test_rummage");
    assert!(
        sage_engine::pending_player_choice(&empty_handed).is_none(),
        "nothing to discard, nothing to ask",
    );
    assert!(declined(&empty_handed));

    let mut holding = state.clone();
    let card = holding.new_instance(cid(&db, "test_bear"));
    holding.players[0].hand.push(card);
    let owed = answer(&cast(&holding, &db, "test_rummage"), &db, true);
    let discarded = apply_action(
        &owed,
        &Action::AnswerChoice {
            chosen: vec![card.id],
        },
        &db,
    );
    assert_eq!(discarded.players[0].hand.len(), 2, "one out, two in");
    assert!(discarded.players[0]
        .graveyard
        .iter()
        .any(|inst| inst.id == card.id));
}
