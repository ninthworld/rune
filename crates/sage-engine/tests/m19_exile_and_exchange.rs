//! Two cards about **control and where a permanent is**: an exile that comes back, and an
//! exchange (CR 610.3, CR 701.10, issue #706).
//!
//! Hieromancer's Cage is the engine's first **linked** ability. What was exiled and what
//! exiled it are recorded together, because "until this leaves the battlefield" is a
//! sentence about one particular card and no snapshot of the exile zone could say which
//! exile a creature is waiting on. Two Cages are two links, and one of them dying returns
//! exactly one creature.
//!
//! Switcheroo is all-or-nothing (CR 701.10c): if either creature is an illegal target, or
//! the two are already controlled by the same player, no control changes at all.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, controller_of_id, pending_trigger_target_choice, Action, CardDatabase, CardId,
    CardInstance, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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
        for color in [Color::White, Color::Blue, Color::Green] {
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

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// Cast the Cage and aim its enters trigger at `victim`.
fn cage(state: &GameState, db: &CardDatabase, victim: PermanentId) -> (GameState, PermanentId) {
    let mut state = state.clone();
    let card = to_hand(&mut state, db, "hieromancer_s_cage", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let ability = pending_trigger_target_choice(&state).expect("the enters trigger owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(victim)],
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let made = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == card.id)
        .expect("the Cage is on the battlefield")
        .id;
    (state, made)
}

/// It exiles what it caught, and the exiled card is linked to it.
#[test]
fn the_cage_exiles_what_it_caught() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let (state, _) = cage(&state, &db, victim);

    assert!(!on_battlefield(&state, victim), "it is gone");
    assert_eq!(state.players[1].exile.len(), 1, "to its owner's exile");
    assert_eq!(state.exiled_until.len(), 1, "and the pair is recorded");
}

/// The Cage leaving returns it — under its **owner's** control, not the Cage
/// controller's.
#[test]
fn the_cage_leaving_returns_it_to_its_owner() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let (mut state, cage_id) = cage(&state, &db, victim);

    state.battlefield.retain(|perm| perm.id != cage_id);
    // The return is settled by the state-based-action pass, like every other consequence
    // of a permanent no longer being there.
    let state = apply_action(&state, &Action::PassPriority, &db);

    let returned = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "onakke_ogre")))
        .expect("it came back");
    assert_eq!(
        controller_of_id(&state, returned.id),
        Some(PlayerId(1)),
        "CR 610.3b: under its owner's control, not the seat that caged it"
    );
    assert!(state.exiled_until.is_empty(), "and the link is spent");
    assert!(state.players[1].exile.is_empty());
}

/// Two Cages are two links: one dying returns exactly one creature.
#[test]
fn two_cages_are_two_links() {
    let db = db();
    let mut state = main_phase(&db);
    let first = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let second = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let (state, first_cage) = cage(&state, &db, first);
    let (mut state, _) = cage(&state, &db, second);
    assert_eq!(state.exiled_until.len(), 2);

    state.battlefield.retain(|perm| perm.id != first_cage);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.printed.card() == Some(cid(&db, "onakke_ogre"))),
        "the one that Cage took is back"
    );
    assert!(
        !state
            .battlefield
            .iter()
            .any(|perm| perm.printed.card() == Some(cid(&db, "gigantosaurus"))),
        "and the other is still caged"
    );
    assert_eq!(state.exiled_until.len(), 1, "one link left");
}

/// Switcheroo trades the two creatures it names.
#[test]
fn switcheroo_exchanges_control() {
    let db = db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let theirs = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let spell = to_hand(&mut state, &db, "switcheroo", PlayerId(0));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(controller_of_id(&state, mine), Some(PlayerId(1)));
    assert_eq!(controller_of_id(&state, theirs), Some(PlayerId(0)));
}

/// CR 701.10c: two creatures one player already controls exchange nothing.
#[test]
fn switcheroo_does_nothing_between_two_of_your_own() {
    let db = db();
    let mut state = main_phase(&db);
    let one = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let two = place(&mut state, &db, "gigantosaurus", PlayerId(0));
    let spell = to_hand(&mut state, &db, "switcheroo", PlayerId(0));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(one), Target::Permanent(two)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(controller_of_id(&state, one), Some(PlayerId(0)));
    assert_eq!(controller_of_id(&state, two), Some(PlayerId(0)));
    assert!(
        state.static_effects.is_empty(),
        "no control change was created at all"
    );
}
