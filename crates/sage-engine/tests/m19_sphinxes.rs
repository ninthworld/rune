//! Two M19 cards the vocabulary could already say, and nobody had said (issue #706).
//!
//! Neither needed a line of engine code. That is the point of driving them: the claim
//! "this card is authorable today" is worth exactly as much as the test behind it, and
//! the composition each one asks for — an optional effect that *targets*, and a trigger
//! that watches the whole board's attackers — is one no other bundled card makes.
//!
//! Riddlemaster Sphinx aims first and asks second: the target is chosen when the trigger
//! goes on the stack (CR 603.3d) and the `you may` is answered when it resolves
//! (CR 601.2b does not apply to a trigger). Windreader Sphinx watches *every* attacking
//! flier, including its controller's opponents' — the card says "a creature with flying",
//! not "a creature you control".
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, pending_trigger_target_choice, Action, Attack,
    AttackTarget, CardDatabase, CardId, CardInstance, Color, FunctionalId, GameState, Permanent,
    PermanentId, PlayerId, Step, Target,
};

/// One attacker aimed at seat 0, which is who the Sphinx is watching for.
fn at_seat_zero(attacker: PermanentId) -> Attack {
    Attack {
        attacker,
        defender: AttackTarget::Player(PlayerId(0)),
    }
}

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
    let forest = cid(db, "forest");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(forest)).collect();
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

/// Cast `card` and let both seats pass, so it resolves and its trigger goes on the stack.
fn cast(state: &GameState, db: &CardDatabase, card: CardInstance) -> GameState {
    let state = apply_action(
        state,
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

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// The whole card: aim on the way onto the stack, answer on the way off it.
#[test]
fn riddlemaster_sphinx_aims_first_and_asks_second() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let sphinx = to_hand(&mut state, &db, "riddlemaster_sphinx", PlayerId(0));

    let state = cast(&state, &db, sphinx);

    // The trigger is on the stack owing a target, and only an opponent's creature is a
    // legal one — the class is on the *effect the `may` wraps*, which is the whole reason
    // an optional effect declares its wrapped effect's group.
    let ability = pending_trigger_target_choice(&state).expect("the trigger owes a target");
    let refused = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(mine)],
        },
        &db,
    );
    assert_eq!(
        pending_trigger_target_choice(&refused),
        Some(ability),
        "your own creature is not an opponent's"
    );

    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(victim)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    // Aimed, and only now asked.
    let pending = pending_player_choice(&state).expect("the `you may` is asked on resolution");
    assert_eq!(pending.chooser, PlayerId(0));
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);

    assert!(!on_battlefield(&state, victim), "the Ogre was returned");
    assert!(
        state.players[1]
            .hand
            .iter()
            .any(|card| card.card == cid(&db, "onakke_ogre")),
        "to its owner's hand, not its returner's"
    );
}

/// Declining is an answer: the target was legal, the trigger resolved, and nothing moved.
#[test]
fn riddlemaster_sphinx_may_decline_after_aiming() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let sphinx = to_hand(&mut state, &db, "riddlemaster_sphinx", PlayerId(0));

    let state = cast(&state, &db, sphinx);
    let ability = pending_trigger_target_choice(&state).expect("the trigger owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(victim)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);

    assert!(on_battlefield(&state, victim), "declining moved nothing");
    assert!(state.stack.is_empty(), "and the trigger is done");
}

/// A game at seat 1's declare-attackers step, with Windreader Sphinx watching from seat 0.
fn watching(db: &CardDatabase, attackers: &[&str]) -> (GameState, Vec<PermanentId>) {
    let mut state = main_phase(db);
    place(&mut state, db, "windreader_sphinx", PlayerId(0));
    let ids: Vec<PermanentId> = attackers
        .iter()
        .map(|slug| place(&mut state, db, slug, PlayerId(1)))
        .collect();
    state.active_player = PlayerId(1);
    state.priority = PlayerId(1);
    state.players[1].turn_began = state.turn;
    state.step = Step::DeclareAttackers;
    (state, ids)
}

/// It watches the **board**, not its controller: an opponent's flier attacking is a
/// creature with flying attacking.
#[test]
fn windreader_sphinx_notices_an_opponents_flier() {
    let db = db();
    let (state, attackers) = watching(&db, &["snapping_drake"]);
    let hand = state.players[0].hand.len();

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![at_seat_zero(attackers[0])],
        },
        &db,
    );

    // The trigger went on the stack when the declaration was made; the question comes
    // when it resolves, which is a round of priority away.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let pending = pending_player_choice(&state).expect("the `you may` is asked");
    assert_eq!(
        pending.chooser,
        PlayerId(0),
        "asked of the Sphinx's controller"
    );
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    assert_eq!(state.players[0].hand.len(), hand + 1, "a card drawn");
}

/// A ground attacker is not one: the keyword is the filter, and it is read through the
/// computed characteristics rather than the printed face.
#[test]
fn windreader_sphinx_ignores_a_creature_without_flying() {
    let db = db();
    let (state, attackers) = watching(&db, &["onakke_ogre"]);

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![at_seat_zero(attackers[0])],
        },
        &db,
    );

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        pending_player_choice(&state).is_none(),
        "nothing with flying attacked"
    );
    assert!(state.stack.is_empty());
}

/// Once per attacking flier (CR 603.6d): two of them ask twice.
#[test]
fn windreader_sphinx_fires_once_per_attacking_flier() {
    let db = db();
    let (state, attackers) = watching(&db, &["snapping_drake", "rustwing_falcon", "onakke_ogre"]);
    let hand = state.players[0].hand.len();

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: attackers.iter().map(|id| at_seat_zero(*id)).collect(),
        },
        &db,
    );

    // Two fliers, two questions — answered one at a time, each its own trigger.
    let mut state = state;
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, &db);
        state = apply_action(&state, &Action::PassPriority, &db);
        assert!(
            pending_player_choice(&state).is_some(),
            "a question per flier"
        );
        state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    }
    assert!(
        pending_player_choice(&state).is_none(),
        "and the Ogre asked nothing"
    );
    assert_eq!(state.players[0].hand.len(), hand + 2, "two cards drawn");
}
