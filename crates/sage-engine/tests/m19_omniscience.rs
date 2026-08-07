//! Casting from hand **without paying a mana cost** (issue #723).
//!
//! Omniscience is the card, and the alternative cost (CR 601.2b) is what is under test. It is
//! answered where every cost is answered, so the offer, the payment search, the charge, and
//! the view cannot disagree: a spell advertised free and then charged would be the worst
//! possible shape for this rule.
//!
//! Every test drives the real [`apply_action`] pipeline and the real [`valid_actions`] offer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, total_cast_cost, valid_actions, Action, CardDatabase, CardId, CardInstance,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A main phase with **no mana at all**, which is the whole point: anything castable here
/// is castable because nothing is being paid.
fn dry_main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    state
}

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

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// With no mana and no Omniscience, an expensive spell is simply not offered — the control
/// the other tests are read against.
#[test]
fn issue_723_without_the_permission_an_unpayable_spell_is_not_offered() {
    let db = db();
    let mut state = dry_main_phase();
    let dragon = to_hand(&mut state, &db, "shivan_dragon", PlayerId(0));

    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == dragon.id)),
        "an unpayable spell is not offered"
    );
    assert_ne!(
        total_cast_cost(&state, &db, dragon),
        Some(sage_engine::ManaCost::default()),
        "and it is not priced at nothing"
    );
}

/// With Omniscience out, the same spell costs nothing and is offered on an empty pool.
#[test]
fn issue_723_omniscience_prices_a_hand_cast_at_nothing_and_offers_it() {
    let db = db();
    let mut state = dry_main_phase();
    let _omniscience = place(&mut state, &db, "omniscience", PlayerId(0));
    let dragon = to_hand(&mut state, &db, "shivan_dragon", PlayerId(0));

    // One function answers what a cast costs, so this is the number the offer, the
    // payment, and the charge all read.
    assert_eq!(
        total_cast_cost(&state, &db, dragon),
        Some(sage_engine::ManaCost::default()),
        "the mana cost is replaced, not reduced"
    );

    let cast = valid_actions(&state, &db)
        .into_iter()
        .find(|action| matches!(action, Action::CastSpell { card, .. } if card.id == dragon.id))
        .expect("the spell is offered with an empty pool");

    let after = apply_action(&state, &cast, &db);
    assert!(
        !after.players[0].hand.iter().any(|c| c.id == dragon.id),
        "the spell left the hand for the stack"
    );
    assert_eq!(
        after.players[0].mana_pool, state.players[0].mana_pool,
        "nothing was charged for it"
    );
}

/// It reaches the **hand** and no other zone, because that is what the card says.
#[test]
fn issue_723_the_permission_does_not_reach_a_graveyard_cast() {
    let db = db();
    let mut state = dry_main_phase();
    let _omniscience = place(&mut state, &db, "omniscience", PlayerId(0));
    let buried = state.new_instance(cid(&db, "shivan_dragon"));
    state.players[0].graveyard.push(buried);

    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == buried.id)),
        "a graveyard cast needs its own permission, and free-casting is not one"
    );
}

/// It is the source's controller's permission, not everybody's.
#[test]
fn issue_723_the_permission_belongs_to_the_seat_that_controls_the_source() {
    let db = db();
    let mut state = dry_main_phase();
    let _omniscience = place(&mut state, &db, "omniscience", PlayerId(0));
    let theirs = to_hand(&mut state, &db, "shivan_dragon", PlayerId(1));

    // Priority is seat 0's, so ask the cost question as seat 1 by handing them priority.
    let mut opponent_turn = state.clone();
    opponent_turn.priority = PlayerId(1);
    assert_ne!(
        total_cast_cost(&opponent_turn, &db, theirs),
        Some(sage_engine::ManaCost::default()),
        "the opponent still pays for their own spells"
    );
}

/// An **additional** cost the card names is still paid: CR 601.2b replaces the mana cost
/// and nothing else.
#[test]
fn issue_723_an_additional_cost_is_still_owed() {
    let db = db();
    let mut state = dry_main_phase();
    let _omniscience = place(&mut state, &db, "omniscience", PlayerId(0));
    // Blood Divination's additional cost is sacrificing a creature; with no creature to
    // sacrifice it stays unofferable however free its mana cost has become.
    let spell = to_hand(&mut state, &db, "blood_divination", PlayerId(0));

    assert_eq!(
        total_cast_cost(&state, &db, spell),
        Some(sage_engine::ManaCost::default()),
        "the mana half is free"
    );
    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == spell.id)),
        "but the additional cost is still owed, and cannot be paid"
    );

    // Give it something to eat, and the same spell becomes castable — proving the refusal
    // above was the additional cost and not something else.
    let mut fed = state.clone();
    let _food = place(&mut fed, &db, "onakke_ogre", PlayerId(0));
    assert!(
        valid_actions(&fed, &db)
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == spell.id)),
        "with a creature to sacrifice it is castable"
    );
}

/// The permission ends with its source, and a spell already cast stays cast.
#[test]
fn issue_723_the_permission_ends_when_the_source_leaves() {
    let db = db();
    let mut state = dry_main_phase();
    let omniscience = place(&mut state, &db, "omniscience", PlayerId(0));
    let dragon = to_hand(&mut state, &db, "shivan_dragon", PlayerId(0));
    assert_eq!(
        total_cast_cost(&state, &db, dragon),
        Some(sage_engine::ManaCost::default())
    );

    let mut gone = state.clone();
    gone.battlefield.retain(|perm| perm.id != omniscience);
    assert_ne!(
        total_cast_cost(&gone, &db, dragon),
        Some(sage_engine::ManaCost::default()),
        "nothing is stored, so the permission leaves with its source"
    );
}

/// A targeted free cast still names its target — the alternative cost changes the price and
/// nothing else about casting.
#[test]
fn issue_723_a_free_cast_still_announces_its_targets() {
    let db = db();
    let mut state = dry_main_phase();
    let _omniscience = place(&mut state, &db, "omniscience", PlayerId(0));
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let murder = to_hand(&mut state, &db, "murder", PlayerId(0));

    let cast = Action::CastSpell {
        card: murder,
        mode: None,
        x: None,
        targets: vec![Target::Permanent(victim)],
        payment: Vec::new(),
    };
    let state = apply_action(&state, &cast, &db);
    assert!(!state.stack.is_empty(), "the spell is on the stack");
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        !state.battlefield.iter().any(|perm| perm.id == victim),
        "and it resolved against the target it named"
    );
}
