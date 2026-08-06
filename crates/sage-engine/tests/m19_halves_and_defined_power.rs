//! Three amounts that come from somewhere new, and the two different *moments* they are
//! read at.
//!
//! Every amount the IR had before this was taken once, where the resolution reached it
//! (CR 608.2), and stayed that number for good. Two of the three here keep that promise —
//! half a total rounded up, and the power of a permanent an exile is about to remove — and
//! the third breaks it deliberately: a **characteristic-defining** power (CR 604.3) is not
//! an effect at all, and is re-derived on every read of the permanent with nothing in
//! between.
//!
//! Everything drives the real [`apply_action`] pipeline over the bundled catalog. The
//! cards are Enigma Drake (M19 216), Infernal Reckoning (M19 102), and Fraying Omnipotence
//! (M19 97).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_player_choice, permanent_choice_bounds,
    permanent_choice_candidates, Action, CardDatabase, CardId, CardInstance, Color, CounterKind,
    Duration, EffectAffects, FunctionalId, GameState, Modification, PendingChoice, Permanent,
    PermanentId, PlayerId, StaticEffect, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity. Never a written-down `CardId`.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability
/// never decides a test that is about an amount.
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

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

fn to_graveyard(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
}

/// Cast `slug` from player 0's hand at `targets` and let it resolve, with both seats
/// passing. A spell that suspends on a choice comes back still suspended.
fn cast_and_resolve(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

fn power_of(state: &GameState, db: &CardDatabase, id: PermanentId) -> i32 {
    characteristics(state, id, db).power.expect("a creature")
}

fn creatures_of(state: &GameState, db: &CardDatabase, seat: PlayerId) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| {
            sage_engine::controller_of(state, perm) == seat
                && perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.has_type(sage_engine::CardType::Creature))
        })
        .count()
}

fn pending(state: &GameState) -> &PendingChoice {
    pending_player_choice(state).expect("a choice is owed")
}

/// Answer the pending sacrifice by naming the first `count` permanents it offers.
fn sacrifice_first(state: &GameState, db: &CardDatabase, count: usize) -> GameState {
    let request = pending(state)
        .question
        .permanents()
        .expect("a permanent selection");
    let chosen: Vec<PermanentId> = permanent_choice_candidates(state, request, db)
        .into_iter()
        .take(count)
        .collect();
    apply_action(state, &Action::AnswerPermanents { chosen }, db)
}

/// Answer the pending discard by naming the first `count` cards it offers.
fn discard_first(state: &GameState, db: &CardDatabase, count: usize) -> GameState {
    let request = pending(state).question.cards().expect("a card selection");
    let chosen = sage_engine::choice_candidates(state, request, db)
        .into_iter()
        .take(count)
        .map(|inst| inst.id)
        .collect();
    apply_action(state, &Action::AnswerChoice { chosen }, db)
}

// ----- Enigma Drake: a characteristic-defining power at CR 613 layer 7a ------

/// CR 604.3: the power is re-derived on **every read**. Nothing happens between these two
/// reads — no action, no trigger, no event — and the number still moves, because a card
/// arriving in the graveyard is all it takes.
#[test]
fn cr_604_3_enigma_drake_power_follows_the_graveyard_with_no_event_between() {
    let db = db();
    let mut state = main_phase();
    let drake = place(&mut state, &db, "enigma_drake", PlayerId(0));
    assert_eq!(
        power_of(&state, &db, drake),
        0,
        "an empty graveyard makes it a 0/4"
    );

    to_graveyard(&mut state, &db, "shock", PlayerId(0));
    to_graveyard(&mut state, &db, "divination", PlayerId(0));
    assert_eq!(power_of(&state, &db, drake), 2, "one instant, one sorcery");

    // A creature card in the same graveyard is not counted, and a card in the *other*
    // graveyard is not "in your graveyard".
    to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    to_graveyard(&mut state, &db, "shock", PlayerId(1));
    assert_eq!(power_of(&state, &db, drake), 2, "neither one counts");

    // And it goes back down: the count is a read, not a modifier anything recorded.
    state.players[0].graveyard.clear();
    assert_eq!(power_of(&state, &db, drake), 0, "back to nothing");
}

/// CR 613.4: layer 7a *sets* the power, and layer 7c adds to whatever it left. A `+1/+1`
/// counter is worth one more than the graveyard says, never one more than the printed `0`.
#[test]
fn cr_613_4_a_defined_power_is_the_base_a_counter_adds_to() {
    let db = db();
    let mut state = main_phase();
    let drake = place(&mut state, &db, "enigma_drake", PlayerId(0));
    for slug in ["shock", "shock", "divination"] {
        to_graveyard(&mut state, &db, slug, PlayerId(0));
    }
    assert_eq!(power_of(&state, &db, drake), 3);

    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == drake)
        .expect("the Drake")
        .counters
        .insert(CounterKind::PlusOnePlusOne, 2);
    assert_eq!(
        power_of(&state, &db, drake),
        5,
        "3 from the graveyard at 7a, then +2 from counters at 7c"
    );
    assert_eq!(
        characteristics(&state, drake, &db).toughness,
        Some(6),
        "toughness is printed 4 plus the same counters; 7a does not touch it"
    );

    // The counters stay put while the graveyard moves underneath them.
    to_graveyard(&mut state, &db, "shock", PlayerId(0));
    assert_eq!(power_of(&state, &db, drake), 6);
}

/// The other half of layer 7c — a timestamped `+X/+Y` modifier — stacks on the defined
/// power the same way, and the two together are the whole of layer 7 today: no effect in
/// the engine sets a base power at 7b, so a defined power is never overruled.
#[test]
fn cr_613_4_a_defined_power_is_the_base_an_anthem_adds_to() {
    let db = db();
    let mut state = main_phase();
    let drake = place(&mut state, &db, "enigma_drake", PlayerId(0));
    to_graveyard(&mut state, &db, "shock", PlayerId(0));
    to_graveyard(&mut state, &db, "shock", PlayerId(0));

    let timestamp = state.mint_id();
    state.static_effects.push(StaticEffect {
        source: timestamp,
        affects: EffectAffects::SpecificPermanent(drake),
        modification: Modification::PowerToughness {
            power: 2,
            toughness: 0,
        },
        duration: Duration::UntilEndOfTurn,
    });
    assert_eq!(
        power_of(&state, &db, drake),
        4,
        "2 from the graveyard at 7a, then +2 at 7c"
    );
}

/// "Your graveyard" is read through CR 613 layer 2, so a Drake someone has taken reads
/// its *current* controller's graveyard rather than the one it started under.
#[test]
fn cr_613_2_a_stolen_drake_counts_its_new_controllers_graveyard() {
    let db = db();
    let mut state = main_phase();
    let drake = place(&mut state, &db, "enigma_drake", PlayerId(1));
    to_graveyard(&mut state, &db, "shock", PlayerId(0));
    to_graveyard(&mut state, &db, "shock", PlayerId(0));
    to_graveyard(&mut state, &db, "shock", PlayerId(1));
    assert_eq!(power_of(&state, &db, drake), 1, "its own seat's graveyard");

    let timestamp = state.mint_id();
    state.static_effects.push(StaticEffect {
        source: timestamp,
        affects: EffectAffects::SpecificPermanent(drake),
        modification: Modification::GainControl(PlayerId(0)),
        duration: Duration::UntilEndOfTurn,
    });
    assert_eq!(
        power_of(&state, &db, drake),
        2,
        "under seat 0 it reads seat 0's two instants"
    );
}

// ----- Infernal Reckoning: a chosen permanent's power, read before it leaves -----

/// CR 608.2h: the life gained is the power the creature *had*, read before the exile
/// takes it off the battlefield. A 2/1 grown to 4/3 by counters is worth four.
#[test]
fn cr_608_2h_infernal_reckoning_gains_the_power_the_creature_had() {
    let db = db();
    let mut state = main_phase();
    let creeper = place(&mut state, &db, "field_creeper", PlayerId(1));
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == creeper)
        .expect("the Creeper")
        .counters
        .insert(CounterKind::PlusOnePlusOne, 2);
    assert_eq!(power_of(&state, &db, creeper), 4, "2 printed, +2 counters");
    let life = state.players[0].life;

    let state = cast_and_resolve(
        &state,
        &db,
        "infernal_reckoning",
        vec![Target::Permanent(creeper)],
    );

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == creeper),
        "the creature is exiled"
    );
    assert_eq!(state.players[1].exile.len(), 1, "and it is in exile");
    assert_eq!(
        state.players[0].life,
        life + 4,
        "four life — its power as it left, not the 2 its card prints and not the 0 a \
         re-read of a gone permanent would give"
    );
}

/// The amount is fixed by the exile and nothing later in the turn moves it: the life is
/// gained, the permanent is gone, and taking the pump that grew it back off the board
/// afterwards changes neither.
#[test]
fn cr_608_2_the_life_gained_survives_the_permanent_leaving() {
    let db = db();
    let mut state = main_phase();
    let creeper = place(&mut state, &db, "field_creeper", PlayerId(1));
    let timestamp = state.mint_id();
    state.static_effects.push(StaticEffect {
        source: timestamp,
        affects: EffectAffects::SpecificPermanent(creeper),
        modification: Modification::PowerToughness {
            power: 3,
            toughness: 0,
        },
        duration: Duration::UntilEndOfTurn,
    });
    let life = state.players[0].life;

    let mut state = cast_and_resolve(
        &state,
        &db,
        "infernal_reckoning",
        vec![Target::Permanent(creeper)],
    );
    assert_eq!(state.players[0].life, life + 5, "2 printed, +3 pumped");

    state.static_effects.clear();
    assert_eq!(
        state.players[0].life,
        life + 5,
        "the pump ending does not take the life back"
    );
}

/// A coloured creature is not a legal target, so a spell aimed at one never resolves and
/// no life is gained (CR 608.2b).
#[test]
fn cr_115_1_infernal_reckoning_cannot_be_aimed_at_a_coloured_creature() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let life = state.players[0].life;

    let state = cast_and_resolve(
        &state,
        &db,
        "infernal_reckoning",
        vec![Target::Permanent(ogre)],
    );
    assert!(
        state.battlefield.iter().any(|perm| perm.id == ogre),
        "a red Ogre is not a colorless creature"
    );
    assert_eq!(state.players[0].life, life, "and no life was gained");
}

// ----- Fraying Omnipotence: half a total, rounded up, in three places -------

/// Every number this spell reads is odd, so every one of them rounds up: 7 life halves to
/// 4 lost, a hand of 5 halves to 3 discarded, and a board of 3 halves to 2 sacrificed.
#[test]
fn cr_608_2_fraying_omnipotence_rounds_up_in_all_three_places() {
    let db = db();
    let mut state = main_phase();
    for seat in [PlayerId(0), PlayerId(1)] {
        state.players[seat.0].life = 7;
        state.players[seat.0].hand.clear();
        for _ in 0..3 {
            place(&mut state, &db, "onakke_ogre", seat);
        }
        // A land is a permanent that is not a creature, so it is never in the class the
        // sacrifice offers — the count and the candidates read the same filter.
        place(&mut state, &db, "forest", seat);
    }
    // Five cards each, before the spell itself is put in seat 0's hand.
    for seat in [PlayerId(0), PlayerId(1)] {
        for _ in 0..5 {
            to_hand(&mut state, &db, "shock", seat);
        }
    }

    // Casting takes the spell out of the hand again, so both hands are five as it
    // resolves.
    let state = cast_and_resolve(&state, &db, "fraying_omnipotence", Vec::new());

    assert_eq!(state.players[0].life, 3, "7 loses 4 — half, rounded up");
    assert_eq!(state.players[1].life, 3, "and the caster is not spared");

    // The discards are posed one per seat, in seat order, each for three of five.
    let state = discard_first(&state, &db, 3);
    let state = discard_first(&state, &db, 3);
    assert_eq!(state.players[0].hand.len(), 2, "5 discards 3");
    assert_eq!(state.players[1].hand.len(), 2);

    // Then the sacrifices, again one per seat, each for two of three.
    let request = pending(&state)
        .question
        .permanents()
        .expect("a permanent selection");
    assert_eq!(
        permanent_choice_bounds(&state, request, &db),
        (2, 2),
        "3 creatures sacrifices 2 — half, rounded up"
    );
    assert_eq!(
        permanent_choice_candidates(&state, request, &db).len(),
        3,
        "the Forest is not a candidate"
    );

    let state = sacrifice_first(&state, &db, 2);
    let state = sacrifice_first(&state, &db, 2);
    assert!(pending_player_choice(&state).is_none(), "the spell is done");
    assert_eq!(creatures_of(&state, &db, PlayerId(0)), 1, "3 sacrifices 2");
    assert_eq!(creatures_of(&state, &db, PlayerId(1)), 1);
    assert_eq!(
        state.players[0].graveyard.len(),
        // three discards, two sacrificed Ogres, and the spell itself
        6,
        "everything that left went to a graveyard"
    );
}

/// An even total halves cleanly, which is the other half of "rounded up" meaning
/// something: 8 life loses 4, not 5.
#[test]
fn cr_608_2_fraying_omnipotence_halves_an_even_total_exactly() {
    let db = db();
    let mut state = main_phase();
    for seat in [PlayerId(0), PlayerId(1)] {
        state.players[seat.0].life = 8;
        state.players[seat.0].hand.clear();
    }

    let state = cast_and_resolve(&state, &db, "fraying_omnipotence", Vec::new());
    assert_eq!(state.players[0].life, 4);
    assert_eq!(state.players[1].life, 4);
    assert!(
        pending_player_choice(&state).is_none(),
        "empty hands and empty boards are never asked anything"
    );
}

/// CR 608.2: the number is fixed when the question is **posed**, from the board as it
/// stood then. Seat 0 answering — which takes two permanents off the battlefield — does
/// not resize the question already waiting for seat 1, and a board that has shrunk under
/// a frozen number is the case a re-read would get wrong.
#[test]
fn cr_608_2_a_sacrifice_already_posed_is_not_resized_by_a_board_that_moved() {
    let db = db();
    let mut state = main_phase();
    for seat in [PlayerId(0), PlayerId(1)] {
        state.players[seat.0].hand.clear();
        for _ in 0..3 {
            place(&mut state, &db, "onakke_ogre", seat);
        }
    }
    let before = state.battlefield.len();

    let state = cast_and_resolve(&state, &db, "fraying_omnipotence", Vec::new());
    // Both questions are on the queue at once; seat 0's is at the head.
    assert_eq!(pending(&state).chooser, PlayerId(0));
    let state = sacrifice_first(&state, &db, 2);
    assert_eq!(
        state.battlefield.len(),
        before - 2,
        "two permanents have left since the questions were posed"
    );

    let waiting = pending(&state);
    assert_eq!(waiting.chooser, PlayerId(1));
    let request = waiting
        .question
        .permanents()
        .expect("a permanent selection");
    assert_eq!(
        permanent_choice_bounds(&state, request, &db),
        (2, 2),
        "still two — the number was written down before anything moved"
    );
}

/// A player asked for more than they have sacrifices what they have, and a player with
/// none is never asked at all: the bound is clamped to the candidates, which is the whole
/// of the never-stall guarantee for this question shape.
#[test]
fn cr_701_17_a_sacrifice_is_clamped_to_the_board_and_never_stalls() {
    let db = db();
    let mut state = main_phase();
    for seat in [PlayerId(0), PlayerId(1)] {
        state.players[seat.0].hand.clear();
    }
    // Only seat 1 has creatures, and only one of them.
    place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let state = cast_and_resolve(&state, &db, "fraying_omnipotence", Vec::new());
    let waiting = pending(&state);
    assert_eq!(
        waiting.chooser,
        PlayerId(1),
        "seat 0 controls no creature and is never asked"
    );
    let request = waiting
        .question
        .permanents()
        .expect("a permanent selection");
    assert_eq!(permanent_choice_bounds(&state, request, &db), (1, 1));

    let state = sacrifice_first(&state, &db, 1);
    assert!(pending_player_choice(&state).is_none());
    assert_eq!(creatures_of(&state, &db, PlayerId(1)), 0);
}

/// A **token** is sacrificeable, which is the whole reason the sacrifice is its own
/// question shape: a token has no card behind it (CR 111), so it could never appear in a
/// card-shaped candidate list, and it leaves no card behind when it goes (CR 111.7).
#[test]
fn cr_111_a_token_can_be_named_by_a_sacrifice() {
    let db = db();
    let mut state = main_phase();
    for seat in [PlayerId(0), PlayerId(1)] {
        state.players[seat.0].hand.clear();
    }
    // Gallant Cavalry brings a 2/2 Knight token with it, and two of them bring two — so
    // seat 0 ends with four creatures, half of which is two, and both may be tokens.
    for _ in 0..2 {
        let cavalry = to_hand(&mut state, &db, "gallant_cavalry", PlayerId(0));
        let mut cast = apply_action(
            &state,
            &Action::CastSpell {
                card: cavalry,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        for _ in 0..4 {
            cast = apply_action(&cast, &Action::PassPriority, &db);
        }
        state = cast;
    }
    assert_eq!(
        creatures_of(&state, &db, PlayerId(0)),
        4,
        "two bodies and two tokens"
    );

    let state = cast_and_resolve(&state, &db, "fraying_omnipotence", Vec::new());
    let request = pending(&state)
        .question
        .permanents()
        .expect("a permanent selection");
    assert_eq!(
        permanent_choice_bounds(&state, request, &db),
        (2, 2),
        "four creatures sacrifices two"
    );
    assert_eq!(
        permanent_choice_candidates(&state, request, &db).len(),
        4,
        "the tokens are candidates like anything else"
    );

    let tokens: Vec<PermanentId> = state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.card().is_none())
        .map(|perm| perm.id)
        .collect();
    assert_eq!(tokens.len(), 2);
    let graveyard = state.players[0].graveyard.len();
    let state = apply_action(&state, &Action::AnswerPermanents { chosen: tokens }, &db);

    assert_eq!(
        creatures_of(&state, &db, PlayerId(0)),
        2,
        "both tokens are gone"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        graveyard + 1,
        "and neither left a card behind (CR 111.7) — the one arrival is the sorcery \
         itself, which reaches its final zone as the last answer resumes it (CR 608.3)"
    );
}
