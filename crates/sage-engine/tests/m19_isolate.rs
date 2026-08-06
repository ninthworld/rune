//! Isolate (M19 #17): a target spec filtered by **mana value** (CR 202.3, issue #748).
//!
//! Every spec before this one named a type, a controller, a zone, or a keyword. This one
//! reads a number off the printed face, which makes three questions worth asking of it:
//! that the value is compared for *equality* rather than as a cap, that the comparison is
//! made against the same face a token answers with (CR 111.4 — a token's mana value is
//! zero, not "absent"), and that the offer and the CR 608.2b re-check agree, since both
//! run the one predicate.
//!
//! Every test drives the real [`apply_action`] over the bundled catalog.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, target_requirements, Action, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Printed, Step, Target, TokenData,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
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

/// A 1/1 token on the battlefield: a permanent with no card behind it and therefore no
/// mana cost at all (CR 111.4).
fn place_token(state: &mut GameState, controller: PlayerId) -> PermanentId {
    let id = PermanentId(state.mint_id());
    let instance = state.mint_id();
    state.battlefield.push(Permanent {
        id,
        instance: sage_engine::CardInstanceId(instance),
        printed: Printed::Token(Box::new(TokenData {
            name: "Soldier".to_string(),
            types: vec![sage_engine::CardType::Creature],
            subtypes: vec!["Soldier".to_string()],
            colors: vec![Color::White],
            power: Some(1),
            toughness: Some(1),
            ..Default::default()
        })),
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

/// The candidate set Isolate's one slot offers from `state`.
fn candidates(state: &GameState, db: &CardDatabase, card: CardInstance) -> Vec<Target> {
    let slots = target_requirements(
        state,
        db,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    assert_eq!(slots.len(), 1, "one target, one slot");
    assert!(!slots[0].optional, "and a mandatory one");
    slots[0].candidates.clone()
}

/// Cast Isolate at `target` and let it resolve, or `None` if the announcement was
/// refused.
fn cast_at(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    target: PermanentId,
) -> Option<GameState> {
    let after = apply_action(
        state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(target)],
            payment: Vec::new(),
        },
        db,
    );
    if &after == state {
        return None;
    }
    let after = apply_action(&after, &Action::PassPriority, db);
    Some(apply_action(&after, &Action::PassPriority, db))
}

// ----- the filter ------------------------------------------------------------

#[test]
fn issue_748_isolate_takes_a_mana_value_one_permanent_and_leaves_the_rest() {
    // `{W}` is mana value 1 and `{1}{W}` is 2 (CR 202.3), so a one-drop is a candidate
    // and its two-drop neighbour is not — and it is exiled rather than destroyed, which
    // is the verb the card prints.
    let db = db();
    let mut state = main_phase();
    let one_drop = place(&mut state, &db, "rustwing_falcon", PlayerId(1));
    let two_drop = place(&mut state, &db, "ajani_s_pridemate", PlayerId(1));
    let card = to_hand(&mut state, &db, "isolate", PlayerId(0));

    let legal = candidates(&state, &db, card);
    assert!(legal.contains(&Target::Permanent(one_drop)));
    assert!(
        !legal.contains(&Target::Permanent(two_drop)),
        "the filter is an equality, not a cap: mana value 2 is out"
    );

    let after = cast_at(&state, &db, card, one_drop).expect("a legal target");
    assert!(
        !after.battlefield.iter().any(|p| p.id == one_drop),
        "the one-drop left the battlefield"
    );
    assert_eq!(
        after.players[1].exile.len(),
        1,
        "into exile, not a graveyard"
    );
    assert!(after.players[1].graveyard.is_empty());
    assert!(after.battlefield.iter().any(|p| p.id == two_drop));
}

#[test]
fn issue_748_a_mana_value_two_permanent_is_refused_by_apply_action() {
    // Not offered, and not applied either: the announcement gate and the resolution
    // re-check are the same predicate, so a forged action naming an ineligible permanent
    // is a no-op rather than an exile.
    let db = db();
    let mut state = main_phase();
    let two_drop = place(&mut state, &db, "ajani_s_pridemate", PlayerId(1));
    let card = to_hand(&mut state, &db, "isolate", PlayerId(0));

    assert!(cast_at(&state, &db, card, two_drop).is_none());
    assert!(
        state.battlefield.iter().any(|p| p.id == two_drop),
        "and it is still there"
    );
}

#[test]
fn issue_748_a_token_is_a_mana_value_zero_permanent() {
    // CR 111.4: a token has no mana cost, so its mana value is 0 — a real number rather
    // than a missing one. It is therefore a permanent Isolate misses, and one a spell
    // naming mana value 0 would hit. The land beside it is the same fact from the card
    // side: a mana cost of nothing is a mana value of zero.
    let db = db();
    let mut state = main_phase();
    let token = place_token(&mut state, PlayerId(1));
    let land = place(&mut state, &db, "plains", PlayerId(1));
    let one_drop = place(&mut state, &db, "rustwing_falcon", PlayerId(1));
    let card = to_hand(&mut state, &db, "isolate", PlayerId(0));

    let legal = candidates(&state, &db, card);
    assert!(
        !legal.contains(&Target::Permanent(token)),
        "a token is worth 0, so mana value 1 does not reach it"
    );
    assert!(!legal.contains(&Target::Permanent(land)));
    assert_eq!(
        legal,
        vec![Target::Permanent(one_drop)],
        "and the one-drop is the whole legal set"
    );

    assert!(
        cast_at(&state, &db, card, token).is_none(),
        "aiming at it anyway is refused rather than crashing on a permanent with no card",
    );
}

#[test]
fn issue_748_the_filter_reaches_every_permanent_type_and_either_seat() {
    // `any_permanent_with_mana_value` narrows the permanent universe rather than
    // replacing it, so a one-mana artifact, a one-mana enchantment, and the caster's own
    // one-drop are all candidates. "Target permanent" is not "target creature an
    // opponent controls", and the number is the only thing this spec adds.
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "rustwing_falcon", PlayerId(0));
    let theirs = place(&mut state, &db, "court_cleric", PlayerId(1));
    let enchantment = place(&mut state, &db, "ajani_s_welcome", PlayerId(1));
    let card = to_hand(&mut state, &db, "isolate", PlayerId(0));

    let legal = candidates(&state, &db, card);
    for id in [mine, theirs, enchantment] {
        assert!(
            legal.contains(&Target::Permanent(id)),
            "every mana-value-1 permanent is a candidate",
        );
    }

    let after = cast_at(&state, &db, card, enchantment).expect("a legal target");
    assert!(!after.battlefield.iter().any(|p| p.id == enchantment));
}
