//! Lena, Selfless Champion: a token count taken from the board, and a class-wide grant
//! bought by sacrificing the source (issue #722).
//!
//! The card is the one that justifies counting **nontoken** permanents (CR 111). A
//! token-making effect that counted its own tokens would grow every time it resolved, so the
//! distinction is not a nicety — it is the difference between six Soldiers and an argument.
//!
//! Every test drives the real [`apply_action`] pipeline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, Action, CardDatabase, CardId, FunctionalId, GameState, Keyword,
    Permanent, PermanentId, PlayerId, Step,
};

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
        for color in [
            sage_engine::Color::White,
            sage_engine::Color::Blue,
            sage_engine::Color::Black,
            sage_engine::Color::Red,
            sage_engine::Color::Green,
        ] {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`.
fn permanent(
    state: &mut GameState,
    db: &CardDatabase,
    controller: PlayerId,
    slug: &str,
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

/// Put a 1/1 token creature onto the battlefield under `controller` — the thing the count
/// must *not* see.
fn token(state: &mut GameState, controller: PlayerId) -> PermanentId {
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: sage_engine::CardInstanceId(9_001),
        printed: sage_engine::Printed::Token(Box::new(sage_engine::TokenData {
            name: "Goblin".to_string(),
            types: vec![sage_engine::CardType::Creature],
            subtypes: vec!["Goblin".to_string()],
            colors: vec![sage_engine::Color::Red],
            power: Some(1),
            toughness: Some(1),
            ..Default::default()
        })),
        controller,
        ..Default::default()
    });
    id
}

/// Cast Lena from seat 0's hand and let both seats pass so she resolves and her trigger with
/// her.
fn cast_lena(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, "lena_selfless_champion"));
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
    // The spell, then the enters-the-battlefield trigger it puts on the stack.
    let mut state = apply_action(&state, &Action::PassPriority, db);
    for _ in 0..4 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// How many Soldier tokens seat 0 controls.
fn soldiers(state: &GameState, db: &CardDatabase) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.printed.is_token()
                && perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.name() == "Soldier")
        })
        .count()
}

/// The count is of **nontoken** creatures, so the tokens Lena makes do not feed the number
/// that made them, and neither do tokens that were already there.
#[test]
fn issue_722_lena_counts_nontoken_creatures_only() {
    let db = db();
    let mut state = main_phase();
    // Two nontoken creatures, a token creature, and a land that is not a creature at all.
    let _ = permanent(&mut state, &db, PlayerId(0), "onakke_ogre");
    let _ = permanent(&mut state, &db, PlayerId(0), "walking_corpse");
    let _ = permanent(&mut state, &db, PlayerId(0), "forest");
    // The one this rule exists for: a token creature you already control is not counted.
    let _ = token(&mut state, PlayerId(0));
    // An opponent's creature is not yours to count.
    let _ = permanent(&mut state, &db, PlayerId(1), "onakke_ogre");

    let after = cast_lena(&state, &db);

    // Lena herself is a nontoken creature and is on the battlefield when her trigger
    // resolves, so she counts: two others plus herself.
    // Two nontoken creatures plus Lena herself. The Goblin token, the Forest, and the
    // opponent's Ogre are all excluded, each for its own reason.
    assert_eq!(
        soldiers(&after, &db),
        3,
        "expected one Soldier per nontoken creature you control, and the Goblin is not one"
    );
}

/// The number is fixed on resolution (CR 608.2) and nothing later changes it.
#[test]
fn issue_722_the_token_count_is_taken_once_on_resolution() {
    let db = db();
    let mut state = main_phase();
    let _ = permanent(&mut state, &db, PlayerId(0), "onakke_ogre");
    let after = cast_lena(&state, &db);
    let made = soldiers(&after, &db);
    assert_eq!(made, 2, "the Ogre and Lena");

    // A creature arriving afterwards does not retroactively add a Soldier.
    let mut later = after.clone();
    let _ = permanent(&mut later, &db, PlayerId(0), "walking_corpse");
    assert_eq!(soldiers(&later, &db), made);
}

/// Tapping and sacrificing her gives every creature you control indestructible.
#[test]
fn issue_722_sacrificing_lena_grants_indestructible_to_your_creatures() {
    let db = db();
    let mut state = main_phase();
    // Placed rather than cast: this test is about the activated ability, and a permanent
    // that has been here since before the turn began is one whose `{T}` cost is payable
    // (CR 302.6).
    let lena = permanent(&mut state, &db, PlayerId(0), "lena_selfless_champion");
    let ogre = permanent(&mut state, &db, PlayerId(0), "onakke_ogre");
    let theirs = permanent(&mut state, &db, PlayerId(1), "walking_corpse");
    state.players[0].turn_began = state.turn;

    // Taken from what the engine itself offers, so the test cannot address an ability by
    // an index the card no longer has.
    let offered = sage_engine::valid_actions(&state, &db);
    let activation = offered
        .iter()
        .find(
            |action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == lena),
        )
        .cloned()
        .unwrap_or_else(|| panic!("Lena's ability is offered: {offered:#?}"));

    let state = apply_action(&state, &activation, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    // She paid herself as a cost, so she is gone…
    assert!(
        !state.battlefield.iter().any(|perm| perm.id == lena),
        "Lena was sacrificed to pay for her own ability"
    );
    // …and what she left behind is indestructible, on your side only.
    assert!(
        characteristics(&state, ogre, &db)
            .keywords
            .contains(&Keyword::Indestructible),
        "your creature gained indestructible"
    );
    assert!(
        !characteristics(&state, theirs, &db)
            .keywords
            .contains(&Keyword::Indestructible),
        "the grant is to creatures *you* control"
    );
}
