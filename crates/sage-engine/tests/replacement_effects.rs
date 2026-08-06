//! The **replacement-effect layer** (CR 614, issue #731): an event modified before it
//! happens, the affected player ordering the effects that would modify it (CR 616.1), and
//! no effect applying to one event twice (CR 614.5).
//!
//! Every test drives the **real** [`apply_action`] pipeline. The shapes exercised here —
//! a creature that enters tapped *and* with counters, a one-shot replacement created by
//! an ability, a token created while a `nontoken` replacement waits — are IR shapes M19
//! does not print all of, so they are authored inline (ADR 0009); the M19 card that does
//! print one is tested against the bundled catalog in `m19_mistcaller.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, pending_replacement_options, valid_actions, Action,
    CardDatabase, CardId, CardInstance, Color, CounterKind, FunctionalId, GameState,
    OfferedReplacement, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

/// An inline catalog of the four shapes the layer is made of.
///
/// - `test_revenant` carries **two** self-replacements (CR 614.1c) and a graveyard
///   ability that puts itself onto the battlefield — an entry with two applicable
///   replacements and no created ones.
/// - `test_ghoul` carries the same graveyard ability and no replacements at all, so an
///   entry of one is modified only by whatever an ability created.
/// - `test_warden` sacrifices itself to create the one-shot replacement.
/// - `test_summoner` makes a token, which enters the battlefield exactly as a card does.
fn db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_revenant","name":"Test Revenant",
         "types":["creature"],"subtypes":["Spirit"],"mana_cost":"{1}{B}","colors":["black"],
         "power":1,"toughness":1,
         "abilities":[
           {"type":"enters_tapped"},
           {"type":"enters_with_counters","counter":"plus_one_plus_one","count":1},
           {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
            "effects":[{"kind":"return_self_from_graveyard","destination":"battlefield"}]}]},
        {"schema_version":1,"functional_id":"test_ghoul","name":"Test Ghoul",
         "types":["creature"],"subtypes":["Zombie"],"mana_cost":"{1}{B}","colors":["black"],
         "power":2,"toughness":2,
         "abilities":[
           {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
            "effects":[{"kind":"return_self_from_graveyard","destination":"battlefield"}]}]},
        {"schema_version":1,"functional_id":"test_warden","name":"Test Warden",
         "types":["creature"],"subtypes":["Spirit"],"mana_cost":"{1}{U}","colors":["blue"],
         "power":2,"toughness":1,"keywords":["flash"],
         "abilities":[
           {"type":"activated","cost":[{"kind":"sacrifice_this"}],
            "effects":[{"kind":"create_replacement","replacement":{
              "kind":"exile_entering",
              "entering":{"card_type":"creature","nontoken":true,"not_cast":true}}}]}]},
        {"schema_version":1,"functional_id":"test_summoner","name":"Test Summoner",
         "types":["creature"],"subtypes":["Human"],"mana_cost":"{1}{G}","colors":["green"],
         "power":1,"toughness":1,
         "abilities":[
           {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
            "effects":[{"kind":"create_token","token":{
              "name":"Beast","types":["creature"],"subtypes":["Beast"],
              "colors":["green"],"power":3,"toughness":3}}]}]}
    ]"#;
    CardDatabase::from_json(json).expect("an inline catalog")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a catalogued card")
}

/// A two-player game parked at player 0's precombat main with both pools stocked, so
/// payability never decides a test that is about a replacement.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    state
}

fn to_graveyard(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// Put `slug` on `seat`'s battlefield the crude way — the tests below are about what
/// happens to something *else* entering, so how the source got there is not the subject.
fn to_battlefield(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    let permanent = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: permanent,
        instance: instance.id,
        printed: sage_engine::Printed::Card(instance.card),
        controller: seat,
        entered_turn: 0,
        ..Default::default()
    });
    state.players[seat.0].hand.pop();
}

/// Sacrifice every `test_warden` `seat` controls, creating one replacement effect each.
fn create_replacements(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    count: usize,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..count {
        to_battlefield(&mut state, db, "test_warden", seat);
    }
    for _ in 0..count {
        let permanent = state
            .battlefield
            .iter()
            .find(|perm| perm.printed.card() == Some(cid(db, "test_warden")))
            .expect("a warden to sacrifice")
            .id;
        state.priority = seat;
        state.consecutive_passes = 0;
        state = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent,
                index: 0,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            db,
        );
        // The ability uses the stack; let it resolve.
        state = apply_action(&state, &Action::PassPriority, db);
        state = apply_action(&state, &Action::PassPriority, db);
    }
    assert_eq!(
        state.replacements.len(),
        count,
        "one replacement per warden"
    );
    state
}

/// Activate `slug`'s graveyard ability under `seat`, putting the card onto the
/// battlefield **without being cast**, and let the ability resolve.
fn reanimate(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> (GameState, CardInstance) {
    let mut state = state.clone();
    let card = to_graveyard(&mut state, db, slug, seat);
    state.priority = seat;
    state.consecutive_passes = 0;
    // The graveyard ability is the last one the definition authors.
    let index = sage_engine::abilities_of(db, card.card).len() - 1;
    let state = apply_action(
        &state,
        &Action::ActivateAbilityFromGraveyard {
            card,
            index,
            targets: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    (apply_action(&state, &Action::PassPriority, db), card)
}

fn on_battlefield(state: &GameState, card: CardInstance) -> bool {
    state
        .battlefield
        .iter()
        .any(|perm| perm.instance == card.id)
}

fn in_exile(state: &GameState, seat: PlayerId, card: CardInstance) -> bool {
    state.players[seat.0].exile.iter().any(|c| c.id == card.id)
}

// ----- the layer ------------------------------------------------------------

#[test]
fn issue_731_a_single_applicable_replacement_applies_without_asking_anyone() {
    // One replacement and nothing else: there is no ordering to decide, so the layer
    // applies it outright and the game never stops. The creature is exiled instead of
    // entering, which means no permanent, no ETB trigger, and nothing on the battlefield.
    let db = db();
    let state = create_replacements(&main_phase(), &db, PlayerId(1), 1);
    let (state, ghoul) = reanimate(&state, &db, "test_ghoul", PlayerId(0));

    assert!(pending_player_choice(&state).is_none(), "nothing was asked");
    assert!(!on_battlefield(&state, ghoul), "the entry was replaced");
    assert!(
        in_exile(&state, PlayerId(0), ghoul),
        "it went to exile instead"
    );
    assert!(
        state.replacements.is_empty(),
        "the next time it would happen has happened: the one-shot is spent"
    );
}

#[test]
fn issue_731_two_applicable_replacements_are_ordered_by_the_affected_permanents_controller() {
    // CR 616.1: the affected object's controller chooses, not the effects' controller.
    // Both replacements belong to player 1; the creature entering is player 0's, so
    // player 0 is asked — and asked exactly once, because applying one ends the event.
    let db = db();
    let state = create_replacements(&main_phase(), &db, PlayerId(1), 2);
    let ids: Vec<u64> = state.replacements.iter().map(|r| r.id).collect();
    let (asked, ghoul) = reanimate(&state, &db, "test_ghoul", PlayerId(0));

    let pending = pending_player_choice(&asked).expect("an ordering choice is owed");
    assert_eq!(
        pending.chooser,
        PlayerId(0),
        "the entering permanent's controller orders them (CR 616.1)"
    );
    assert_eq!(
        pending_replacement_options(&asked, &db).len(),
        2,
        "both replacements are offered"
    );
    assert_eq!(
        valid_actions(&asked, &db)
            .iter()
            .filter(|action| matches!(action, Action::AnswerReplacement { .. }))
            .count(),
        1,
        "the question is advertised once; the answer names the position"
    );
    assert_eq!(
        valid_actions(&asked, &db),
        vec![Action::AnswerReplacement { index: 0 }, Action::Concede],
        "no other seat and no other action while the question is owed"
    );

    // Either answer exiles the creature exactly once — and consumes *the one that was
    // named*, which is what makes the choice a real decision rather than a formality.
    for (answered, spent) in [(0usize, ids[0]), (1, ids[1])] {
        let after = apply_action(
            &asked,
            &Action::AnswerReplacement {
                index: u8::try_from(answered).unwrap(),
            },
            &db,
        );
        assert!(!on_battlefield(&after, ghoul), "the entry was replaced");
        assert!(in_exile(&after, PlayerId(0), ghoul));
        assert_eq!(
            after.replacements.len(),
            1,
            "exactly one replacement applied to the one event (CR 614.5)"
        );
        assert_ne!(
            after.replacements[0].id, spent,
            "the replacement the player named is the one that was spent"
        );
        assert!(
            pending_player_choice(&after).is_none(),
            "the event is gone, so the survivor no longer applies"
        );
    }
}

#[test]
fn issue_731_a_self_replacement_applies_once_and_the_entry_completes() {
    // CR 614.5, and the reason the layer terminates: a permanent whose own abilities
    // carry two self-replacements is asked which applies first, and each applies exactly
    // once. Without the already-applied record, "enters tapped" would match its own
    // result forever.
    let db = db();
    let (asked, revenant) = reanimate(&main_phase(), &db, "test_revenant", PlayerId(0));

    let pending = pending_player_choice(&asked).expect("two self-replacements are ordered too");
    assert_eq!(pending.chooser, PlayerId(0));
    let offered = pending_replacement_options(&asked, &db);
    assert_eq!(offered.len(), 2);
    assert!(
        offered
            .iter()
            .all(|o| matches!(o, OfferedReplacement::SelfReplacement(_))),
        "both come from the entering object's own abilities (CR 614.1c)"
    );

    for order in [0u8, 1] {
        let after = apply_action(&asked, &Action::AnswerReplacement { index: order }, &db);
        assert!(
            pending_player_choice(&after).is_none(),
            "the second is the only one left, so it is applied rather than asked about"
        );
        let permanent = after
            .battlefield
            .iter()
            .find(|perm| perm.instance == revenant.id)
            .expect("the permanent entered");
        assert!(permanent.tapped, "the enters-tapped replacement applied");
        assert_eq!(
            permanent.counter_count(CounterKind::PlusOnePlusOne),
            1,
            "the enters-with-counters replacement applied, once"
        );
    }
}

#[test]
fn issue_731_a_self_replacement_and_a_created_one_are_ordered_together() {
    // One list, not two: the entering object's own replacement and the one an opponent's
    // ability created are offered side by side, because CR 616.1 orders every applicable
    // replacement rather than each source in turn.
    let db = db();
    let state = create_replacements(&main_phase(), &db, PlayerId(1), 1);
    let (asked, revenant) = reanimate(&state, &db, "test_revenant", PlayerId(0));

    let offered = pending_replacement_options(&asked, &db);
    assert_eq!(offered.len(), 3, "two printed, one created");
    assert_eq!(
        offered
            .iter()
            .filter(|o| matches!(o, OfferedReplacement::Created(_)))
            .count(),
        1
    );

    // Taking the created one first ends the event outright — nothing enters, and the
    // self-replacements never apply to anything.
    let index = u8::try_from(
        offered
            .iter()
            .position(|o| matches!(o, OfferedReplacement::Created(_)))
            .unwrap(),
    )
    .unwrap();
    let exiled = apply_action(&asked, &Action::AnswerReplacement { index }, &db);
    assert!(!on_battlefield(&exiled, revenant));
    assert!(in_exile(&exiled, PlayerId(0), revenant));
    assert!(pending_player_choice(&exiled).is_none());
}

#[test]
fn issue_731_an_index_past_the_offered_list_is_rejected() {
    // The answer names a position in the list the engine derives *now*, and an index
    // past its end is an answer to a question nobody asked: the action is illegal and
    // leaves the state untouched.
    let db = db();
    let state = create_replacements(&main_phase(), &db, PlayerId(1), 2);
    let (asked, _) = reanimate(&state, &db, "test_ghoul", PlayerId(0));

    let after = apply_action(&asked, &Action::AnswerReplacement { index: 7 }, &db);
    assert_eq!(after, asked, "an illegal answer is a no-op");
}

#[test]
fn issue_731_a_token_is_asked_about_and_a_nontoken_replacement_passes_over_it() {
    // A token enters the battlefield through the same seam a card does (ADR 0015), so a
    // replacement is consulted about it — and one that says `nontoken` simply does not
    // apply, which is a filter rather than an omission.
    let db = db();
    let state = create_replacements(&main_phase(), &db, PlayerId(1), 1);
    let mut state = state;
    to_battlefield(&mut state, &db, "test_summoner", PlayerId(0));
    let summoner = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "test_summoner")))
        .expect("the summoner")
        .id;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: summoner,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state.battlefield.iter().any(|perm| perm.printed.is_token()),
        "the token entered"
    );
    assert_eq!(
        state.replacements.len(),
        1,
        "the nontoken replacement was not applied and is still waiting"
    );
}

#[test]
fn issue_731_a_cast_creature_is_not_replaced_by_a_without_being_cast_effect() {
    // The one fact about an entry that cannot be read off the object: the same card
    // reanimated and cast produces the same permanent, and `without being cast`
    // distinguishes exactly those two.
    let db = db();
    let mut state = create_replacements(&main_phase(), &db, PlayerId(1), 1);
    let ghoul = state.new_instance(cid(&db, "test_ghoul"));
    state.players[0].hand.push(ghoul);
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: ghoul,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(on_battlefield(&state, ghoul), "a cast creature enters");
    assert_eq!(
        state.replacements.len(),
        1,
        "the replacement did not apply and is still waiting"
    );
}

#[test]
fn issue_731_a_replacement_created_this_turn_lapses_with_the_turn() {
    // CR 614.1b, the `this turn` half: a one-shot that never saw its event is gone at
    // the turn boundary, dropped where every other per-turn permission is.
    let db = db();
    let state = create_replacements(&main_phase(), &db, PlayerId(1), 1);
    let turn = state.turn;
    let mut next = state;
    for _ in 0..400 {
        if next.turn != turn {
            break;
        }
        // Pass wherever passing is offered; take the first other offer (a combat
        // declaration) wherever it is not, so the turn actually finishes.
        let actions = valid_actions(&next, &db);
        let action = if actions.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            actions
                .into_iter()
                .find(|action| !matches!(action, Action::Concede))
                .expect("something is always offered")
        };
        next = apply_action(&next, &action, &db);
    }
    assert_ne!(next.turn, turn, "the turn advanced");
    assert!(next.replacements.is_empty(), "it did not outlive its turn");
}
