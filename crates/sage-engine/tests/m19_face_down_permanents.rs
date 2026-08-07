//! Tezzeret, Cruel Machinist (issue #706): the first **face-down** permanents, and the
//! first effect that lasts until its controller's next turn.
//!
//! A face-down permanent (CR 708.2) is the two halves of the two things a permanent could
//! already be, one from each: its characteristics come from a carried blob, as a token's
//! do, and its **card** is still underneath, as an ordinary permanent's is. That second
//! half is the whole reason it is not modelled as a token — a face-down card that dies
//! goes to its owner's graveyard as itself (CR 708.4), where a token would simply cease to
//! exist (CR 111.7).
//!
//! It has no name and no abilities, and that is not a rule stated anywhere in the engine:
//! it is what the carried characteristics say, because the effect that turned the cards
//! down wrote neither.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_player_choice, Action, CardDatabase, CardId, CardType,
    Color, CounterKind, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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

/// A Tezzeret with enough loyalty to reach the ultimate.
fn tezzeret(state: &mut GameState, db: &CardDatabase, loyalty: u32) -> PermanentId {
    let id = place(state, db, "tezzeret_cruel_machinist", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == id) {
        perm.counters.insert(CounterKind::Loyalty, loyalty);
        perm.entered_turn = 0;
    }
    id
}

/// Activate ability `index` and stop at the first question, or when the stack empties.
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
        if state.stack.is_empty() || pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// Walk the turn structure to the precombat main phase of `turn`, answering the two
/// declarations combat asks for with nothing so the walk never stalls.
fn walk_to_turn(state: &GameState, db: &CardDatabase, turn: u32) -> GameState {
    let mut state = state.clone();
    for _ in 0..200 {
        if state.turn >= turn && state.step == Step::PrecombatMain {
            return state;
        }
        let action = if state.step == Step::DeclareAttackers && !state.attackers_declared {
            Action::DeclareAttackers {
                attackers: Vec::new(),
            }
        } else if state.step == Step::DeclareBlockers && !state.blockers_declared {
            state.priority = PlayerId(1 - state.active_player.0);
            Action::DeclareBlockers { blocks: Vec::new() }
        } else {
            Action::PassPriority
        };
        let next = apply_action(&state, &action, db);
        assert_ne!(next, state, "the walk stalled at {:?}", state.step);
        state = next;
    }
    state
}

/// The face-down permanents on the battlefield.
fn face_down(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.is_face_down())
        .map(|perm| perm.id)
        .collect()
}

/// **The crux.** Two cards go down and arrive as 5/5 artifact creatures with nothing of
/// their own — and they are still cards.
#[test]
fn issue_706_cards_put_down_face_down_are_five_five_artifact_creatures() {
    let db = db();
    let mut state = main_phase(&db);
    let tezzeret = tezzeret(&mut state, &db, 9);
    // Two cards in hand that are nothing like a 5/5 artifact creature.
    let hand: Vec<_> = ["shock", "plains"]
        .iter()
        .map(|slug| state.new_instance(cid(&db, slug)))
        .collect();
    state.players[0].hand = hand.clone();

    let state = activate(&state, &db, tezzeret, 2, Vec::new());
    let pending = pending_player_choice(&state).expect("the ultimate asks which cards");
    assert_eq!(pending.chooser, PlayerId(0));
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: hand.iter().map(|card| card.id).collect(),
        },
        &db,
    );

    let down = face_down(&state);
    assert_eq!(down.len(), 2, "both cards arrived");
    for id in down {
        let current = characteristics(&state, id, &db);
        assert_eq!(current.power, Some(5), "a 5/5");
        assert_eq!(current.toughness, Some(5));
        assert!(
            current.types.contains(&CardType::Artifact)
                && current.types.contains(&CardType::Creature),
            "an artifact creature, whatever the card was"
        );
        assert!(current.keywords.is_empty(), "and nothing of its own");
    }
    assert!(state.players[0].hand.is_empty(), "the hand is empty");
}

/// **The half that makes it not a token.** A face-down permanent that dies is a card, and
/// reaches its owner's graveyard as the card it always was (CR 708.4).
#[test]
fn issue_706_a_face_down_permanent_that_dies_is_still_a_card() {
    let db = db();
    let mut state = main_phase(&db);
    let tezzeret = tezzeret(&mut state, &db, 9);
    let shock = state.new_instance(cid(&db, "shock"));
    state.players[0].hand = vec![shock];

    let state = activate(&state, &db, tezzeret, 2, Vec::new());
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![shock.id],
        },
        &db,
    );
    let down = *face_down(&state).first().expect("it arrived");

    // Kill it with a Murder — it is a creature while it is down, whatever it is printed as.
    let mut state = state;
    let murder = state.new_instance(cid(&db, "murder"));
    state.players[0].hand.push(murder);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: murder,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(down)],
            payment: Vec::new(),
        },
        &db,
    );
    let mut state = state;
    for _ in 0..8 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    assert!(
        state.battlefield.iter().all(|perm| perm.id != down),
        "the face-down creature died"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == shock.id),
        "and the card it was is in the graveyard"
    );
}

/// The zero ability animates an artifact **until its controller's next turn** — so it is
/// still a creature after the turn it was made one ends.
#[test]
fn issue_706_an_animated_artifact_lasts_past_the_turn_it_was_animated_on() {
    let db = db();
    let mut state = main_phase(&db);
    let tezzeret = tezzeret(&mut state, &db, 4);
    // A Millstone: an artifact that is not a creature, so the animation is visible
    // coming and going.
    let bauble = place(&mut state, &db, "millstone", PlayerId(0));

    let state = activate(&state, &db, tezzeret, 1, vec![Target::Permanent(bauble)]);
    assert_eq!(
        characteristics(&state, bauble, &db).power,
        Some(5),
        "a 5/5 while the ability stands"
    );

    // Walk to the next turn's main phase: the cleanup that ends an until-end-of-turn
    // effect happens on the way, and this one survives it.
    let state = walk_to_turn(&state, &db, 2);
    assert_eq!(state.turn, 2, "a turn has passed");
    assert_eq!(
        characteristics(&state, bauble, &db).power,
        Some(5),
        "and it is still a 5/5 on the opponent's turn"
    );

    // And on to the controller's own next turn, where it ends.
    let state = walk_to_turn(&state, &db, 3);
    assert_eq!(state.turn, 3, "and another");
    assert_eq!(
        characteristics(&state, bauble, &db).power,
        None,
        "the animation ended at the cleanup of its controller's next turn"
    );
}
