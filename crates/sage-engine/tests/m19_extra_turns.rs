//! Magistrate's Scepter: charge counters bought one at a time, spent three at a time, for
//! an extra turn (CR 720.1, issue #706).
//!
//! Everything the card needs but one thing was already here — charge counters, a cost that
//! removes counters, and a turn rotation that takes queued extra turns before the natural
//! next one. What was missing is the effect that queues one, and the test that matters is
//! the one that walks the rotation: an extra turn is not a state flag, it is whose turn
//! comes next.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, forced_declaration_without_choice, priority_has_no_meaningful_action, Action,
    CardDatabase, CardId, Color, CounterKind, FunctionalId, GameState, Permanent, PermanentId,
    PlayerId, Step,
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
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..30).map(|_| state.new_instance(filler)).collect();
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

fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> GameState {
    let state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

fn counters(state: &GameState, id: PermanentId) -> u32 {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .map_or(0, |perm| perm.counter_count(CounterKind::Charge))
}

/// Walk forward until the active player changes, and answer nothing on the way.
fn to_next_turn(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let turn = state.turn;
    for _ in 0..400 {
        if state.turn != turn {
            return state;
        }
        let action = if priority_has_no_meaningful_action(&state, db) {
            Action::PassPriority
        } else {
            forced_declaration_without_choice(&state, db).unwrap_or_else(|| match state.step {
                Step::DeclareAttackers => Action::DeclareAttackers {
                    attackers: Vec::new(),
                },
                Step::DeclareBlockers => Action::DeclareBlockers { blocks: Vec::new() },
                step => panic!("the walk stalled at {step:?}"),
            })
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the walk ran past its cap");
}

/// Three activations, three counters — the ordinary road, one `{4}` at a time.
#[test]
fn the_scepter_charges_one_counter_at_a_time() {
    let db = db();
    let mut state = main_phase(&db);
    let scepter = place(&mut state, &db, "magistrate_s_scepter", PlayerId(0));

    for expected in 1..=3 {
        state.players[0].mana_pool.add_colorless(4);
        // It taps to charge, so a real game waits for the untap step; the test untaps it
        // rather than walking three turns to make the same point.
        if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == scepter) {
            perm.tapped = false;
        }
        state = activate(&state, &db, scepter, 0);
        assert_eq!(counters(&state, scepter), expected, "one per activation");
    }
}

/// Spending three of them queues the turn, and the rotation hands it back to the same
/// player rather than passing it on.
#[test]
fn spending_three_counters_takes_an_extra_turn() {
    let db = db();
    let mut state = main_phase(&db);
    let scepter = place(&mut state, &db, "magistrate_s_scepter", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == scepter) {
        perm.counters.insert(CounterKind::Charge, 3);
    }

    let state = activate(&state, &db, scepter, 1);
    assert_eq!(counters(&state, scepter), 0, "all three were spent");

    let next = to_next_turn(&state, &db);
    assert_eq!(
        next.active_player,
        PlayerId(0),
        "the extra turn is taken before the natural next one (CR 720.1)"
    );
    // And the turn after *that* is the opponent's — one extra turn, not a lock.
    let after = to_next_turn(&next, &db);
    assert_eq!(after.active_player, PlayerId(1));
}

/// Without three counters there is nothing to spend, so the ability is not offered.
#[test]
fn the_scepter_offers_nothing_without_its_counters() {
    let db = db();
    let mut state = main_phase(&db);
    let scepter = place(&mut state, &db, "magistrate_s_scepter", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == scepter) {
        perm.counters.insert(CounterKind::Charge, 2);
    }
    state.players[0].mana_pool.add(Color::White, 0);

    let offered = sage_engine::valid_actions(&state, &db)
        .into_iter()
        .filter(|action| {
            matches!(action, Action::ActivateAbility { permanent, index, .. }
                if *permanent == scepter && *index == 1)
        })
        .count();

    assert_eq!(offered, 0, "two counters do not pay a cost of three");
}
