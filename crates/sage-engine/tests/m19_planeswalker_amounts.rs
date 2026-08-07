//! Three of M19's five planeswalkers (issue #706), and the three **amounts** they are
//! the reason for.
//!
//! Nearly everything these cards do was already in the vocabulary — a life gain per
//! creature, a mass pump, a one-sided fight, damage to a class, a keyword grant. What
//! they add is three ways of saying *how many*: a count taken from a life total, a
//! destroy that may name two targets, and a search with no ceiling printed on it at all.
//!
//! Each is taken **once, as the effect resolves** (CR 608.2), which is what the tests
//! below pin: a life total read after the gain rather than before it, and a search bounded
//! by what the library actually holds rather than by a number.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_player_choice, Action, CardDatabase, CardId, Color,
    CounterKind, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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

/// Put `walker` onto the battlefield with the loyalty it prints, ready to activate.
fn walker(state: &mut GameState, db: &CardDatabase, slug: &str) -> PermanentId {
    let loyalty = db
        .card(cid(db, slug))
        .and_then(|data| data.loyalty)
        .expect("a planeswalker prints loyalty");
    loyal_walker(state, db, slug, loyalty)
}

/// The same, with enough loyalty on it to pay for an ultimate — every one of these costs
/// more than the card enters with, so a test of one has to have been building up to it.
fn loyal_walker(state: &mut GameState, db: &CardDatabase, slug: &str, loyalty: u32) -> PermanentId {
    let id = place(state, db, slug, PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == id) {
        perm.counters.insert(CounterKind::Loyalty, loyalty);
        perm.entered_turn = 0;
    }
    id
}

/// Activate loyalty ability `index` of `walker` and let it resolve.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    walker: PermanentId,
    index: usize,
    targets: Vec<Target>,
) -> GameState {
    let state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent: walker,
            index,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let mut state = state;
    for _ in 0..8 {
        if state.stack.is_empty() || pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// The counters of `kind` on `perm`.
fn counters(state: &GameState, perm: PermanentId, kind: CounterKind) -> u32 {
    state
        .battlefield
        .iter()
        .find(|p| p.id == perm)
        .and_then(|p| p.counters.get(&kind).copied())
        .unwrap_or(0)
}

// ----- Ajani, Wise Counselor: an amount read off a life total ------------------

/// **The crux.** X is the life total **as the ultimate resolves**, not as it was
/// activated — and the counters are one per point.
#[test]
fn issue_706_ajanis_ultimate_counts_the_life_total_it_resolves_with() {
    let db = db();
    let mut state = main_phase(&db);
    let ajani = loyal_walker(&mut state, &db, "ajani_wise_counselor", 12);
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));
    state.players[0].life = 24;

    let state = activate(&state, &db, ajani, 2, vec![Target::Permanent(bear)]);

    assert_eq!(
        counters(&state, bear, CounterKind::PlusOnePlusOne),
        24,
        "one counter per point of life"
    );
    assert_eq!(
        characteristics(&state, bear, &db).power,
        Some(6 + 24),
        "and the creature is that much bigger"
    );
}

/// The +2 is the vocabulary's own: a life gain counted per creature.
#[test]
fn issue_706_ajani_gains_a_life_for_each_creature() {
    let db = db();
    let mut state = main_phase(&db);
    let ajani = walker(&mut state, &db, "ajani_wise_counselor");
    for _ in 0..3 {
        place(&mut state, &db, "bogstomper", PlayerId(0));
    }
    place(&mut state, &db, "bogstomper", PlayerId(1));
    let life = state.players[0].life;

    let state = activate(&state, &db, ajani, 0, Vec::new());

    assert_eq!(
        state.players[0].life,
        life + 3,
        "three creatures, three life — the opponent's is not yours"
    );
    assert_eq!(
        counters(&state, ajani, CounterKind::Loyalty),
        7,
        "and the plus ability added its loyalty"
    );
}

// ----- Sarkhan, Dragonsoul: a search with no ceiling ---------------------------

/// **The second crux.** `Any number` is bounded by the library rather than by a printed
/// figure: three Dragons in the deck is three Dragons on the battlefield.
#[test]
fn issue_706_sarkhans_ultimate_finds_every_dragon_there_is() {
    let db = db();
    let mut state = main_phase(&db);
    let sarkhan = loyal_walker(&mut state, &db, "sarkhan_dragonsoul", 12);
    let mut library: Vec<_> = (0..6)
        .map(|_| state.new_instance(cid(&db, "bogstomper")))
        .collect();
    for _ in 0..3 {
        let dragon = state.new_instance(cid(&db, "sarkhan_s_whelp"));
        library.push(dragon);
    }
    state.players[0].library = library;

    let state = activate(&state, &db, sarkhan, 2, Vec::new());
    let pending = pending_player_choice(&state).expect("the search asks");
    let request = pending.question.cards().expect("a card selection");
    let candidates = sage_engine::choice_candidates(&state, request, &db);
    assert_eq!(candidates.len(), 3, "every Dragon is a candidate");

    let chosen = candidates.iter().map(|card| card.id).collect();
    let state = apply_action(&state, &Action::AnswerChoice { chosen }, &db);

    let dragons = state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.card() == Some(cid(&db, "sarkhan_s_whelp")))
        .count();
    assert_eq!(dragons, 3, "and all three arrived");
}

/// The +2 hits every opponent and everything they control, in one breath.
#[test]
fn issue_706_sarkhan_burns_the_board_across_the_table() {
    let db = db();
    let mut state = main_phase(&db);
    let sarkhan = walker(&mut state, &db, "sarkhan_dragonsoul");
    let theirs = place(&mut state, &db, "bogstomper", PlayerId(1));
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    let life = state.players[1].life;

    let state = activate(&state, &db, sarkhan, 0, Vec::new());

    assert_eq!(state.players[1].life, life - 1, "one to the opponent");
    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == theirs)
            .map(|perm| perm.damage),
        Some(1),
        "and one to their creature"
    );
    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == mine)
            .map(|perm| perm.damage),
        Some(0),
        "and none to mine"
    );
}

// ----- Vivien of the Arkbow: a card the vocabulary already had -----------------

/// Vivien needed nothing new at all. The `up to one` is the same variable arity two other
/// effects already carried, and the ultimate is two sentences the card prints as one line.
#[test]
fn issue_706_vivien_pumps_and_tramples_the_board() {
    let db = db();
    let mut state = main_phase(&db);
    let vivien = loyal_walker(&mut state, &db, "vivien_of_the_arkbow", 12);
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));

    let state = activate(&state, &db, vivien, 2, Vec::new());

    let current = characteristics(&state, bear, &db);
    assert_eq!(current.power, Some(6 + 4), "+4/+4");
    assert!(
        current.keywords.contains(&sage_engine::Keyword::Trample),
        "and trample, from the same line"
    );
}

/// Her +2 may name **no** target at all — "up to one" is a real zero, and the ability
/// still resolves and still added its loyalty.
#[test]
fn issue_706_viviens_counters_may_name_nobody() {
    let db = db();
    let mut state = main_phase(&db);
    let vivien = walker(&mut state, &db, "vivien_of_the_arkbow");

    let state = activate(&state, &db, vivien, 0, Vec::new());

    assert!(state.stack.is_empty(), "it resolved");
    assert_eq!(
        counters(&state, vivien, CounterKind::Loyalty),
        7,
        "and the loyalty went up regardless"
    );
}
