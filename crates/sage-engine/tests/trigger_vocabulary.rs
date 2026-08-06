//! The events a triggered ability can now watch: a card being drawn, another permanent
//! attacking, a keyword on the permanent it watches, and an ability being activated
//! (issue #732).
//!
//! Every test drives the **real** [`apply_action`] pipeline. Two of the four conditions
//! have a printed M19 card to prove them with — Psychic Corrosion for the draw and
//! Runic Armasaur for the activation — and those tests use the bundled catalog, naming
//! cards by `functional_id` and never by an interned handle (ADR 0008 §3). The attack
//! watcher and the keyword filter have none yet: the M19 cards that want them
//! (Windreader Sphinx, Arcades) each need a second piece of vocabulary this change does
//! not add, so those definitions are authored inline (ADR 0009).
//!
//! The two assertions worth naming up front are the ones the conditions turn on. A draw
//! is counted from the **cards that moved**, never from hand size: Sift draws three and
//! discards one, and a hand-size reading would say two. A keyword is read through the
//! **computed** characteristics, never off the printed face: a printed ground creature
//! that was granted flying really does trip a flying watcher, and really does not once
//! the grant is gone.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, valid_actions, Action, Attack, AttackTarget, CardDatabase,
    CardId, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
};

/// Enough actions to walk a few whole turns; a settle that has not arrived by then is a
/// hang, and failing beats spinning.
const SETTLE_LIMIT: usize = 400;

// ----- shared fixtures ------------------------------------------------------

fn bundled() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a catalog card")
}

/// A two-player game at seat 0's precombat main, both pools stocked so payability never
/// decides a test that is about a trigger condition, and both libraries stocked so a
/// draw always has a card and a mill always has something to move.
fn main_phase(db: &CardDatabase, filler: &str) -> GameState {
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
    let card = cid(db, filler);
    for seat in 0..2 {
        state.players[seat].library = (0..30).map(|_| state.new_instance(card)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free
/// of summoning sickness, and return its battlefield identity.
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

/// Pass priority until the stack has emptied or a mid-resolution question is owed — the
/// two states in which nothing more happens without a real decision.
fn settle_stack(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if state.stack.is_empty() || pending_player_choice(&state).is_some() {
            return state;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    panic!("the stack never settled");
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is (the
/// empty combat declaration a declare step owes, or an arbitrary answer to a question).
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if done(&state) {
            return state;
        }
        let offered = valid_actions(&state, db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|action| action != &Action::Concede)
                .expect("some action is always available")
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the game never reached the state under test");
}

/// Cast `slug` from `seat`'s hand and settle whatever it produced.
fn cast_by(state: &GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    settle_stack(&state, db)
}

/// Activate ability `index` of `permanent` as `seat`, and settle whatever it produced.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    permanent: PermanentId,
    index: usize,
) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    settle_stack(&state, db)
}

/// The number of cards in `seat`'s graveyard — the ledger a mill writes to.
fn graveyard(state: &GameState, seat: PlayerId) -> usize {
    state.players[seat.0].graveyard.len()
}

/// The number of cards in `seat`'s hand.
fn hand(state: &GameState, seat: PlayerId) -> usize {
    state.players[seat.0].hand.len()
}

/// Whether the game is currently owing a yes-or-no.
fn confirm_owed(state: &GameState) -> bool {
    pending_player_choice(state).is_some_and(|pending| pending.question.confirm().is_some())
}

/// Answer a pending card choice with its first candidate — the arbitrary discard a test
/// about draws does not care about — and settle whatever was waiting behind it.
fn answer_first_card(state: &GameState, db: &CardDatabase) -> GameState {
    let pending = pending_player_choice(state).expect("a question is owed");
    let request = pending.question.cards().expect("a card choice");
    let candidates = sage_engine::choice_candidates(state, request, db);
    let chosen = candidates.first().expect("a candidate to choose").id;
    let state = apply_action(
        state,
        &Action::AnswerChoice {
            chosen: vec![chosen],
        },
        db,
    );
    settle_stack(&state, db)
}

// ----- Psychic Corrosion: a card drawn ---------------------------------------

#[test]
fn psychic_corrosion_mills_two_for_each_card_its_controller_draws() {
    // "Whenever you draw a card, each opponent mills two cards." The headline property
    // is the *per card* one: Divination is one spell, two draws, and therefore two
    // triggers — four cards milled, not two.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "psychic_corrosion", PlayerId(0));
    let milled_before = graveyard(&state, PlayerId(1));

    let state = cast_by(&state, &db, "divination", PlayerId(0));
    assert_eq!(
        graveyard(&state, PlayerId(1)),
        milled_before + 4,
        "a two-card draw triggers twice, milling two each time"
    );
    assert_eq!(
        graveyard(&state, PlayerId(0)),
        1,
        "and only the Divination itself reaches its caster's graveyard"
    );
}

#[test]
fn psychic_corrosion_counts_the_cards_drawn_not_the_hand() {
    // The assertion the condition turns on. Sift draws three and discards one, so the
    // hand grows by two while three cards were drawn: a hand-size reading would mill
    // four, and the event reading mills six.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "psychic_corrosion", PlayerId(0));
    let milled_before = graveyard(&state, PlayerId(1));
    let held_before = hand(&state, PlayerId(0));

    // The discard suspends Sift's resolution to ask which card goes; answering it lets
    // the three triggers behind it resolve.
    let state = cast_by(&state, &db, "sift", PlayerId(0));
    let state = answer_first_card(&state, &db);

    assert_eq!(
        hand(&state, PlayerId(0)),
        held_before + 2,
        "three drawn and one discarded leaves the hand two larger"
    );
    assert_eq!(
        graveyard(&state, PlayerId(1)),
        milled_before + 6,
        "three cards were drawn, so the watcher fired three times"
    );
}

#[test]
fn psychic_corrosion_ignores_a_draw_by_the_seat_it_is_not_on() {
    // "Whenever **you** draw": the opponent's own draw is not this ability's event, so
    // an opponent drawing mills nobody. Revitalize is the instant, so the opponent can
    // really cast it on this seat's turn.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "psychic_corrosion", PlayerId(0));
    let held = hand(&state, PlayerId(1));

    let state = cast_by(&state, &db, "revitalize", PlayerId(1));
    assert_eq!(
        hand(&state, PlayerId(1)),
        held + 1,
        "the opponent really did draw"
    );
    assert_eq!(
        graveyard(&state, PlayerId(1)),
        1,
        "and only their own spent instant reached their graveyard"
    );
    assert!(state.stack.is_empty(), "nothing triggered");
}

#[test]
fn psychic_corrosion_fires_on_the_draw_step_draw() {
    // The commonest draw of all is the turn-based one (CR 504.1), and the condition is
    // about drawing rather than about a spell, so it fires there too — on its own
    // controller's draw step and on nobody else's.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "psychic_corrosion", PlayerId(0));
    let milled_before = graveyard(&state, PlayerId(1));

    // Turn 2 is seat 1's, and the draw step of it is theirs: the watcher is silent.
    let state = settle_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        graveyard(&state, PlayerId(1)),
        milled_before,
        "seat 1's own draw is not this ability's event"
    );

    // Turn 3 is seat 0's, and its draw step fires the watcher exactly once.
    let state = settle_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        graveyard(&state, PlayerId(1)),
        milled_before + 2,
        "seat 0's own draw step milled two"
    );
}

// ----- Runic Armasaur: an ability activated ---------------------------------

#[test]
fn runic_armasaur_watches_an_opponent_activating_a_creature_ability() {
    // "Whenever an opponent activates a nonmana ability of a creature or land, you may
    // draw a card." Seat 1 pumps its own Shivan Dragon; seat 0 is owed the question.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "runic_armasaur", PlayerId(0));
    let dragon = place(&mut state, &db, "shivan_dragon", PlayerId(1));
    let held = hand(&state, PlayerId(0));

    let state = activate(&state, &db, PlayerId(1), dragon, 0);
    assert!(
        confirm_owed(&state),
        "the activation put the watcher's may-draw on the stack, and it asked"
    );
    let drawn = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    assert_eq!(
        hand(&drawn, PlayerId(0)),
        held + 1,
        "accepting draws the card"
    );
}

#[test]
fn runic_armasaur_ignores_a_mana_ability() {
    // CR 605.3a, and the reason the exclusion lives in the condition rather than on the
    // card: a mana ability never uses the stack, so tapping a land for mana is not an
    // activation this condition can see at all.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "runic_armasaur", PlayerId(0));
    let forest = place(&mut state, &db, "forest", PlayerId(1));

    let state = activate(&state, &db, PlayerId(1), forest, 0);
    assert!(
        state.battlefield.iter().any(|p| p.id == forest && p.tapped),
        "the mana ability really was activated"
    );
    assert!(state.stack.is_empty(), "and nothing triggered");
    assert!(!confirm_owed(&state), "so nobody was asked anything");
}

#[test]
fn runic_armasaur_ignores_an_artifact_and_its_own_controller() {
    // The two filters, each on its own. An artifact's ability is an activation of the
    // wrong kind of permanent, and the watcher's own controller is not an opponent.
    let db = bundled();
    let mut state = main_phase(&db, "forest");
    place(&mut state, &db, "runic_armasaur", PlayerId(0));
    let book = place(&mut state, &db, "arcane_encyclopedia", PlayerId(1));
    let own_dragon = place(&mut state, &db, "shivan_dragon", PlayerId(0));

    let artifact = activate(&state, &db, PlayerId(1), book, 0);
    assert!(
        !confirm_owed(&artifact),
        "an artifact is neither a creature nor a land"
    );

    let mine = activate(&state, &db, PlayerId(0), own_dragon, 0);
    assert!(
        !confirm_owed(&mine),
        "the watcher's own controller is not an opponent"
    );
    assert!(mine.stack.is_empty());
}

// ----- inline definitions: an attacker, a keyword, and any activator --------

/// An inline catalog for the two conditions no authorable M19 card can prove yet: a
/// watcher of *another* permanent attacking, filtered by a keyword, and a watcher of an
/// activation by any player. The M19 cards that want the first (Windreader Sphinx,
/// Arcades) each need a second piece of vocabulary this change does not add.
///
/// Life is the ledger, in distinct amounts, so one total says which ability fired and
/// how often.
fn inline_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_flying_watcher","name":"Test Flying Watcher",
         "types":["enchantment"],"mana_cost":"{2}{U}","colors":["blue"],
         "abilities":[{"type":"triggered",
            "event":{"permanent_attacks":{"scope":"any_creature","keyword":"flying"}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
        {"schema_version":1,"functional_id":"test_ground_beast","name":"Test Ground Beast",
         "types":["creature"],"subtypes":["Beast"],"mana_cost":"{2}{G}","colors":["green"],
         "power":2,"toughness":2},
        {"schema_version":1,"functional_id":"test_sky_beast","name":"Test Sky Beast",
         "types":["creature"],"subtypes":["Bird"],"mana_cost":"{2}{U}","colors":["blue"],
         "power":2,"toughness":2,"keywords":["flying"]},
        {"schema_version":1,"functional_id":"test_wing_lord","name":"Test Wing Lord",
         "types":["creature"],"subtypes":["Bird"],"mana_cost":"{3}{U}","colors":["blue"],
         "power":1,"toughness":1,
         "abilities":[{"type":"static","affects":{"scope":"creatures_you_control","except_this":true},
            "modification":{"kind":"grant_keyword","keyword":"flying"}}]},
        {"schema_version":1,"functional_id":"test_activation_watcher","name":"Test Activation Watcher",
         "types":["enchantment"],"mana_cost":"{1}{W}","colors":["white"],
         "abilities":[{"type":"triggered",
            "event":{"ability_activated":{"source_types":["creature","land"]}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]},
        {"schema_version":1,"functional_id":"test_shrine","name":"Test Shrine",
         "types":["land"],"subtypes":[],"mana_cost":"","colors":[],
         "abilities":[
            {"type":"activated","cost":[{"kind":"tap"}],
             "effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]},
            {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
             "effects":[{"kind":"add_mana","color":"white","amount":1}]}]}
    ]"#;
    CardDatabase::from_json(json).expect("an inline catalog")
}

/// Walk to seat 0's declare-attackers step, declare `attackers`, and settle the stack.
fn attack_with(state: &GameState, db: &CardDatabase, attackers: &[PermanentId]) -> GameState {
    let state = settle_until(state, db, |s| s.step == Step::DeclareAttackers);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: attackers
                .iter()
                .map(|&attacker| Attack {
                    attacker,
                    defender: AttackTarget::Player(PlayerId(1)),
                })
                .collect(),
        },
        db,
    );
    settle_stack(&state, db)
}

#[test]
fn an_attack_watcher_fires_once_per_matching_attacker() {
    // The widened attack condition: it watches the board rather than its own source,
    // and reports how many times it was met, so a two-flier alpha strike is two
    // triggers and the ground creature beside them is none.
    let db = inline_db();
    let mut state = main_phase(&db, "test_ground_beast");
    place(&mut state, &db, "test_flying_watcher", PlayerId(0));
    let first = place(&mut state, &db, "test_sky_beast", PlayerId(0));
    let second = place(&mut state, &db, "test_sky_beast", PlayerId(0));
    let ground = place(&mut state, &db, "test_ground_beast", PlayerId(0));
    let life = state.players[0].life;

    let state = attack_with(&state, &db, &[first, second, ground]);
    assert_eq!(
        state.players[0].life,
        life + 2,
        "two fliers attacked, so the watcher fired twice"
    );
}

#[test]
fn an_attack_watcher_reads_granted_keywords_not_printed_ones() {
    // The assertion the keyword filter turns on. The same printed ground creature
    // attacks in both halves; the only difference is a lord granting flying at CR 613
    // layer 6, and the watcher notices exactly when the grant is there.
    let db = inline_db();
    let mut base = main_phase(&db, "test_ground_beast");
    place(&mut base, &db, "test_flying_watcher", PlayerId(0));
    let beast = place(&mut base, &db, "test_ground_beast", PlayerId(0));
    let life = base.players[0].life;

    let bare = attack_with(&base, &db, &[beast]);
    assert_eq!(
        bare.players[0].life, life,
        "a printed ground creature is not a flier"
    );

    let mut granted = base.clone();
    place(&mut granted, &db, "test_wing_lord", PlayerId(0));
    let granted = attack_with(&granted, &db, &[beast]);
    assert_eq!(
        granted.players[0].life,
        life + 1,
        "the same creature, granted flying, is one the watcher notices"
    );
}

#[test]
fn an_activation_watcher_notices_a_land_and_every_seat() {
    // The other half of the activation selector: `source_types` is satisfied by any one
    // of the types it names, and the default activator scope is every player — so the
    // watcher's own controller activating their own land fires it.
    let db = inline_db();
    let mut state = main_phase(&db, "test_ground_beast");
    place(&mut state, &db, "test_activation_watcher", PlayerId(0));
    let shrine = place(&mut state, &db, "test_shrine", PlayerId(0));
    let life = state.players[0].life;

    let state = activate(&state, &db, PlayerId(0), shrine, 0);
    assert_eq!(
        state.players[0].life,
        life + 2 + 4,
        "the land's own ability gained 2 and the watcher's trigger gained 4"
    );
}

#[test]
fn an_activation_watcher_ignores_the_mana_ability_of_the_same_permanent() {
    // One permanent, two abilities, and only one of them uses the stack (CR 605.3a).
    // The mana ability is invisible to the condition even though its source is exactly
    // the kind of permanent the selector names.
    let db = inline_db();
    let mut state = main_phase(&db, "test_ground_beast");
    place(&mut state, &db, "test_activation_watcher", PlayerId(0));
    let shrine = place(&mut state, &db, "test_shrine", PlayerId(0));
    let life = state.players[0].life;

    let state = activate(&state, &db, PlayerId(0), shrine, 1);
    assert_eq!(
        state.players[0].life, life,
        "a mana ability triggers nothing"
    );
    assert!(state.stack.is_empty());
}
