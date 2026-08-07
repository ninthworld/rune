//! Hungering Hydra (issue #706): a 0/0 whose size comes entirely from two things the
//! board keeps no record of — **the X its spell announced** and **the damage it has been
//! dealt**.
//!
//! Both are facts about an event rather than about the permanent, and both are gone by
//! the time anything could look them up: an announced X lives on the stack object that
//! stops existing when the spell resolves, and marked damage is marked damage however it
//! arrived. So each is read where it still exists — the X at the entry seam, the amount
//! where the trigger is collected — and written into the thing that asks for it.
//!
//! The corollary is the interesting half: a Hydra that enters **without being cast** has
//! no X to read, enters as a 0/0, and dies to the state-based actions. That is the rule
//! (CR 107.3b), not a gap.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, Action, CardDatabase, CardId, Color, FunctionalId, GameState,
    Permanent, PermanentId, PlayerId, Step, Target,
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
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
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
        ..Default::default()
    });
    id
}

/// Cast `slug` from seat 0's hand for `x`, then let both players pass so it resolves.
fn cast_for_x(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    x: Option<u32>,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// The Hydra on the battlefield, whichever id it was minted with.
fn hydra(state: &GameState, db: &CardDatabase) -> Option<PermanentId> {
    let card = cid(db, "hungering_hydra");
    state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(card))
        .map(|perm| perm.id)
}

/// The current power/toughness of `perm`, read through the layer system.
fn pt(state: &GameState, perm: PermanentId, db: &CardDatabase) -> (Option<i32>, Option<i32>) {
    let current = characteristics(state, perm, db);
    (current.power, current.toughness)
}

/// The damage marked on `perm`, or zero once it is gone.
fn marked(state: &GameState, perm: PermanentId) -> u32 {
    state
        .battlefield
        .iter()
        .find(|p| p.id == perm)
        .map_or(0, |p| p.damage)
}

/// **The crux.** X is announced at cast (CR 601.2b) and read at the entry seam, so the
/// 0/0 arrives already three counters large — never as a 0/0 the state-based actions
/// could bury first (CR 614.12).
#[test]
fn issue_706_a_hydra_cast_for_three_enters_as_a_three_three() {
    let db = db();
    let state = main_phase(&db);
    let state = cast_for_x(&state, &db, "hungering_hydra", Some(3), Vec::new());

    let hydra = hydra(&state, &db).expect("the Hydra survived entering");
    assert_eq!(
        pt(&state, hydra, &db),
        (Some(3), Some(3)),
        "a 0/0 with three +1/+1 counters"
    );
}

/// X = 0 is a legal announcement, and the 0/0 that results dies to CR 704.5f — the
/// counters are the only thing that was holding it up.
#[test]
fn issue_706_a_hydra_cast_for_zero_dies_where_it_stands() {
    let db = db();
    let state = main_phase(&db);
    let state = cast_for_x(&state, &db, "hungering_hydra", Some(0), Vec::new());

    assert!(
        hydra(&state, &db).is_none(),
        "no counters, no toughness, no permanent"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        1,
        "it went to a graveyard"
    );
}

/// The mirror: a Hydra put onto the battlefield **without being cast** announced no X and
/// enters as the 0/0 it is printed as. The entry seam has nothing to read, and that is
/// the same absence rather than a second rule.
#[test]
fn issue_706_a_hydra_that_was_never_cast_enters_as_a_zero_zero() {
    let db = db();
    let mut state = main_phase(&db);
    let hydra = place(&mut state, &db, "hungering_hydra", PlayerId(0));

    assert_eq!(
        pt(&state, hydra, &db),
        (Some(0), Some(0)),
        "nothing announced anything, so nothing was added"
    );
}

/// **The second crux.** Two damage from a burn spell puts two counters on it — "that
/// many" is the amount the event dealt, measured where the trigger is collected. The
/// damage stays marked: the counters raise toughness, they do not heal anything.
#[test]
fn issue_706_damage_dealt_to_the_hydra_becomes_that_many_counters() {
    let db = db();
    let state = main_phase(&db);
    let state = cast_for_x(&state, &db, "hungering_hydra", Some(3), Vec::new());
    let hydra = hydra(&state, &db).expect("the Hydra is on the battlefield");

    let state = cast_for_x(&state, &db, "shock", None, vec![Target::Permanent(hydra)]);
    // The trigger the damage put on the stack still has to resolve.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(
        pt(&state, hydra, &db),
        (Some(5), Some(5)),
        "three from X, two from the damage it survived"
    );
    assert_eq!(marked(&state, hydra), 2, "and the damage is still marked");
}

/// Lethal damage kills it before the trigger can resolve: the state-based actions run
/// first (CR 704.3), and a 3/3 dealt three damage is dead however many counters its
/// ability was about to put on it. The reminder text's "it must survive the damage" is
/// this, and nothing here says it.
#[test]
fn issue_706_a_hydra_dealt_lethal_damage_does_not_grow_out_of_it() {
    let db = db();
    let state = main_phase(&db);
    let state = cast_for_x(&state, &db, "hungering_hydra", Some(3), Vec::new());
    let hydra = hydra(&state, &db).expect("the Hydra is on the battlefield");

    let state = cast_for_x(
        &state,
        &db,
        "lightning_strike",
        None,
        vec![Target::Permanent(hydra)],
    );

    assert!(
        hydra_is_gone(&state, hydra),
        "three damage on a 3/3 is lethal before anything resolves"
    );
}

/// Whether the Hydra has left the battlefield.
fn hydra_is_gone(state: &GameState, hydra: PermanentId) -> bool {
    !state.battlefield.iter().any(|perm| perm.id == hydra)
}
