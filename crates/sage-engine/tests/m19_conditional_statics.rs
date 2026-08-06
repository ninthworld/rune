//! Continuous abilities with an `as long as …` clause.
//!
//! The point of every test here is that the clause is re-asked on **every read** rather
//! than remembered from a previous one: the modification appears the instant the
//! condition becomes true and disappears the instant it stops, with nothing stored and
//! nothing to prune.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, Action, Attack, AttackTarget, CardDatabase, CardId, Color,
    FunctionalId, GameState, Keyword, Permanent, PermanentId, PlayerId, Step, Target,
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

fn pt(state: &GameState, db: &CardDatabase, id: PermanentId) -> (i32, i32) {
    let c = characteristics(state, id, db);
    (c.power.unwrap_or(0), c.toughness.unwrap_or(0))
}

fn keywords(state: &GameState, db: &CardDatabase, id: PermanentId) -> Vec<Keyword> {
    characteristics(state, id, db).keywords
}

/// Remove `id` from the battlefield the way an exile would, so the condition stops
/// holding without any other state changing.
fn remove(state: &mut GameState, id: PermanentId) {
    state.battlefield.retain(|p| p.id != id);
}

/// Gearsmith Prodigy's clause turns on and off with the artifact, and the artifact does
/// not have to be the one that was there when it entered.
#[test]
fn gearsmith_prodigy_grows_only_while_you_control_an_artifact() {
    let db = db();
    let mut state = main_phase();
    let prodigy = place(&mut state, &db, "gearsmith_prodigy", PlayerId(0));
    assert_eq!(pt(&state, &db, prodigy), (1, 2), "bare, it is printed size");

    let millstone = place(&mut state, &db, "millstone", PlayerId(0));
    assert_eq!(pt(&state, &db, prodigy), (2, 2));

    // A second artifact is not additive: the clause is a yes-or-no, not a count.
    let second = place(&mut state, &db, "millstone", PlayerId(0));
    assert_eq!(pt(&state, &db, prodigy), (2, 2));

    remove(&mut state, second);
    assert_eq!(pt(&state, &db, prodigy), (2, 2), "one is still enough");
    remove(&mut state, millstone);
    assert_eq!(pt(&state, &db, prodigy), (1, 2), "and none is not");
}

/// An opponent's artifact is not yours: the clause is controller-relative, exactly as
/// every other selector in the engine is.
#[test]
fn gearsmith_prodigy_does_not_count_an_opponents_artifact() {
    let db = db();
    let mut state = main_phase();
    let prodigy = place(&mut state, &db, "gearsmith_prodigy", PlayerId(0));
    place(&mut state, &db, "millstone", PlayerId(1));
    assert_eq!(pt(&state, &db, prodigy), (1, 2));
}

/// Aerial Engineer is one clause governing two modifications, authored as two static
/// abilities because a modification lands in one CR 613 layer.
#[test]
fn aerial_engineer_gains_both_halves_of_its_clause_together() {
    let db = db();
    let mut state = main_phase();
    let engineer = place(&mut state, &db, "aerial_engineer", PlayerId(0));
    assert_eq!(pt(&state, &db, engineer), (2, 4));
    assert!(!keywords(&state, &db, engineer).contains(&Keyword::Flying));

    let artifact = place(&mut state, &db, "millstone", PlayerId(0));
    assert_eq!(pt(&state, &db, engineer), (4, 4));
    assert!(keywords(&state, &db, engineer).contains(&Keyword::Flying));

    remove(&mut state, artifact);
    assert_eq!(pt(&state, &db, engineer), (2, 4));
    assert!(!keywords(&state, &db, engineer).contains(&Keyword::Flying));
}

/// Gearsmith Guardian's clause names a **colour**, read off the printed colour
/// indicator: an artifact creature with no colour is not a blue creature.
#[test]
fn gearsmith_guardian_wants_a_blue_creature_specifically() {
    let db = db();
    let mut state = main_phase();
    let guardian = place(&mut state, &db, "gearsmith_guardian", PlayerId(0));
    assert_eq!(pt(&state, &db, guardian), (3, 5));

    // Skyscanner is a colourless artifact creature — it does not satisfy the clause.
    let colorless = place(&mut state, &db, "skyscanner", PlayerId(0));
    assert_eq!(pt(&state, &db, guardian), (3, 5));
    remove(&mut state, colorless);

    // Tolarian Scholar is blue.
    place(&mut state, &db, "tolarian_scholar", PlayerId(0));
    assert_eq!(pt(&state, &db, guardian), (5, 5));
}

/// A planeswalker clause names both the type and the subtype, so the wrong
/// planeswalker does not satisfy it.
#[test]
fn court_cleric_grows_beside_an_ajani_and_beside_no_other_planeswalker() {
    let db = db();
    let mut state = main_phase();
    let cleric = place(&mut state, &db, "court_cleric", PlayerId(0));
    assert_eq!(pt(&state, &db, cleric), (1, 1));
    assert!(keywords(&state, &db, cleric).contains(&Keyword::Lifelink));

    let liliana = place(&mut state, &db, "liliana_untouched_by_death", PlayerId(0));
    assert_eq!(pt(&state, &db, cleric), (1, 1), "a Liliana is not an Ajani");
    remove(&mut state, liliana);

    place(&mut state, &db, "ajani_adversary_of_tyrants", PlayerId(0));
    assert_eq!(pt(&state, &db, cleric), (2, 2));
}

/// Arisen Gorgon and Tezzeret's Strider are the keyword-granting shape of the same
/// clause.
#[test]
fn a_planeswalker_clause_grants_a_keyword_too() {
    let db = db();
    let mut state = main_phase();
    let gorgon = place(&mut state, &db, "arisen_gorgon", PlayerId(0));
    let strider = place(&mut state, &db, "tezzeret_s_strider", PlayerId(0));
    assert!(!keywords(&state, &db, gorgon).contains(&Keyword::Deathtouch));
    assert!(!keywords(&state, &db, strider).contains(&Keyword::Menace));

    place(&mut state, &db, "liliana_untouched_by_death", PlayerId(0));
    assert!(keywords(&state, &db, gorgon).contains(&Keyword::Deathtouch));
    assert!(
        !keywords(&state, &db, strider).contains(&Keyword::Menace),
        "a Liliana is not a Tezzeret"
    );

    place(&mut state, &db, "tezzeret_artifice_master", PlayerId(0));
    assert!(keywords(&state, &db, strider).contains(&Keyword::Menace));
}

/// Kargan Dragonrider's clause names a subtype and no card type, so any Dragon you
/// control satisfies it — including a Dragon token.
#[test]
fn kargan_dragonrider_flies_beside_a_dragon() {
    let db = db();
    let mut state = main_phase();
    let rider = place(&mut state, &db, "kargan_dragonrider", PlayerId(0));
    assert!(!keywords(&state, &db, rider).contains(&Keyword::Flying));

    place(&mut state, &db, "volcanic_dragon", PlayerId(0));
    assert!(keywords(&state, &db, rider).contains(&Keyword::Flying));
}

/// Grasping Scoundrel's clause is about the source's own combat state, so it turns on
/// at the declaration and off when the permanent leaves combat.
#[test]
fn grasping_scoundrel_is_bigger_only_while_attacking() {
    let db = db();
    let mut state = main_phase();
    let scoundrel = place(&mut state, &db, "grasping_scoundrel", PlayerId(0));
    assert_eq!(pt(&state, &db, scoundrel), (1, 1));

    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    let attacking = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: scoundrel,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    assert_eq!(pt(&attacking, &db, scoundrel), (2, 1));

    // Its controller's *other* creature attacking does nothing for it: the clause is
    // about this permanent.
    let mut bystander = state.clone();
    let ogre = place(&mut bystander, &db, "onakke_ogre", PlayerId(0));
    bystander.step = Step::DeclareAttackers;
    bystander.priority = PlayerId(0);
    let bystander = apply_action(
        &bystander,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    assert_eq!(pt(&bystander, &db, scoundrel), (1, 1));
}

/// A conditional static ability is not a one-shot: it has no duration to expire and no
/// entry in `static_effects` to prune, so nothing about it survives its condition.
#[test]
fn a_conditional_static_stores_nothing() {
    let db = db();
    let mut state = main_phase();
    let prodigy = place(&mut state, &db, "gearsmith_prodigy", PlayerId(0));
    place(&mut state, &db, "millstone", PlayerId(0));
    assert_eq!(pt(&state, &db, prodigy), (2, 2));
    assert!(
        state.static_effects.is_empty(),
        "a printed static ability is derived on read, never stored"
    );

    // And a shock that kills the artifact takes the bonus with it, through the ordinary
    // pipeline rather than any cleanup of the ability's own.
    let millstone = state
        .battlefield
        .iter()
        .find(|p| p.printed.face(&db).map(|f| f.name()) == Some("Millstone"))
        .unwrap()
        .id;
    let instance = state.new_instance(cid(&db, "naturalize"));
    state.players[0].hand.push(instance);
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(millstone)],
            payment: Vec::new(),
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(pt(&after, &db, prodigy), (1, 2));
}
