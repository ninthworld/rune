//! Two cards whose **second sentence is about a choice the first one made** (issue #706).
//!
//! `Radiating Lightning deals 3 damage to target player **and 1 damage to each creature
//! that player controls**.` `Destroy target creature. **Its controller** creates a 2/4
//! white Ox creature token.`
//!
//! Neither phrase could be a second target slot: that would be a second choice, and the
//! card asks for one. So the resolution records who its last targeted effect named, and
//! the sentence after it reads that — the same road `entered` and `sacrificed` already
//! take.
//!
//! The Wand is why it is recorded **before** the effect is applied rather than after: a
//! creature about to be destroyed has a controller now and none afterwards (CR 608.2h).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, Action, CardDatabase, CardId, CardInstance, Color, CounterKind, FunctionalId,
    GameState, Permanent, PermanentId, PlayerId, Step, Target,
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
        for color in [Color::Red, Color::White] {
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

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

fn damage(state: &GameState, id: PermanentId) -> u32 {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .map_or(0, |perm| perm.damage)
}

/// The board it hits is **that player's**, not everyone's.
#[test]
fn radiating_lightning_hits_the_player_and_their_creatures() {
    let db = db();
    let mut state = main_phase(&db);
    let theirs = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let mine = place(&mut state, &db, "gigantosaurus", PlayerId(0));
    let spell = to_hand(&mut state, &db, "radiating_lightning", PlayerId(0));
    let life = state.players[1].life;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(state.players[1].life, life - 3, "three to the player");
    assert_eq!(damage(&state, theirs), 1, "one to their creature");
    assert_eq!(damage(&state, mine), 0, "and none to yours");
}

/// The Wand destroys a creature and pays **its controller** an Ox — recorded before the
/// creature dies, because afterwards it has no controller to ask about.
#[test]
fn the_wand_pays_the_dead_creatures_controller() {
    let db = db();
    let mut state = main_phase(&db);
    let wand = place(&mut state, &db, "transmogrifying_wand", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == wand) {
        perm.counters.insert(CounterKind::Charge, 3);
    }
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: wand,
            index: 1,
            targets: vec![Target::Permanent(victim)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == victim),
        "the creature was destroyed"
    );
    let ox = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.is_token())
        .expect("an Ox was made");
    assert_eq!(
        ox.controller,
        PlayerId(1),
        "under the dead creature's controller, not the Wand's"
    );
}

/// It enters with three charge counters, and each activation spends one.
#[test]
fn the_wand_enters_charged_and_spends_one_at_a_time() {
    let db = db();
    let mut state = main_phase(&db);
    let card = to_hand(&mut state, &db, "transmogrifying_wand", PlayerId(0));
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let wand = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == card.id)
        .expect("the Wand arrived");
    assert_eq!(
        wand.counter_count(CounterKind::Charge),
        3,
        "it enters with three"
    );
    let wand = wand.id;

    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: wand,
            index: 1,
            targets: vec![Target::Permanent(victim)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == wand)
            .expect("still there")
            .counter_count(CounterKind::Charge),
        2,
        "and one was spent"
    );
}
