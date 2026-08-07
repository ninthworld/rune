//! Demanding Dragon (issue #706): the first card whose **opponent** is asked the
//! question.
//!
//! `deals 5 damage to target opponent unless that player sacrifices a creature of their
//! choice` is an ordinary `unless` offer in every respect but who answers it — and who
//! answers a question was already a property of the choice queue rather than of the
//! effect, so the whole of the difference is which seat the offer is posed to.
//!
//! The second half is where the target lives. Accepting does nothing at all, so the
//! sentence that aims is the **consequence**, and the announcement's target belongs to it.
//! The branch not taken drops the targets either way, which is the same rule read from the
//! other side.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, Action, CardDatabase, CardId, CardInstance, Color,
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

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast the Dragon from seat 0 and let it resolve, aiming its enters trigger at seat 1.
fn dragon_enters(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let dragon = to_hand(&mut state, db, "demanding_dragon", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: dragon,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    // The enters trigger arrives unaimed and its controller points it at an opponent
    // (CR 603.3d).
    let ability = sage_engine::pending_trigger_target_choice(&state).expect("owed an opponent");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Player(PlayerId(1))],
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// How many permanents seat `seat` controls.
fn permanents(state: &GameState, seat: PlayerId) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.controller == seat)
        .count()
}

/// Answer a pending permanent selection with the first candidate.
fn answer_first_permanent(state: &GameState, db: &CardDatabase) -> GameState {
    let pending = pending_player_choice(state).expect("a choice is owed");
    let request = pending
        .question
        .permanents()
        .expect("the sacrifice asks which permanent");
    let candidates = sage_engine::permanent_choice_candidates(state, request, db);
    let chosen = candidates.first().copied().into_iter().collect();
    apply_action(state, &Action::AnswerPermanents { chosen }, db)
}

/// **The crux.** The offer is posed to the *targeted opponent*, not to the Dragon's
/// controller — and it is their creature that goes.
#[test]
fn issue_706_the_targeted_opponent_is_the_one_asked() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let life = state.players[1].life;

    let state = dragon_enters(&state, &db);

    let pending = pending_player_choice(&state).expect("somebody was asked");
    assert_eq!(pending.chooser, PlayerId(1), "the opponent answers it");

    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    let state = answer_first_permanent(&state, &db);

    assert_eq!(
        permanents(&state, PlayerId(1)),
        0,
        "they sacrificed the creature"
    );
    assert_eq!(state.players[1].life, life, "and took no damage for it");
}

/// Declining takes the five, and the damage lands on the player the trigger named — the
/// target the *consequence* was announced for.
#[test]
fn issue_706_declining_takes_the_damage() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let life = state.players[1].life;
    let mine = state.players[0].life;

    let state = dragon_enters(&state, &db);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);

    assert_eq!(state.players[1].life, life - 5, "five to that player");
    assert_eq!(state.players[0].life, mine, "and to nobody else");
    assert_eq!(
        permanents(&state, PlayerId(1)),
        1,
        "and their creature is still there"
    );
}

/// An opponent with no creature is not asked, they are told: a decision with no payable
/// answer is not a decision (CR 608.2), and the consequence still has its target.
#[test]
fn issue_706_an_opponent_with_nothing_to_sacrifice_is_not_asked() {
    let db = db();
    let state = main_phase(&db);
    let life = state.players[1].life;

    let state = dragon_enters(&state, &db);

    assert!(
        pending_player_choice(&state).is_none(),
        "there was nothing to decide"
    );
    assert_eq!(state.players[1].life, life - 5, "so the damage happened");
}
