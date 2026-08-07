//! CR 613 layer 5: the colours a continuous effect **adds** (issue #706).
//!
//! Rise from the Grave is the card, and its second sentence is the whole test: `That
//! creature is a black Zombie in addition to its other colors and types.` Both halves are
//! continuous effects on a permanent that **did not exist when the spell was cast** — no
//! target could have been chosen for it — so they ride the effect that made it and last
//! exactly as long as it does.
//!
//! What makes layer 5 a layer is the same thing that made layer 4 one: every rule that
//! asks about colour now gets the answer the layer produced. A creature made black is
//! black to an evasion restriction, to a count of black permanents, and to a spell that
//! may only target a colourless creature.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, Action, CardDatabase, CardId, CardInstance, CardType, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    for player in &mut state.players {
        for color in [Color::Black, Color::Green, Color::White] {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(filler)).collect();
        state.players[seat].library = library;
    }
    state
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
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
        entered_turn: 0,
        ..Default::default()
    });
    id
}

/// Reanimate a green Gigantosaurus out of the opponent's graveyard.
fn reanimate(db: &CardDatabase) -> (GameState, PermanentId) {
    let mut state = main_phase(db);
    let corpse = state.new_instance(cid(db, "gigantosaurus"));
    state.players[1].graveyard.push(corpse);
    let spell = to_hand(&mut state, db, "rise_from_the_grave", PlayerId(0));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: vec![Target::Card(corpse.id)],
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let made = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == corpse.id)
        .expect("the creature is on the battlefield")
        .id;
    (state, made)
}

/// It comes back **under your control**, as a black Zombie that is still what it was.
#[test]
fn rise_from_the_grave_reanimates_it_as_a_black_zombie() {
    let db = db();
    let (state, made) = reanimate(&db);

    let perm = state
        .battlefield
        .iter()
        .find(|perm| perm.id == made)
        .expect("still there");
    assert_eq!(perm.controller, PlayerId(0), "under your control");

    let stats = characteristics(&state, made, &db);
    assert!(stats.colors.contains(&Color::Black), "black");
    assert!(
        stats.colors.contains(&Color::Green),
        "and still green — in addition to its other colors"
    );
    assert!(stats.subtypes.iter().any(|kind| kind == "Zombie"));
    assert!(
        stats.subtypes.iter().any(|kind| kind == "Dinosaur"),
        "and still a Dinosaur"
    );
    assert!(stats.types.contains(&CardType::Creature));
    assert_eq!(stats.power, Some(10), "with the body it always had");
}

/// The colour is a real colour: a creature that can't be blocked by black creatures is
/// not blocked by this one.
#[test]
fn a_reanimated_creature_is_black_to_an_evasion_restriction() {
    let db = db();
    let (mut state, made) = reanimate(&db);
    // Vine Mare can't be blocked by black creatures.
    let mare = place(&mut state, &db, "vine_mare", PlayerId(1));
    state.active_player = PlayerId(1);
    state.priority = PlayerId(1);
    state.players[1].turn_began = state.turn;
    state.step = Step::DeclareAttackers;
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![sage_engine::Attack {
                attacker: mare,
                defender: sage_engine::AttackTarget::Player(PlayerId(0)),
            }],
        },
        &db,
    );
    let mut state = state;
    state.priority = PlayerId(0);
    let blocked = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![sage_engine::Block {
                blocker: made,
                attacker: mare,
            }],
        },
        &db,
    );

    assert!(
        !blocked
            .battlefield
            .iter()
            .any(|perm| perm.id == made && !perm.blocking.is_empty()),
        "a creature the spell made black cannot block the Mare"
    );
}

/// And it stops being colourless: a spell that may only aim at a colourless creature no
/// longer may.
#[test]
fn a_black_creature_is_no_longer_a_colourless_one() {
    let db = db();
    let mut state = main_phase(&db);
    // A colourless creature to start with — a token with no colours at all.
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: sage_engine::CardInstanceId(9_100),
        printed: sage_engine::Printed::Token(Box::new(sage_engine::TokenData {
            name: "Construct".to_string(),
            types: vec![CardType::Creature],
            subtypes: vec!["Construct".to_string()],
            colors: Vec::new(),
            power: Some(2),
            toughness: Some(2),
            ..Default::default()
        })),
        controller: PlayerId(0),
        entered_turn: 0,
        ..Default::default()
    });
    assert!(characteristics(&state, id, &db).colors.is_empty());

    state.static_effects.push(sage_engine::StaticEffect {
        source: id.0,
        affects: sage_engine::EffectAffects::SpecificPermanent(id),
        modification: sage_engine::Modification::AddTypes {
            types: Vec::new(),
            subtypes: Vec::new(),
            colors: vec![Color::Black],
        },
        duration: sage_engine::Duration::WhileOnBattlefield,
    });

    assert_eq!(
        characteristics(&state, id, &db).colors,
        vec![Color::Black],
        "the layer gave it a colour it did not print"
    );
}
