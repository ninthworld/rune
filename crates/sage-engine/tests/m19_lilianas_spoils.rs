//! Liliana's Spoils: a targeted discard, and then a look at the top five (issue #773).
//!
//! Reported as *the top-five choice is never presented*. The card is the smallest one in the
//! catalog whose spell ability composes **two** effects where the second poses a question, so
//! what is under test is that seam rather than the card: a resolution that acts, then suspends,
//! then resumes with the rest of its work intact.
//!
//! Every test drives the real [`apply_action`] pipeline.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, choice_candidates, pending_player_choice, Action, CardDatabase, CardId,
    CardInstanceId, Color, FunctionalId, GameState, PlayerId, Step,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main with both pools stocked.
fn main_phase(seed: u64) -> GameState {
    let mut state = GameState::new_two_player_with_seed(seed);
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
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
    state
}

/// Give `seat` a library of exactly these cards, **listed top first**.
fn library_of(state: &mut GameState, db: &CardDatabase, seat: PlayerId, slugs: &[&str]) {
    let instances: Vec<_> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[seat.0].library = instances.into_iter().rev().collect();
}

fn hand_of(state: &mut GameState, db: &CardDatabase, seat: PlayerId, slugs: &[&str]) {
    for slug in slugs {
        let instance = state.new_instance(cid(db, slug));
        state.players[seat.0].hand.push(instance);
    }
}

fn names(db: &CardDatabase, cards: &[sage_engine::CardInstance]) -> Vec<String> {
    cards
        .iter()
        .map(|inst| db.card(inst.card).expect("a bundled card").name.clone())
        .collect()
}

fn hand_names(state: &GameState, db: &CardDatabase, seat: PlayerId) -> Vec<String> {
    names(db, &state.players[seat.0].hand)
}

/// The card names in `seat`'s library, **top first**.
fn library_names(state: &GameState, db: &CardDatabase, seat: PlayerId) -> Vec<String> {
    state.players[seat.0]
        .library
        .iter()
        .rev()
        .map(|inst| db.card(inst.card).expect("a bundled card").name.clone())
        .collect()
}

/// Answer whatever card selection is pending with the first `n` candidates.
fn answer_first(state: &GameState, db: &CardDatabase, n: usize) -> GameState {
    let pending = pending_player_choice(state).expect("a choice is owed");
    let request = pending.question.cards().expect("a card selection");
    let candidates = choice_candidates(state, request, db);
    let chosen: Vec<CardInstanceId> = candidates.iter().take(n).map(|inst| inst.id).collect();
    let after = apply_action(state, &Action::AnswerChoice { chosen }, db);
    assert_ne!(after, *state, "the answer was rejected as illegal");
    after
}

/// Cast Liliana's Spoils at the opponent and let it resolve as far as it gets.
fn cast_spoils(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, "liliana_s_spoils"));
    state.players[0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: vec![sage_engine::Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        db,
    );
    assert!(
        !state.stack.is_empty(),
        "Liliana's Spoils did not reach the stack"
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// A board where the opponent holds one card and the caster's top five hold two black cards.
fn board(seed: u64) -> (CardDatabase, GameState) {
    let db = db();
    let mut state = main_phase(seed);
    hand_of(&mut state, &db, PlayerId(1), &["shock"]);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &[
            // Top five: two black cards among three that are not.
            "walking_corpse",
            "shock",
            "mountain",
            "child_of_night",
            "forest",
            // The sixth is out of reach and must stay where it is.
            "onakke_ogre",
        ],
    );
    (db, state)
}

/// The order the two questions come in, which is where the report starts.
///
/// The card is *"Target opponent discards a card. Look at the top five…"*, and both halves ask
/// somebody something. So resolving suspends **twice**: first on the opponent's discard, whose
/// chooser is the opponent, and only then on the controller's look. The look is not skipped
/// while the first question stands — it is riding in the resume, and a game that never answers
/// the discard never reaches it.
#[test]
fn issue_773_the_discard_is_asked_first_and_the_look_is_still_owed() {
    let (db, state) = board(0x1111);
    let after = cast_spoils(&state, &db);

    let first = pending_player_choice(&after).expect("the discard is owed");
    assert_eq!(
        first.chooser,
        PlayerId(1),
        "the discard is the opponent's to answer"
    );
    // Nothing has been discarded yet: the question is the discard, not its consequence.
    assert_eq!(state.players[1].hand.len(), after.players[1].hand.len());
}

/// The reported bug, in one assertion: once the discard is answered, the look must be posed.
#[test]
fn issue_773_resolving_presents_the_top_five_choice() {
    let (db, state) = board(0x1111);
    let after = answer_first(&cast_spoils(&state, &db), &db, 1);

    // The discard half happened — the opponent's only card is gone.
    assert!(
        state.players[1].hand.len() > after.players[1].hand.len(),
        "the targeted discard did not happen"
    );

    let pending = pending_player_choice(&after).expect("the top-five choice is owed");
    assert_eq!(
        pending.chooser,
        PlayerId(0),
        "the look belongs to the spell's controller"
    );
    let request = pending.question.cards().expect("a card selection");
    let candidates = choice_candidates(&after, request, &db);

    // Exactly the eligible black cards among the top five — never the whole five, and never
    // the sixth card, which the spell does not see.
    let offered = names(&db, &candidates);
    assert_eq!(offered, vec!["Walking Corpse", "Child of Night"]);
}

/// Taking a card puts it in hand and bottoms the rest; the sixth card never moves.
#[test]
fn issue_773_the_taken_card_reaches_the_hand_and_the_rest_go_to_the_bottom() {
    let (db, state) = board(0xB0770);
    let after = answer_first(&cast_spoils(&state, &db), &db, 1);

    let pending = pending_player_choice(&after).expect("a choice is owed");
    let request = pending.question.cards().expect("a card selection");
    let candidates = choice_candidates(&after, request, &db);
    let taken: Vec<CardInstanceId> = vec![candidates[0].id];
    let done = apply_action(&after, &Action::AnswerChoice { chosen: taken }, &db);
    assert_ne!(done, after, "the answer was rejected");

    assert!(
        hand_names(&done, &db, PlayerId(0)).contains(&"Walking Corpse".to_string()),
        "the chosen card did not reach the hand: {:?}",
        hand_names(&done, &db, PlayerId(0))
    );
    assert!(
        pending_player_choice(&done).is_none(),
        "the resolution did not finish"
    );

    // Six cards were in the library; one is in hand, and the other five are still there —
    // the four looked at now underneath the one the spell never saw.
    let library = library_names(&done, &db, PlayerId(0));
    assert_eq!(library.len(), 5);
    assert_eq!(
        library[0], "Onakke Ogre",
        "the sixth card was disturbed: {library:?}"
    );
    for name in ["Shock", "Mountain", "Child of Night", "Forest"] {
        assert!(
            library[1..].contains(&name.to_string()),
            "{name} did not reach the bottom: {library:?}"
        );
    }
}

/// Taking nothing is a legal answer, and the spell still finishes.
#[test]
fn issue_773_choosing_no_card_is_supported() {
    let (db, state) = board(0x0);
    let after = answer_first(&cast_spoils(&state, &db), &db, 1);
    assert!(pending_player_choice(&after).is_some());

    let done = apply_action(&after, &Action::AnswerChoice { chosen: Vec::new() }, &db);
    assert_ne!(done, after, "declining was rejected");
    assert!(
        pending_player_choice(&done).is_none(),
        "the resolution did not finish"
    );
    assert_eq!(library_names(&done, &db, PlayerId(0)).len(), 6);
}

/// No eligible card among the top five: the spell resolves without stalling.
#[test]
fn issue_773_a_top_five_with_no_black_card_resolves_without_stalling() {
    let db = db();
    let mut state = main_phase(0xE3E3);
    hand_of(&mut state, &db, PlayerId(1), &["shock"]);
    library_of(
        &mut state,
        &db,
        PlayerId(0),
        &["forest", "mountain", "shock", "forest", "mountain"],
    );
    let after = answer_first(&cast_spoils(&state, &db), &db, 1);
    assert!(
        pending_player_choice(&after).is_none(),
        "a look with nothing to take must not suspend"
    );
    assert!(after.stack.is_empty(), "the spell did not finish resolving");
}

/// An empty library resolves without stalling either.
#[test]
fn issue_773_an_empty_library_resolves_without_stalling() {
    let db = db();
    let mut state = main_phase(0xE0E0);
    hand_of(&mut state, &db, PlayerId(1), &["shock"]);
    state.players[0].library.clear();
    let after = answer_first(&cast_spoils(&state, &db), &db, 1);
    assert!(pending_player_choice(&after).is_none());
    assert!(after.stack.is_empty(), "the spell did not finish resolving");
}
