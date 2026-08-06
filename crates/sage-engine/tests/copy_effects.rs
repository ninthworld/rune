//! Copy effects (CR 707), driven through the **real** [`apply_action`] pipeline.
//!
//! Three different mechanisms share a rules chapter and almost nothing else, and each is
//! proved here on its own terms:
//!
//! - **Copiable values at CR 613 layer 1.** A copy's characteristics are the copied
//!   object's *printed* ones, and everything the engine already does happens on top of
//!   them. The load-bearing assertion is the ordering: a copy carrying two `+1/+1`
//!   counters is the **copied** power plus two, which is only true if the copy is decided
//!   before layer 7 rather than after it.
//! - **A continuous copy from an Aura** (CR 707.2c). Its values are fixed when the effect
//!   starts to apply and never re-read, and the effect itself ends the instant the Aura
//!   stops being attached — derived, so there is nothing to prune and nothing to leak.
//! - **Copying a spell** (CR 707.10). A new object on the stack that was never *cast*,
//!   with a cast-watcher on the battlefield as the witness.
//!
//! Cards are named by their authored `functional_id`, never by an interned handle
//! (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, copiable_face, copy_choice_candidates, pending_player_choice,
    pending_trigger_target_choice, valid_actions, Action, CardDatabase, CardId, CardInstance,
    CardType, Color, CounterKind, FunctionalId, GameEvent, GameState, Keyword, Permanent,
    PermanentId, PlayerId, StackObjectKind, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game parked at player 0's precombat main, both pools stocked so
/// payability never decides a test that is about a copy.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 20);
        }
        player.mana_pool.add_colorless(20);
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`.
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

/// Cast `slug` from `seat`'s hand at `targets` and pass it down to resolution. The state
/// comes back wherever resolution left it — which for a card that chooses as it enters is
/// *before* the permanent exists.
fn cast(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let instance = to_hand(&mut state, db, slug, seat);
    let state = apply_action(
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
    assert!(
        state.stack.iter().any(|obj| obj.controller == seat),
        "{slug} did not reach the stack"
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Pass priority until the stack is empty, aiming anything that asks to be aimed at
/// `aim` — the CR 707.10c re-target of a spell copy included, which rides the same action
/// a triggered ability's targets do.
fn settle(state: &GameState, db: &CardDatabase, aim: Target) -> GameState {
    let mut state = state.clone();
    for _ in 0..64 {
        if let Some(ability) = pending_trigger_target_choice(&state) {
            state = apply_action(
                &state,
                &Action::ChooseTriggerTargets {
                    ability,
                    targets: vec![aim],
                },
                db,
            );
            continue;
        }
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// The permanent that arrived between `before` and `after`.
fn arrival(before: &GameState, after: &GameState) -> PermanentId {
    after
        .battlefield
        .iter()
        .find(|perm| !before.battlefield.iter().any(|old| old.id == perm.id))
        .expect("something arrived")
        .id
}

fn name_of(state: &GameState, db: &CardDatabase, id: PermanentId) -> String {
    let perm = state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .expect("on the battlefield");
    copiable_face(state, perm, db)
        .expect("a readable face")
        .name()
        .to_string()
}

fn pt(state: &GameState, db: &CardDatabase, id: PermanentId) -> (Option<i32>, Option<i32>) {
    let current = characteristics(state, id, db);
    (current.power, current.toughness)
}

fn life(state: &GameState, seat: PlayerId) -> i32 {
    state.players[seat.0].life
}

// ----- Mirror Image: a copy fixed as the permanent enters (CR 707.5) --------

/// The choice is part of *entering* (CR 614.12 + CR 707.5), so there is no instant at
/// which Mirror Image is a 0/0 on the battlefield. The spell has finished resolving and
/// the card is in no zone at all — exactly where a suspended spell's card sits — and the
/// only thing anyone may do is answer.
#[test]
fn cr_707_5_a_copy_is_named_before_the_permanent_exists() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let state = cast(&state, &db, "mirror_image", PlayerId(0), Vec::new());

    assert_eq!(
        state.battlefield.len(),
        1,
        "only the Ogre is there; the copy has not arrived yet"
    );
    assert!(state.stack.is_empty(), "the spell has finished resolving");
    let pending = pending_player_choice(&state).expect("a permanent is owed");
    assert_eq!(pending.chooser, PlayerId(0), "its controller answers");
    assert_eq!(state.priority, PlayerId(0));
    assert!(
        pending.resume.is_none(),
        "nothing was suspended: the entry is the last step of the resolution"
    );

    // The one thing on offer is the answer.
    let offered = valid_actions(&state, &db);
    assert!(offered.contains(&Action::AnswerPermanent { chosen: None }));
    assert!(!offered.contains(&Action::PassPriority));
    assert!(
        valid_actions(
            &{
                let mut other = state.clone();
                other.priority = PlayerId(1);
                other
            },
            &db
        )
        .is_empty(),
        "no other seat may act while the question is owed"
    );
}

/// Mirror Image really enters as a copy: name, types, and printed power/toughness are the
/// copied creature's, and the 0/0 it is printed as never happens.
#[test]
fn cr_707_2_mirror_image_enters_with_the_copied_characteristics() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let before = cast(&state, &db, "mirror_image", PlayerId(0), Vec::new());

    assert_eq!(
        copy_choice_candidates(
            &before,
            sage_engine::CopyClass::CreatureYouControl,
            PlayerId(0),
            &db
        ),
        vec![ogre],
        "the only creature its controller controls"
    );

    let after = apply_action(
        &before,
        &Action::AnswerPermanent { chosen: Some(ogre) },
        &db,
    );
    let image = arrival(&before, &after);

    assert_eq!(name_of(&after, &db, image), "Onakke Ogre");
    assert_eq!(pt(&after, &db, image), (Some(4), Some(2)));
    let current = characteristics(&after, image, &db);
    assert!(current.subtypes.contains(&"Ogre".to_string()));
    assert!(current.types.contains(&CardType::Creature));
    assert_eq!(current.mana_cost, "{2}{R}", "a mana cost is copiable");

    // The permanent is still physically the Mirror Image card — copying changes what it
    // *is*, never which card it is (CR 707.2 copies characteristics, not identity).
    let perm = after
        .battlefield
        .iter()
        .find(|p| p.id == image)
        .expect("the copy is on the battlefield");
    assert_eq!(perm.printed.card(), Some(cid(&db, "mirror_image")));
}

/// **Layer 1 precedes layer 7.** Counters are not a copiable value (CR 707.2), so they
/// are the copy's own and are folded onto the *copied* power — which is only the right
/// answer if the copy was decided first.
#[test]
fn cr_613_1a_a_copy_with_counters_is_the_copied_power_plus_the_counters() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let before = cast(&state, &db, "mirror_image", PlayerId(0), Vec::new());
    let mut after = apply_action(
        &before,
        &Action::AnswerPermanent { chosen: Some(ogre) },
        &db,
    );
    let image = arrival(&before, &after);

    // Two +1/+1 counters on the copy. Printed 0/0 plus two would be 2/2; copied 4/2 plus
    // two is 6/4, and only one of those readings puts layer 1 first.
    if let Some(perm) = after.battlefield.iter_mut().find(|p| p.id == image) {
        perm.counters.insert(CounterKind::PlusOnePlusOne, 2);
    }
    assert_eq!(pt(&after, &db, image), (Some(6), Some(4)));

    // And the counters were never copied *from* the original either: the Ogre is
    // untouched.
    assert_eq!(pt(&after, &db, ogre), (Some(4), Some(2)));
}

/// A copy has the copied card's **abilities**, because rules text is a copiable value
/// (CR 707.2a) — including its enters-the-battlefield trigger, which fires for the copy
/// as it arrives (CR 707.5).
#[test]
fn cr_707_2a_a_copy_has_the_copied_rules_text() {
    let db = db();
    let mut state = main_phase();
    let mage = place(&mut state, &db, "aven_wind_mage", PlayerId(0));
    let before = cast(&state, &db, "mirror_image", PlayerId(0), Vec::new());
    let after = apply_action(
        &before,
        &Action::AnswerPermanent { chosen: Some(mage) },
        &db,
    );
    let image = arrival(&before, &after);

    let current = characteristics(&after, image, &db);
    assert!(
        current.keywords.contains(&Keyword::Flying),
        "flying is printed on the copied card, so the copy has it"
    );
    assert_eq!(
        current.abilities,
        sage_engine::abilities_of(&db, cid(&db, "aven_wind_mage")),
        "the copy's abilities are the copied card's, and none of its own"
    );
}

/// `You may have …` is a real decision: declining leaves an ordinary Mirror Image, which
/// is a 0/0 and dies to CR 704.5f before anyone can act on it.
#[test]
fn cr_707_5_declining_leaves_the_card_as_itself() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let before = cast(&state, &db, "mirror_image", PlayerId(0), Vec::new());
    let after = apply_action(&before, &Action::AnswerPermanent { chosen: None }, &db);

    assert_eq!(
        after.battlefield.len(),
        1,
        "the 0/0 arrived and was put into the graveyard by CR 704.5f"
    );
    assert!(after.players[0]
        .graveyard
        .iter()
        .any(|card| card.card == cid(&db, "mirror_image")));
}

// ----- Metamorphic Alteration: a continuous copy from an Aura ---------------

/// The Aura's copy is **continuous** (CR 707.2c): its host is a copy for exactly as long
/// as the Aura is attached to it, and stops being one the instant the Aura is gone —
/// with nothing to prune, because the effect was derived rather than stored.
#[test]
fn cr_707_2c_the_hosts_copy_ends_with_the_aura() {
    let db = db();
    let mut state = main_phase();
    let host = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let model = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));

    let before = cast(
        &state,
        &db,
        "metamorphic_alteration",
        PlayerId(0),
        vec![Target::Permanent(host)],
    );
    let after = apply_action(
        &before,
        &Action::AnswerPermanent {
            chosen: Some(model),
        },
        &db,
    );
    let aura = arrival(&before, &after);

    assert_eq!(name_of(&after, &db, host), "Colossal Dreadmaw");
    assert_eq!(pt(&after, &db, host), (Some(6), Some(6)));
    assert!(characteristics(&after, host, &db)
        .keywords
        .contains(&Keyword::Trample));
    // The Aura itself is not the copy — it named the values for its host.
    assert_eq!(name_of(&after, &db, aura), "Metamorphic Alteration");

    // Take the Aura off the battlefield and the copy is simply not there any more. The
    // board is mutated directly because the point is the *derivation*: nothing anywhere
    // had to be told the effect ended.
    let mut gone = after.clone();
    gone.battlefield.retain(|perm| perm.id != aura);
    assert_eq!(name_of(&gone, &db, host), "Onakke Ogre");
    assert_eq!(pt(&gone, &db, host), (Some(4), Some(2)));
    assert!(!characteristics(&gone, host, &db)
        .keywords
        .contains(&Keyword::Trample));
}

/// CR 707.2b: once the copy is made, changing the *original* changes nothing. The chosen
/// creature growing a counter, being pumped, or being enchanted leaves the copy exactly
/// as it was — those were never copiable values in the first place (CR 707.2).
#[test]
fn cr_707_2b_the_chosen_creature_changing_later_does_not_change_the_copy() {
    let db = db();
    let mut state = main_phase();
    let host = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let model = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));

    let before = cast(
        &state,
        &db,
        "metamorphic_alteration",
        PlayerId(0),
        vec![Target::Permanent(host)],
    );
    let mut after = apply_action(
        &before,
        &Action::AnswerPermanent {
            chosen: Some(model),
        },
        &db,
    );
    assert_eq!(pt(&after, &db, host), (Some(6), Some(6)));

    // Three counters on the model, and an anthem-shaped Aura would do the same: none of
    // it is copiable, and none of it reaches the copy.
    if let Some(perm) = after.battlefield.iter_mut().find(|p| p.id == model) {
        perm.counters.insert(CounterKind::PlusOnePlusOne, 3);
    }
    assert_eq!(pt(&after, &db, model), (Some(9), Some(9)));
    assert_eq!(
        pt(&after, &db, host),
        (Some(6), Some(6)),
        "the copy reads the values it copied, not the model's current ones"
    );
}

/// Two copy effects on one permanent are ordered by timestamp (CR 613.7), and a
/// **layer-6 grant** applies over whichever of them wins — because a granted keyword is
/// not a copiable value and is applied three layers later.
#[test]
fn cr_613_7_a_later_copy_wins_and_a_layer_six_grant_survives_it() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let dreadmaw = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));

    // Mirror Image enters copying the Ogre: layer 1, timestamped when it entered.
    let before = cast(&state, &db, "mirror_image", PlayerId(0), Vec::new());
    let state = apply_action(
        &before,
        &Action::AnswerPermanent { chosen: Some(ogre) },
        &db,
    );
    let image = arrival(&before, &state);
    assert_eq!(name_of(&state, &db, image), "Onakke Ogre");

    // Knightly Valor on it: a layer-6 keyword grant and a layer-7c `+2/+2`, over the
    // copied 4/2.
    let before = cast(
        &state,
        &db,
        "knightly_valor",
        PlayerId(0),
        vec![Target::Permanent(image)],
    );
    let state = settle(&before, &db, Target::Permanent(image));
    assert_eq!(pt(&state, &db, image), (Some(6), Some(4)));
    assert!(characteristics(&state, image, &db)
        .keywords
        .contains(&Keyword::Vigilance));

    // Now a *second* layer-1 copy effect, from an Aura that entered later. Its timestamp
    // is the higher one, so it decides the copiable values — and the layer-6 grant from
    // the first Aura still applies on top of the new ones.
    let before = cast(
        &state,
        &db,
        "metamorphic_alteration",
        PlayerId(0),
        vec![Target::Permanent(image)],
    );
    let state = apply_action(
        &before,
        &Action::AnswerPermanent {
            chosen: Some(dreadmaw),
        },
        &db,
    );

    assert_eq!(name_of(&state, &db, image), "Colossal Dreadmaw");
    assert_eq!(
        pt(&state, &db, image),
        (Some(8), Some(8)),
        "the later copy's 6/6, plus Knightly Valor's +2/+2 at layer 7c"
    );
    let current = characteristics(&state, image, &db);
    assert!(current.keywords.contains(&Keyword::Trample), "copied");
    assert!(current.keywords.contains(&Keyword::Vigilance), "granted");
}

// ----- copying a transformed permanent (CR 707.8, CR 712.8e) ----------------

/// A creature that turns over in place, and a card that copies one as it enters. No M19
/// card transforms without changing zones, so this shape is driven from an inline
/// definition (ADR 0009) — the same fixture the multi-face tests use.
fn transform_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_turncoat","name":"Test Turncoat",
         "types":["creature"],"subtypes":["Human"],"mana_cost":"{1}{W}","colors":["white"],
         "power":2,"toughness":2,
         "abilities":[{"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
           "effects":[{"kind":"transform_self"}]}],
         "back_face":{"name":"Test Werewolf","types":["creature"],"subtypes":["Werewolf"],
           "colors":["white"],"power":4,"toughness":4,"keywords":["trample"]}},
        {"schema_version":1,"functional_id":"test_effigy","name":"Test Effigy",
         "types":["creature"],"subtypes":["Shapeshifter"],"mana_cost":"{2}{U}","colors":["blue"],
         "power":0,"toughness":0,
         "abilities":[{"type":"enters_as_copy","of":"any_creature"}]}
    ]"#;
    CardDatabase::from_json(json).expect("the inline transform catalog")
}

/// **CR 707.8**: copying a double-faced permanent uses the copiable values of the face
/// that is currently **up**. A copy of a transformed permanent is therefore the back
/// face — and, per **CR 712.8e**, its mana value is `0`, because a back face has no mana
/// cost of its own and the copy has no front face to borrow one from.
///
/// The copy is not itself two-faced: CR 707.8a's transforming token copy is not built,
/// and nothing here creates a token as a copy, so the copy stays on the face it copied.
#[test]
fn cr_707_8_a_copy_of_a_transformed_permanent_takes_the_face_that_is_up() {
    let db = transform_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        player.mana_pool.add_colorless(20);
        player.mana_pool.add(Color::Blue, 20);
        player.mana_pool.add(Color::White, 20);
    }

    let turncoat = place(&mut state, &db, "test_turncoat", PlayerId(0));
    // Front face up first: the copy would be a 2/2 Human with mana value 2.
    let front = cast(&state, &db, "test_effigy", PlayerId(0), Vec::new());
    let front = apply_action(
        &front,
        &Action::AnswerPermanent {
            chosen: Some(turncoat),
        },
        &db,
    );
    let front_copy = arrival(&state, &front);
    assert_eq!(name_of(&front, &db, front_copy), "Test Turncoat");
    assert_eq!(
        characteristics(&front, front_copy, &db).mana_cost,
        "{1}{W}",
        "a front face's mana cost is copied like any other printed value"
    );

    // Turn it over, then copy it again.
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: turncoat,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = settle(&state, &db, Target::Permanent(turncoat));
    assert_eq!(name_of(&state, &db, turncoat), "Test Werewolf");

    let before = cast(&state, &db, "test_effigy", PlayerId(0), Vec::new());
    let after = apply_action(
        &before,
        &Action::AnswerPermanent {
            chosen: Some(turncoat),
        },
        &db,
    );
    let copy = arrival(&before, &after);

    assert_eq!(name_of(&after, &db, copy), "Test Werewolf");
    assert_eq!(pt(&after, &db, copy), (Some(4), Some(4)));
    assert!(characteristics(&after, copy, &db)
        .keywords
        .contains(&Keyword::Trample));
    // CR 712.8e: mana value 0 — the back face has no mana cost, and the copy has no
    // front face of its own to take one from.
    assert_eq!(characteristics(&after, copy, &db).mana_cost, "");

    // Copying is not a status: the copy stays on the face it copied and its own card
    // still has only one face, so it never turns back into anything.
    let perm = after
        .battlefield
        .iter()
        .find(|p| p.id == copy)
        .expect("the copy is on the battlefield");
    assert_eq!(perm.printed.card(), Some(cid(&db, "test_effigy")));
}

/// CR 707.2b again, over the one thing that really does change an original's *copiable*
/// values: turning it over. The copy keeps the face it copied.
#[test]
fn cr_707_2b_the_original_transforming_does_not_change_the_copy() {
    let db = transform_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        player.mana_pool.add_colorless(20);
        player.mana_pool.add(Color::Blue, 20);
        player.mana_pool.add(Color::White, 20);
    }
    let turncoat = place(&mut state, &db, "test_turncoat", PlayerId(0));
    let before = cast(&state, &db, "test_effigy", PlayerId(0), Vec::new());
    let state = apply_action(
        &before,
        &Action::AnswerPermanent {
            chosen: Some(turncoat),
        },
        &db,
    );
    let copy = arrival(&before, &state);
    assert_eq!(name_of(&state, &db, copy), "Test Turncoat");

    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: turncoat,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = settle(&state, &db, Target::Permanent(turncoat));

    assert_eq!(name_of(&state, &db, turncoat), "Test Werewolf");
    assert_eq!(
        name_of(&state, &db, copy),
        "Test Turncoat",
        "the copy was fixed when it was made (CR 707.2b)"
    );
    assert_eq!(pt(&state, &db, copy), (Some(2), Some(2)));
}

// ----- Doublecast: copying a spell (CR 707.10) ------------------------------

/// The copy resolves, and it was **never cast**: a cast-watcher on the battlefield fires
/// once for the spell and not at all for its copy (CR 707.10). Guttersnipe is the
/// witness, because its trigger is exactly the question.
#[test]
fn cr_707_10_a_copied_spell_was_not_cast() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "guttersnipe", PlayerId(0));

    // Doublecast is itself an instant-or-sorcery cast, so the watcher fires for it too;
    // resolving everything here means the Shock step below measures only the Shock.
    let state = cast(&state, &db, "doublecast", PlayerId(0), Vec::new());
    let state = settle(&state, &db, Target::Player(PlayerId(1)));
    let opening = life(&state, PlayerId(1));

    let before = cast(
        &state,
        &db,
        "shock",
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );
    let after = settle(&before, &db, Target::Player(PlayerId(1)));

    // Guttersnipe's 2, Shock's 2, and the copy's 2 — six. A copy that counted as a cast
    // would be eight.
    assert_eq!(
        opening - life(&after, PlayerId(1)),
        6,
        "the copy dealt its damage and fired nothing"
    );

    // And the log says so outright: one cast event for the Shock, none for the copy.
    let casts = after
        .log
        .iter()
        .filter(|entry| {
            matches!(&entry.event, GameEvent::SpellCast { card, .. }
                if card.card == cid(&db, "shock"))
        })
        .count();
    assert_eq!(casts, 1, "the copy is not a cast");

    // Nothing extra reached a graveyard either: a copy has no card (CR 707.10a).
    let shocks = after.players[0]
        .graveyard
        .iter()
        .filter(|card| card.card == cid(&db, "shock"))
        .count();
    assert_eq!(shocks, 1, "one physical Shock, one graveyard entry");
}

/// CR 707.10c: the copy's controller may choose **new targets** for it. The copy reaches
/// the stack unaimed and is answered by the same action a triggered ability's targets
/// are, so the two halves of a Doublecast can hit two different players.
#[test]
fn cr_707_10c_the_copy_can_take_new_targets() {
    let db = db();
    let state = cast(&main_phase(), &db, "doublecast", PlayerId(0), Vec::new());
    let state = settle(&state, &db, Target::Player(PlayerId(1)));

    // Cast the Shock without passing it down, so the delayed ability can be inspected
    // where it lands: on the stack, above the spell it watched.
    let mut state = state;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let shock = to_hand(&mut state, &db, "shock", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    // The delayed ability is on the stack, already aimed at the spell it watched — its
    // controller was never asked which spell to copy.
    let aimed = state
        .stack
        .iter()
        .find(|object| matches!(object.kind, StackObjectKind::Ability { .. }))
        .expect("the delayed ability is on the stack");
    assert_eq!(
        aimed.targets.len(),
        1,
        "'that spell' was fixed by the event"
    );

    // Resolve it: the copy arrives owing targets, and its controller aims it at the other
    // seat rather than at the one the original named.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let copy = state
        .stack
        .iter()
        .find(|object| matches!(object.kind, StackObjectKind::SpellCopy { .. }))
        .expect("the copy is on the stack");
    assert!(copy.targets.is_empty(), "the copy is unaimed");
    let ability = pending_trigger_target_choice(&state).expect("the copy owes targets");
    assert_eq!(ability, copy.id);

    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Player(PlayerId(0))],
        },
        &db,
    );
    let after = settle(&state, &db, Target::Player(PlayerId(1)));

    assert_eq!(life(&after, PlayerId(0)), 18, "the copy hit its new target");
    assert_eq!(life(&after, PlayerId(1)), 18, "the original hit its own");
}

/// CR 603.7b: `when you **next** …` fires once. The second spell of the turn is not
/// copied, because the ability was spent on the first.
#[test]
fn cr_603_7b_the_delayed_trigger_fires_only_for_the_next_spell() {
    let db = db();
    let state = cast(&main_phase(), &db, "doublecast", PlayerId(0), Vec::new());
    let state = settle(&state, &db, Target::Player(PlayerId(1)));
    assert_eq!(state.delayed_triggers.len(), 1, "one ability is waiting");

    let state = cast(
        &state,
        &db,
        "shock",
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );
    let state = settle(&state, &db, Target::Player(PlayerId(1)));
    assert!(
        state.delayed_triggers.is_empty(),
        "firing spends it (CR 603.7b)"
    );
    assert_eq!(life(&state, PlayerId(1)), 16, "two Shocks' worth");

    let state = cast(
        &state,
        &db,
        "shock",
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );
    let state = settle(&state, &db, Target::Player(PlayerId(1)));
    assert_eq!(
        life(&state, PlayerId(1)),
        14,
        "the second Shock was not copied"
    );
}

/// `… this turn` lapses at the turn boundary, unfired — the same boundary that clears
/// every other per-turn record.
#[test]
fn cr_603_7b_the_delayed_trigger_expires_at_end_of_turn() {
    let db = db();
    let state = cast(&main_phase(), &db, "doublecast", PlayerId(0), Vec::new());
    let mut state = settle(&state, &db, Target::Player(PlayerId(1)));
    assert_eq!(state.delayed_triggers.len(), 1);

    let turn = state.turn;
    for _ in 0..64 {
        if state.turn != turn {
            break;
        }
        // A step that owes a turn-based declaration offers no pass at all, so the walk
        // takes whatever it is offered — an empty declaration, here.
        let offered = valid_actions(&state, &db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|action| *action != Action::Concede)
                .expect("something is on offer")
        };
        let next = apply_action(&state, &action, &db);
        assert_ne!(next, state, "the walk stalled in {:?}", state.step);
        state = next;
    }
    assert_ne!(state.turn, turn, "the turn boundary was crossed");
    assert!(
        state.delayed_triggers.is_empty(),
        "the ability did not survive its turn"
    );

    // And a spell cast on the new turn is not copied.
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 20);
        }
    }
    let opening = life(&state, PlayerId(1));
    let state = cast(
        &state,
        &db,
        "shock",
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );
    let state = settle(&state, &db, Target::Player(PlayerId(1)));
    assert_eq!(opening - life(&state, PlayerId(1)), 2, "one Shock, no copy");
}
