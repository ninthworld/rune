//! Phylactery Lich (issue #706): a creature that cannot be destroyed and dies anyway.
//!
//! Two seams, and the interesting thing is how tightly they are coupled. `As this
//! creature enters, put a phylactery counter on an artifact you control` happens **as it
//! enters** rather than afterwards, and `when you control no permanents with phylactery
//! counters on them, sacrifice this creature` is a state trigger (CR 603.8) that watches
//! the board rather than an event.
//!
//! Written as an enters-the-battlefield *trigger* instead, the card would kill itself
//! every time: the Lich would be on the battlefield with no counter anywhere while the
//! trigger sat on the stack, and the state trigger would see exactly the board it is
//! looking for. The counter has to be placed before the Lich is there for anything to
//! read — which is what "as" means, and why the arrival waits on the question.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, Action, CardDatabase, CardId, Color, CounterKind,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
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

/// Cast the Lich from seat 0's hand and let it resolve.
fn cast_lich(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let card = state.new_instance(cid(db, "phylactery_lich"));
    state.players[0].hand.push(card);
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
    apply_action(&state, &Action::PassPriority, db)
}

/// Answer a pending permanent selection with the first candidate.
fn answer_first_permanent(state: &GameState, db: &CardDatabase) -> GameState {
    let pending = pending_player_choice(state).expect("a choice is owed");
    let request = pending
        .question
        .permanents()
        .expect("the question is about a permanent");
    let candidates = sage_engine::permanent_choice_candidates(state, request, db);
    let chosen = candidates.first().copied().into_iter().collect();
    apply_action(state, &Action::AnswerPermanents { chosen }, db)
}

/// Whether a Lich is on the battlefield.
fn lich(state: &GameState, db: &CardDatabase) -> Option<PermanentId> {
    let card = cid(db, "phylactery_lich");
    state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(card))
        .map(|perm| perm.id)
}

/// Let the stack empty, answering nothing.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..10 {
        if state.stack.is_empty() && pending_player_choice(&state).is_none() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// **The crux.** The counter goes on the artifact *as* the Lich enters — the arrival waits
/// on the question, so there is no moment at which the Lich is on the battlefield with no
/// counter anywhere for its own ability to notice.
#[test]
fn issue_706_the_counter_is_placed_as_it_enters() {
    let db = db();
    let mut state = main_phase(&db);
    let bauble = place(&mut state, &db, "gargoyle_sentinel", PlayerId(0));

    let state = cast_lich(&state, &db);
    assert!(
        lich(&state, &db).is_none(),
        "the Lich waits off the battlefield while the question is owed"
    );
    let state = answer_first_permanent(&state, &db);
    let state = settle(&state, &db);

    let lich = lich(&state, &db).expect("it arrived");
    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == bauble)
            .map(|perm| perm.counters.get(&CounterKind::Phylactery).copied())
            .unwrap_or_default(),
        Some(1),
        "the artifact carries the counter"
    );
    assert!(
        state.battlefield.iter().any(|perm| perm.id == lich),
        "and the Lich is still there"
    );
}

/// A controller with no artifact is asked nothing — and the state trigger sees exactly the
/// board it watches for, so the indestructible 5/5 sacrifices itself the moment it lands
/// (CR 603.8, and CR 701.17: a sacrifice is not a destruction).
#[test]
fn issue_706_a_lich_with_no_artifact_sacrifices_itself_on_arrival() {
    let db = db();
    let state = main_phase(&db);

    let state = cast_lich(&state, &db);
    assert!(
        pending_player_choice(&state).is_none(),
        "there was nothing to put a counter on, so nothing was asked"
    );
    let state = settle(&state, &db);

    assert!(
        lich(&state, &db).is_none(),
        "indestructible is no help against a sacrifice"
    );
    assert_eq!(state.players[0].graveyard.len(), 1);
}

/// And the state trigger fires **later** too: destroy the artifact holding the counter and
/// the Lich goes with it, on the transition the condition became true.
#[test]
fn issue_706_losing_the_last_counter_takes_the_lich_with_it() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "gargoyle_sentinel", PlayerId(0));
    let state = cast_lich(&state, &db);
    let state = answer_first_permanent(&state, &db);
    let state = settle(&state, &db);
    assert!(
        lich(&state, &db).is_some(),
        "the Lich is on the battlefield"
    );

    // The artifact goes; nothing on the board carries a phylactery counter any more.
    let mut state = state;
    let bauble = state
        .battlefield
        .iter()
        .find(|perm| perm.counters.contains_key(&CounterKind::Phylactery))
        .map(|perm| perm.id)
        .expect("the artifact with the counter");
    let naturalize = state.new_instance(cid(&db, "naturalize"));
    state.players[0].hand.push(naturalize);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: naturalize,
            mode: None,
            x: None,
            targets: vec![sage_engine::Target::Permanent(bauble)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = settle(&state, &db);

    assert!(
        lich(&state, &db).is_none(),
        "the condition became true, and the trigger sacrificed it"
    );
}
