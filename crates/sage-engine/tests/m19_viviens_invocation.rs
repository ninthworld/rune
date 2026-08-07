//! Vivien's Invocation, and the reflexive triggered ability it needs (issue #722,
//! CR 603.11).
//!
//! `Look at the top seven cards of your library. You may put a creature card from among
//! them onto the battlefield. Put the rest on the bottom of your library in a random
//! order. When a creature is put onto the battlefield this way, it deals damage equal to
//! its power to target creature an opponent controls.`
//!
//! The last sentence is the new thing. It is an ability the *resolution* creates, about
//! something that resolution just did, and it cannot be an ordinary trigger for two
//! reasons: nothing on the battlefield is watching (the sorcery is in a graveyard by
//! then, and an ETB watcher would fire for every creature that ever entered), and its
//! target could not have been chosen at announcement, because until the player answers
//! there is nothing to deal damage *with*.
//!
//! So it goes on the stack unaimed, after the spell finishes, and its controller aims it
//! — which is what CR 603.11b says, and is exactly the treatment every printed trigger
//! already gets.
//!
//! Every test drives the real [`apply_action`] pipeline: the real mid-resolution choice,
//! the real trigger seam, and the real aiming action.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_player_choice, pending_trigger_target_choice,
    valid_actions, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId, GameState,
    Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase() -> GameState {
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

/// Seat 0's library, **top last**, with the Invocation in hand.
fn board(db: &CardDatabase, library: &[&str]) -> (GameState, CardInstance, Vec<CardInstance>) {
    let mut state = main_phase();
    let spell = state.new_instance(cid(db, "vivien_s_invocation"));
    state.players[0].hand.push(spell);
    let cards: Vec<CardInstance> = library
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[0].library = cards.clone();
    (state, spell, cards)
}

/// Cast the Invocation and let it resolve up to the question it asks.
fn cast(state: &GameState, db: &CardDatabase, spell: CardInstance) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card: spell,
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

/// The whole card, end to end: the look, the put, the reflexive trigger, and the damage.
#[test]
fn issue_722_a_creature_put_onto_the_battlefield_deals_its_power_to_a_chosen_creature() {
    let db = db();
    // Onakke Ogre is a 4/2, so its power is a number no other quantity here shares.
    let (mut state, spell, cards) = board(
        &db,
        &[
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "onakke_ogre",
        ],
    );
    // A 10/10 to be shot at: it survives whatever lands, so "did the damage land" and
    // "was it the right amount" stay different questions.
    let victim = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let state = cast(&state, &db, spell);

    let pending = pending_player_choice(&state).expect("the look asks which card to take");
    assert_eq!(pending.chooser, PlayerId(0));
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![cards[6].id],
        },
        &db,
    );

    // The creature arrived, and the reflexive ability is on the stack owing a target.
    let ogre = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("the Ogre is on the battlefield")
        .id;
    let ability =
        pending_trigger_target_choice(&state).expect("the reflexive ability owes a target");
    // Aiming is the only thing on offer: nobody receives priority while a trigger owes
    // its targets (CR 603.3b).
    assert!(
        valid_actions(&state, &db).iter().all(|action| matches!(
            action,
            Action::ChooseTriggerTargets { .. } | Action::Concede
        )),
        "aiming the trigger is all that is offered — conceding is always legal"
    );
    // And the class really is *an opponent's*: aiming at the creature that just arrived
    // is refused, and the aim is still owed afterwards.
    let refused = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(ogre)],
        },
        &db,
    );
    assert_eq!(
        pending_trigger_target_choice(&refused),
        Some(ability),
        "your own creature is not a legal aim, so the trigger still owes one"
    );

    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(victim)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let marked = state
        .battlefield
        .iter()
        .find(|perm| perm.id == victim)
        .expect("a 10/10 survives 4 damage")
        .damage;
    assert_eq!(marked, 4, "the Ogre's power, not a number the card prints");
    assert_eq!(
        state.players[0].library.len(),
        6,
        "the six it did not take went to the bottom"
    );
}

/// *You may.* Declining puts nothing anywhere, and the reflexive ability never happens —
/// there is no "it" for the sentence to be about.
#[test]
fn issue_722_declining_the_put_creates_no_trigger_at_all() {
    let db = db();
    let (mut state, spell, _) = board(
        &db,
        &[
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "onakke_ogre",
        ],
    );
    let victim = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let state = cast(&state, &db, spell);

    let state = apply_action(&state, &Action::AnswerChoice { chosen: Vec::new() }, &db);

    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "nothing was put onto the battlefield, so nothing triggered"
    );
    assert!(state.stack.is_empty(), "and the stack is empty");
    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == victim)
            .expect("still there")
            .damage,
        0,
        "no damage was dealt"
    );
    assert_eq!(
        state.players[0].library.len(),
        7,
        "all seven went to the bottom"
    );
}

/// A **creature** card, and only a creature card: the land among the seven is not on
/// offer, and a library with no creature in its top seven asks nothing.
#[test]
fn issue_722_only_a_creature_card_may_be_taken() {
    let db = db();
    let (state, spell, _) = board(
        &db,
        &[
            "forest", "forest", "forest", "forest", "forest", "forest", "forest",
        ],
    );
    let state = cast(&state, &db, spell);

    assert!(
        pending_player_choice(&state).is_none(),
        "seven lands is nothing to choose between, so no question is posed"
    );
    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "and nothing entered, so nothing triggered"
    );
    assert_eq!(state.battlefield.len(), 0, "no land was put onto the field");
    assert_eq!(state.players[0].library.len(), 7, "all seven bottomed");
}

/// CR 608.2h: killing the creature in response does not stop the damage. Its power was
/// last known when the ability was created, and that is what it deals.
#[test]
fn issue_722_a_creature_killed_in_response_still_deals_its_damage() {
    let db = db();
    let (mut state, spell, cards) = board(
        &db,
        &[
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "onakke_ogre",
        ],
    );
    let victim = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let state = cast(&state, &db, spell);
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![cards[6].id],
        },
        &db,
    );
    let ogre = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("the Ogre is on the battlefield")
        .id;
    let ability = pending_trigger_target_choice(&state).expect("the ability owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(victim)],
        },
        &db,
    );

    // Removal, in response: the Ogre is gone before the ability resolves.
    let mut state = state;
    state.battlefield.retain(|perm| perm.id != ogre);
    assert!(!on_battlefield(&state, ogre));

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == victim)
            .expect("a 10/10 survives 4 damage")
            .damage,
        4,
        "last known power (CR 608.2h), from a creature that is no longer there"
    );
}

/// The power is read on **resolution** (CR 608.2), so a creature pumped after the ability
/// is on the stack deals the larger number.
#[test]
fn issue_722_the_power_is_read_when_the_ability_resolves() {
    let db = db();
    let (mut state, spell, cards) = board(
        &db,
        &[
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "onakke_ogre",
        ],
    );
    let victim = place(&mut state, &db, "gigantosaurus", PlayerId(1));
    let state = cast(&state, &db, spell);
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![cards[6].id],
        },
        &db,
    );
    let ogre = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("the Ogre is on the battlefield")
        .id;
    let ability = pending_trigger_target_choice(&state).expect("the ability owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(victim)],
        },
        &db,
    );

    // A +1/+1 counter, in response.
    let mut state = state;
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == ogre) {
        perm.counters
            .insert(sage_engine::CounterKind::PlusOnePlusOne, 1);
    }
    assert_eq!(
        characteristics(&state, ogre, &db).power,
        Some(5),
        "a 4/2 with a counter is a 5/3"
    );

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == victim)
            .expect("a 10/10 survives 5 damage")
            .damage,
        5,
        "the power it has now, not the power it had when the ability was created"
    );
}

/// CR 603.3c: with no legal target the ability never goes on the stack — the creature is
/// still put onto the battlefield, because that already happened.
#[test]
fn issue_722_with_no_opponent_creature_the_ability_is_not_put_on_the_stack() {
    let db = db();
    let (state, spell, cards) = board(
        &db,
        &[
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "forest",
            "onakke_ogre",
        ],
    );
    let state = cast(&state, &db, spell);
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![cards[6].id],
        },
        &db,
    );

    assert_eq!(state.battlefield.len(), 1, "the Ogre still arrived");
    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "and its ability found nothing to aim at, so it was never put on the stack"
    );
    assert!(state.stack.is_empty());
}
