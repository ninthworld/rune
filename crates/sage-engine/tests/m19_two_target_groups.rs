//! Liliana, the Necromancer (issue #706), and the pairing her ultimate is the reason for.
//!
//! `Destroy up to two target creatures. Put up to two creature cards from graveyards onto
//! the battlefield under your control.` is the first ability to declare **two**
//! variable-arity target groups, and the reason that used to be refused: an announcement
//! hands the engine one flat list of targets, and with two groups that both vary there is
//! no arithmetic that says how many belong to each.
//!
//! What settles it is the one fact about a target that never changes — what kind of
//! object it names. A permanent can only be the first group's and a card in a graveyard
//! can only be the second's, so the split is read rather than guessed. Two variable groups
//! that named the *same* kind would still be ambiguous, and the catalog validator still
//! refuses those.
//!
//! Pairing on kind rather than on legality is what makes it survive a resolution: a target
//! that has become illegal must still pair with the group it was announced for, or the
//! CR 608.2b re-check would be asking about the wrong slot.
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

/// Put a Liliana on the battlefield with enough loyalty to ultimate.
fn liliana(state: &mut GameState, db: &CardDatabase) -> PermanentId {
    let id = place(state, db, "liliana_the_necromancer", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == id) {
        perm.counters.insert(CounterKind::Loyalty, 9);
        perm.entered_turn = 0;
    }
    id
}

/// Put `slug` into seat `seat`'s graveyard.
fn to_graveyard(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// Activate ability `index` with `targets` and let it resolve.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    walker: PermanentId,
    index: usize,
    targets: Vec<Target>,
) -> GameState {
    let mut state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent: walker,
            index,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    for _ in 0..8 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// How many permanents on the battlefield were printed as `slug`.
fn count_of(state: &GameState, db: &CardDatabase, slug: &str) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.card() == Some(cid(db, slug)))
        .count()
}

/// **The crux.** Two of each: two creatures destroyed and two cards reanimated, from one
/// flat announcement, split by what each target is.
#[test]
fn issue_706_lilianas_ultimate_fills_two_variable_groups_at_once() {
    let db = db();
    let mut state = main_phase(&db);
    let liliana = liliana(&mut state, &db);
    let first = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let second = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    // One card in each graveyard, so the "from graveyards" scope is doing work too.
    let mine = to_graveyard(&mut state, &db, "bogstomper", PlayerId(0));
    let theirs = to_graveyard(&mut state, &db, "bogstomper", PlayerId(1));

    let state = activate(
        &state,
        &db,
        liliana,
        2,
        vec![
            Target::Permanent(first),
            Target::Permanent(second),
            Target::Card(mine.id),
            Target::Card(theirs.id),
        ],
    );

    assert_eq!(
        count_of(&state, &db, "onakke_ogre"),
        0,
        "both creatures were destroyed"
    );
    assert_eq!(
        state
            .battlefield
            .iter()
            .filter(|perm| perm.controller == PlayerId(0)
                && perm.printed.card() == Some(cid(&db, "bogstomper")))
            .count(),
        2,
        "and both cards came back under your control"
    );
}

/// The halves are independent: naming one creature and two cards is a legal announcement,
/// and each group gets exactly what it was given.
#[test]
fn issue_706_the_two_groups_take_what_belongs_to_each() {
    let db = db();
    let mut state = main_phase(&db);
    let liliana = liliana(&mut state, &db);
    let doomed = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let spared = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let one = to_graveyard(&mut state, &db, "bogstomper", PlayerId(0));
    let two = to_graveyard(&mut state, &db, "bogstomper", PlayerId(0));

    let state = activate(
        &state,
        &db,
        liliana,
        2,
        vec![
            Target::Permanent(doomed),
            Target::Card(one.id),
            Target::Card(two.id),
        ],
    );

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == doomed),
        "the creature named was destroyed"
    );
    assert!(
        state.battlefield.iter().any(|perm| perm.id == spared),
        "and the one that was not, was not"
    );
    assert_eq!(
        count_of(&state, &db, "bogstomper"),
        2,
        "both named cards arrived"
    );
}

/// And naming **nothing** is legal on both halves at once: an "up to" group filled with
/// none is not a fizzle, because the ability never had a target to lose (CR 608.2b).
#[test]
fn issue_706_an_ultimate_that_names_nobody_still_resolves() {
    let db = db();
    let mut state = main_phase(&db);
    let liliana = liliana(&mut state, &db);
    let bystander = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let state = activate(&state, &db, liliana, 2, Vec::new());

    assert!(state.stack.is_empty(), "it resolved");
    assert!(
        state.battlefield.iter().any(|perm| perm.id == bystander),
        "and did nothing at all"
    );
    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == liliana)
            .and_then(|perm| perm.counters.get(&CounterKind::Loyalty).copied()),
        Some(2),
        "the loyalty was still spent"
    );
}

/// The +1 is the vocabulary's own, and is here because a planeswalker is three abilities
/// rather than one.
#[test]
fn issue_706_lilianas_plus_drains_the_player_it_names() {
    let db = db();
    let mut state = main_phase(&db);
    let liliana = liliana(&mut state, &db);
    let life = state.players[1].life;

    let state = activate(&state, &db, liliana, 0, vec![Target::Player(PlayerId(1))]);

    assert_eq!(state.players[1].life, life - 2, "two life off the target");
}

/// And the −1, in its own turn: CR 606.3 allows one loyalty ability per planeswalker per
/// turn, so a test of the second one is a second test rather than a second line.
#[test]
fn issue_706_lilianas_minus_returns_a_creature_card() {
    let db = db();
    let mut state = main_phase(&db);
    let liliana = liliana(&mut state, &db);
    let card = to_graveyard(&mut state, &db, "bogstomper", PlayerId(0));

    let state = activate(&state, &db, liliana, 1, vec![Target::Card(card.id)]);

    assert!(
        state.players[0].hand.iter().any(|held| held.id == card.id),
        "the creature card came back to hand"
    );
}
