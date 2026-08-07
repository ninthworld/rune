//! Abilities that function while their source is in a **graveyard** (CR 113.6), built
//! against Reassembling Skeleton (M19 #116) — the smallest card that proves the seam.
//!
//! Every other activated ability in the engine starts from a permanent: the offer walks
//! the battlefield, the cost is paid by the source, and the effects modify it. This one
//! starts from a card in a zone, which has no [`sage_engine::PermanentId`] at all — so
//! the tests here are mostly about the *absence* of the battlefield. The ability is not
//! offered while the card is in hand or on the battlefield, it is offered while the card
//! is in its controller's graveyard, and an opponent who removes the card in response is
//! left with an ability that resolves and does nothing.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId,
    GameState, Permanent, PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main. `mana` is how much of every colour
/// each seat floats, so a test about *timing* or *zone* can stock the pool and a test
/// about cost can starve it.
fn main_phase(db: &CardDatabase, mana: u8) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let forest = cid(db, "forest");
    for seat in 0..2 {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            state.players[seat].mana_pool.add(color, mana);
        }
        state.players[seat].mana_pool.add_colorless(mana);
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a Reassembling Skeleton into `seat`'s graveyard and return the instance.
fn to_graveyard(state: &mut GameState, db: &CardDatabase, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, "reassembling_skeleton"));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// The requirement form of the graveyard activation — the shape `valid_actions`
/// advertises, with no target filled in. Index 0 because the Skeleton prints no other
/// ability.
fn offer(card: CardInstance) -> Action {
    Action::ActivateAbilityFromGraveyard {
        card,
        index: 0,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// Pass priority until the top of the stack has resolved.
fn resolve(state: &GameState, db: &CardDatabase) -> GameState {
    let after = apply_action(state, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

// ----- the offer ------------------------------------------------------------

#[test]
fn issue_723_a_graveyard_ability_is_offered_from_the_graveyard_and_nowhere_else() {
    // CR 113.6: where an ability functions is a fact about the ability, and this one
    // functions in exactly one zone. The same card, with the same pool, offers the
    // activation from a graveyard and offers nothing at all from a hand — the difference
    // is the zone and nothing else.
    let db = db();
    let mut state = main_phase(&db, 5);

    let in_hand = state.new_instance(cid(&db, "reassembling_skeleton"));
    state.players[0].hand.push(in_hand);
    assert!(
        !valid_actions(&state, &db).contains(&offer(in_hand)),
        "a card in hand offers no graveyard activation"
    );

    let mut buried = main_phase(&db, 5);
    let card = to_graveyard(&mut buried, &db, PlayerId(0));
    assert!(
        valid_actions(&buried, &db).contains(&offer(card)),
        "the same card in its controller's graveyard offers it"
    );
}

#[test]
fn issue_723_a_graveyard_ability_is_not_offered_on_the_battlefield() {
    // The mirror of the gate above, and the one that would otherwise pass silently: a
    // Skeleton *on the battlefield* has the ability printed on it, and every other
    // activation the engine offers comes off exactly that list. Withheld here rather than
    // offered and then found to have no card in a graveyard to return.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = cid(&db, "reassembling_skeleton");
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller: PlayerId(0),
        ..Default::default()
    });

    assert!(
        !valid_actions(&state, &db).iter().any(|action| matches!(
            action,
            Action::ActivateAbility { permanent, .. } if *permanent == id
        )),
        "the permanent offers no activation of an ability that functions in a graveyard"
    );
}

#[test]
fn issue_723_only_the_graveyards_own_seat_is_offered_the_activation() {
    // A graveyard is a per-player zone (CR 404.1) and the offer is enumerated from the
    // priority holder's own. An opponent holding priority at instant speed sees nothing,
    // however much mana they have.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));

    state.priority = PlayerId(1);
    assert!(
        !valid_actions(&state, &db).contains(&offer(card)),
        "the other seat is not offered a card in someone else's graveyard"
    );
}

#[test]
fn issue_723_the_offer_and_the_charge_agree_about_the_cost() {
    // The offer is gated on the pool as it stands, exactly as a battlefield activation
    // is. One black short of `{1}{B}` and the ability is simply absent; with the mana
    // floating it is on offer and the activation charges it.
    let db = db();
    let mut broke = main_phase(&db, 0);
    let card = to_graveyard(&mut broke, &db, PlayerId(0));
    assert!(
        !valid_actions(&broke, &db).contains(&offer(card)),
        "an unpayable cost withholds the offer"
    );

    let mut funded = broke.clone();
    funded.players[0].mana_pool.add(Color::Black, 2);
    assert!(valid_actions(&funded, &db).contains(&offer(card)));

    let after = apply_action(&funded, &offer(card), &db);
    assert_eq!(
        after.players[0].mana_pool.black, 0,
        "{{1}}{{B}} was paid out of the two black floating"
    );
}

// ----- the activation and its resolution ------------------------------------

#[test]
fn issue_723_the_card_stays_in_the_graveyard_until_the_ability_resolves() {
    // CR 602.2b: activating puts the ability on the stack and pays its cost; it does not
    // move the card. That is what makes a response to it worth anything — and it is the
    // difference between this and a cast, which takes the card to the stack.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));

    let activated = apply_action(&state, &offer(card), &db);
    assert_eq!(activated.stack.len(), 1, "the ability is on the stack");
    assert!(
        activated.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == card.id),
        "the card has not moved"
    );
    assert!(
        activated.battlefield.is_empty(),
        "and nothing has entered the battlefield yet"
    );

    let resolved = resolve(&activated, &db);
    assert!(
        !resolved.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == card.id),
        "resolving takes the card out of the graveyard"
    );
    let skeleton = resolved
        .battlefield
        .iter()
        .find(|p| p.instance == card.id)
        .expect("the same physical card is on the battlefield");
    assert!(skeleton.tapped, "it returns tapped, as the card says");
    assert_eq!(skeleton.controller, PlayerId(0));
    assert_eq!(
        skeleton.entered_turn, resolved.turn,
        "a battlefield entry is a fresh object, summoning sick like any other arrival"
    );
}

#[test]
fn issue_723_the_activation_can_be_repeated_for_as_long_as_the_mana_lasts() {
    // The whole point of the card: it comes back, dies, and comes back again. Two full
    // cycles, driven through the real pipeline, prove the second activation is not
    // reading anything the first one left behind.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));

    let mut state = resolve(&apply_action(&state, &offer(card), &db), &db);
    assert_eq!(state.battlefield.len(), 1);

    // Back to the graveyard. However it died is not this test's subject — the
    // leaves-battlefield seam has its own — so the move is made directly, and what
    // matters is that the *same physical card* is in the graveyard again.
    let skeleton = state.battlefield.remove(0);
    state.players[0].graveyard.push(CardInstance {
        id: skeleton.instance,
        card: cid(&db, "reassembling_skeleton"),
    });
    state.priority = PlayerId(0);
    assert!(
        valid_actions(&state, &db).contains(&offer(card)),
        "the same card, back in the graveyard, offers the ability again"
    );

    let state = resolve(&apply_action(&state, &offer(card), &db), &db);
    assert_eq!(
        state.battlefield.len(),
        1,
        "and the second activation returns it a second time"
    );
    assert_eq!(state.battlefield[0].instance, card.id);
}

#[test]
fn issue_723_a_card_removed_in_response_leaves_an_ability_that_does_nothing() {
    // The source is looked up on **resolution**, not at announcement (CR 608.2). An
    // opponent who exiles the card while the ability is on the stack leaves it with
    // nothing to return: it resolves, moves nothing, and the mana is still spent.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));

    let mut activated = apply_action(&state, &offer(card), &db);
    // Removal from outside the action pipeline: whatever exiled it, the ability is
    // already on the stack and the card is gone.
    activated.players[0].graveyard.retain(|c| c.id != card.id);
    activated.players[0].exile.push(card);

    let resolved = resolve(&activated, &db);
    assert!(
        resolved.battlefield.is_empty(),
        "nothing came back — there was nothing left to come back"
    );
    assert!(
        resolved.players[0].exile.iter().any(|c| c.id == card.id),
        "and the card stayed where it was put"
    );
    assert!(resolved.stack.is_empty(), "the ability left the stack");
}

// ----- the apply-time gate --------------------------------------------------

#[test]
fn issue_723_apply_rejects_an_activation_of_a_card_no_longer_in_the_graveyard() {
    // Handed the action directly — a stale or forged id `valid_actions` would not offer —
    // `apply_action` refuses it and changes nothing. The gate re-derives the zone from
    // current state rather than trusting the offer list.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));
    let action = offer(card);
    assert!(valid_actions(&state, &db).contains(&action));

    let mut moved = state.clone();
    moved.players[0].graveyard.clear();
    moved.players[0].hand.push(card);

    let after = apply_action(&moved, &action, &db);
    assert_eq!(after, moved, "an illegal activation changes nothing");
    assert!(after.stack.is_empty());
    assert_eq!(
        after.players[0].mana_pool.black, 5,
        "and no mana was spent for it"
    );
}

#[test]
fn issue_723_apply_rejects_an_activation_taken_without_priority() {
    // The timing gate is the hand cast's, and it is re-derived at apply rather than only
    // at offer time: submitted from a seat that does not hold priority, the activation is
    // refused. `action_is_legal` regenerates `valid_actions` for the *current* holder, so
    // the offer player 0 held a moment ago is worthless once priority has moved.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));
    let action = offer(card);
    assert!(valid_actions(&state, &db).contains(&action));

    let mut opponents_window = state.clone();
    opponents_window.priority = PlayerId(1);
    let after = apply_action(&opponents_window, &action, &db);
    assert_eq!(
        after, opponents_window,
        "without priority the activation is a no-op"
    );
}

#[test]
fn issue_723_apply_rejects_an_activation_while_a_combat_declaration_is_owed() {
    // The other half of "the same timing gates as a hand cast": while a turn-based player
    // choice is owed, `valid_actions` offers that choice and nothing else — no spells, no
    // abilities. A graveyard activation is bound by that window exactly as a hand cast is,
    // because it is enumerated by the same function, past the same early returns.
    let db = db();
    let mut state = main_phase(&db, 5);
    let card = to_graveyard(&mut state, &db, PlayerId(0));
    state.step = Step::DeclareAttackers;
    state.attackers_declared = false;

    assert!(!valid_actions(&state, &db).contains(&offer(card)));
    let after = apply_action(&state, &offer(card), &db);
    assert_eq!(after, state, "the declaration window admits nothing else");
}
