//! M19 cards whose whole content is a lord, a watcher, or a one-shot class effect —
//! the batch the ability IR already expressed before any of it was widened for this
//! set.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! A definition that parses is not evidence of anything: what has to be true is that
//! the card does what it says, so each test asserts on the resulting [`GameState`].
//! Cards are named by their authored `functional_id`, never by an interned handle
//! (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_trigger_target_choice, valid_actions, Action,
    CardDatabase, CardId, CardInstance, CardType, Color, FunctionalId, GameState, Keyword,
    Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game parked at player 0's precombat main with both pools stocked, so
/// payability never decides a test that is about an effect.
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

/// Cast `slug` from player 0's hand with `targets` and let it resolve.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
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
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    targets: Vec<Target>,
) -> GameState {
    let after = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    assert_ne!(&after, state, "the activation was rejected");
    let after = apply_action(&after, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

/// A permanent's current power/toughness through the layer system.
fn pt(state: &GameState, db: &CardDatabase, id: PermanentId) -> (i32, i32) {
    let c = characteristics(state, id, db);
    (c.power.unwrap_or(0), c.toughness.unwrap_or(0))
}

fn keywords(state: &GameState, db: &CardDatabase, id: PermanentId) -> Vec<Keyword> {
    characteristics(state, id, db).keywords
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

// ----- lords ----------------------------------------------------------------

/// Death Baron reads as one sentence and is four continuous effects: two classes
/// ("Skeletons you control", "other Zombies you control") crossed with two
/// modifications (+1/+1, deathtouch). Each is a separate `Ability::Static`, because a
/// selector names one class and a modification lands in one CR 613 layer.
#[test]
fn death_baron_pumps_skeletons_and_other_zombies_and_never_itself() {
    let db = db();
    let mut state = main_phase();
    let baron = place(&mut state, &db, "death_baron", PlayerId(0));
    // Reassembling Skeleton is a Skeleton Warrior; Walking Corpse is a Zombie.
    let skeleton = place(&mut state, &db, "walking_corpse", PlayerId(0));
    let opposing = place(&mut state, &db, "walking_corpse", PlayerId(1));

    // The Zombie half: +1/+1 and deathtouch on another Zombie its controller controls.
    assert_eq!(pt(&state, &db, skeleton), (3, 3));
    assert!(keywords(&state, &db, skeleton).contains(&Keyword::Deathtouch));

    // `except_this` keeps the lord off its own anthem — Death Baron is itself a
    // Zombie, and a 3/3 here would be a different card.
    assert_eq!(pt(&state, &db, baron), (2, 2));
    assert!(!keywords(&state, &db, baron).contains(&Keyword::Deathtouch));

    // And the class is "you control": an opponent's Zombie is untouched.
    assert_eq!(pt(&state, &db, opposing), (2, 2));
}

/// Supreme Phantom is the plain lord shape: other Spirits you control get +1/+1, and
/// the Phantom itself does not.
#[test]
fn supreme_phantom_pumps_other_spirits_only() {
    let db = db();
    let mut state = main_phase();
    let phantom = place(&mut state, &db, "supreme_phantom", PlayerId(0));
    let other = place(&mut state, &db, "supreme_phantom", PlayerId(0));
    // Two copies of one lord do pump each other: `except_this` compares the
    // permanent, not the card.
    assert_eq!(pt(&state, &db, phantom), (2, 4));
    assert_eq!(pt(&state, &db, other), (2, 4));

    // A non-Spirit creature is outside the class.
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    assert_eq!(pt(&state, &db, ogre), (4, 2));
}

/// Valiant Knight is a lord plus a mass keyword grant over the same tribe. The grant
/// is a one-shot that locks its set in on resolution, so a Knight that arrives later
/// in the turn does not gain double strike.
#[test]
fn valiant_knight_pumps_knights_and_grants_them_double_strike_until_end_of_turn() {
    let db = db();
    let mut state = main_phase();
    let valiant = place(&mut state, &db, "valiant_knight", PlayerId(0));
    let knight = place(&mut state, &db, "knight_of_the_tusk", PlayerId(0));

    assert_eq!(pt(&state, &db, knight), (4, 8), "the lord's +1/+1 applies");
    assert_eq!(pt(&state, &db, valiant), (3, 4), "and not to itself");

    let state = activate(&state, &db, valiant, 1, Vec::new());
    assert!(keywords(&state, &db, knight).contains(&Keyword::DoubleStrike));
    // The lord is itself a Knight, so the mass grant — which has no "other" — reaches
    // it even though the anthem does not.
    assert!(keywords(&state, &db, valiant).contains(&Keyword::DoubleStrike));

    // A Knight that arrives after the grant resolved is outside the locked-in set
    // (CR 611.2c).
    let mut later = state.clone();
    let latecomer = place(&mut later, &db, "knight_of_the_tusk", PlayerId(0));
    assert!(!keywords(&later, &db, latecomer).contains(&Keyword::DoubleStrike));
    assert_eq!(
        pt(&later, &db, latecomer),
        (4, 8),
        "the anthem still applies"
    );
}

// ----- watchers -------------------------------------------------------------

/// Open the Graves watches nontoken creatures its controller controls dying. The
/// `nontoken` filter is what keeps the Zombie it creates from feeding its own trigger.
#[test]
fn open_the_graves_makes_a_zombie_for_a_nontoken_death_and_not_for_its_own_token() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "open_the_graves", PlayerId(0));
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));

    // A lethal shock on the Ogre: the death is observed and the trigger resolves.
    let state = cast(&state, &db, "shock", vec![Target::Permanent(ogre)]);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!on_battlefield(&state, ogre));

    let zombies: Vec<&Permanent> = state
        .battlefield
        .iter()
        .filter(|p| p.printed.face(&db).map(|f| f.name()) == Some("Zombie") && p.printed.is_token())
        .collect();
    assert_eq!(zombies.len(), 1, "one nontoken death, one Zombie");
    let token = zombies[0].id;
    assert_eq!(zombies[0].controller, PlayerId(0));
    assert_eq!(pt(&state, &db, token), (2, 2));
}

/// Stitcher's Supplier mills on **either** of two events. They are two triggered
/// abilities, not one condition that means two things.
#[test]
fn stitchers_supplier_mills_on_entering_and_again_on_dying() {
    let db = db();
    let mut state = main_phase();
    for _ in 0..10 {
        let instance = state.new_instance(cid(&db, "forest"));
        state.players[0].library.push(instance);
    }
    let before = state.players[0].graveyard.len();

    let supplier = to_hand(&mut state, &db, "stitcher_s_supplier", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: supplier,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    // The creature resolved; its enters-the-battlefield trigger is on the stack.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(
        state.players[0].graveyard.len(),
        before + 3,
        "entering milled three"
    );

    let id = state
        .battlefield
        .iter()
        .find(|p| p.instance == supplier.id)
        .unwrap()
        .id;
    let after = cast(&state, &db, "shock", vec![Target::Permanent(id)]);
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(!on_battlefield(&after, id));
    // Three more milled, plus the Supplier's own card and the spent Shock.
    assert_eq!(after.players[0].graveyard.len(), before + 3 + 3 + 2);
}

/// Dragon Egg's death trigger creates a token that carries an **activated** ability of
/// its own — authored inline, because the creating effect is the token's printed face.
#[test]
fn dragon_egg_leaves_a_firebreathing_dragon_token_behind() {
    let db = db();
    let mut state = main_phase();
    let egg = place(&mut state, &db, "dragon_egg", PlayerId(0));

    let state = cast(
        &state,
        &db,
        "lightning_strike",
        vec![Target::Permanent(egg)],
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!on_battlefield(&state, egg));

    let dragon = state
        .battlefield
        .iter()
        .find(|p| p.printed.face(&db).map(|f| f.name()) == Some("Dragon") && p.printed.is_token())
        .expect("the Egg hatched")
        .id;
    assert_eq!(pt(&state, &db, dragon), (2, 2));
    assert!(keywords(&state, &db, dragon).contains(&Keyword::Flying));

    // Its firebreathing is a real activation, not decoration.
    let pumped = activate(&state, &db, dragon, 0, Vec::new());
    assert_eq!(pt(&pumped, &db, dragon), (3, 2));
}

// ----- intervening-if -------------------------------------------------------

/// Leonin Vanguard's begin-combat trigger is gated on an intervening if, judged as the
/// effect is reached rather than when the ability was put on the stack.
#[test]
fn leonin_vanguard_pumps_itself_only_with_three_creatures_out() {
    let db = db();
    let mut state = main_phase();
    let vanguard = place(&mut state, &db, "leonin_vanguard", PlayerId(0));
    state.step = Step::PrecombatMain;

    // Two creatures: the condition fails and nothing happens.
    place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let lone = walk_to_combat(&state, &db);
    assert_eq!(pt(&lone, &db, vanguard), (1, 1));
    assert_eq!(lone.players[0].life, 20);

    // A third creature turns the same trigger on.
    let mut three = state.clone();
    place(&mut three, &db, "onakke_ogre", PlayerId(0));
    let three = walk_to_combat(&three, &db);
    assert_eq!(pt(&three, &db, vanguard), (2, 2));
    assert_eq!(three.players[0].life, 21);
}

/// Pass priority until the beginning-of-combat trigger has resolved.
fn walk_to_combat(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..8 {
        if state.step == Step::DeclareAttackers {
            break;
        }
        let offered = valid_actions(&state, db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|a| a != &Action::Concede)
                .expect("some action")
        };
        state = apply_action(&state, &action, db);
    }
    state
}

/// Scholar of Stars draws only when its controller controls an artifact as the
/// conditional is reached.
#[test]
fn scholar_of_stars_draws_only_beside_an_artifact() {
    let db = db();
    let mut state = main_phase();
    for _ in 0..5 {
        let instance = state.new_instance(cid(&db, "forest"));
        state.players[0].library.push(instance);
    }
    let hand = state.players[0].hand.len();

    let bare = resolve_creature(&state, &db, "scholar_of_stars");
    assert_eq!(bare.players[0].hand.len(), hand, "no artifact, no card");

    let mut with_artifact = state.clone();
    place(&mut with_artifact, &db, "millstone", PlayerId(0));
    let drew = resolve_creature(&with_artifact, &db, "scholar_of_stars");
    assert_eq!(drew.players[0].hand.len(), hand + 1);
}

/// Cast `slug` as a creature spell and resolve it and its enters-the-battlefield
/// trigger.
fn resolve_creature(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let mut state = apply_action(
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
    for _ in 0..4 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

// ----- one-shot class effects ------------------------------------------------

/// Make a Stand is two class effects in one spell — neither of which targets, so both
/// reach every creature its controller controls and neither can fizzle.
#[test]
fn make_a_stand_pumps_and_shields_your_whole_board() {
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let theirs = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let state = cast(&state, &db, "make_a_stand", Vec::new());
    assert_eq!(pt(&state, &db, mine), (5, 2));
    assert!(keywords(&state, &db, mine).contains(&Keyword::Indestructible));
    assert_eq!(pt(&state, &db, theirs), (4, 2), "not your opponent's");
    assert!(!keywords(&state, &db, theirs).contains(&Keyword::Indestructible));
}

/// Uncomfortable Chill is the mirror class — the opponents' creatures — and a draw.
#[test]
fn uncomfortable_chill_shrinks_the_opposing_board_and_replaces_itself() {
    let db = db();
    let mut state = main_phase();
    let instance = state.new_instance(cid(&db, "forest"));
    state.players[0].library.push(instance);
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let theirs = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let hand = state.players[0].hand.len();

    let state = cast(&state, &db, "uncomfortable_chill", Vec::new());
    assert_eq!(pt(&state, &db, theirs), (2, 2));
    assert_eq!(pt(&state, &db, mine), (4, 2));
    assert_eq!(state.players[0].hand.len(), hand + 1);
}

// ----- the rest of the batch, asserted where their content is -----------------

/// Cavalry Drillmaster's enters-the-battlefield trigger is one effect that both pumps
/// and grants, so a player cannot pump one creature while another gains first strike.
#[test]
fn cavalry_drillmaster_pumps_and_grants_first_strike_to_one_creature() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));

    let mut state = state.clone();
    let instance = to_hand(&mut state, &db, "cavalry_drillmaster", PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    // Resolve the creature, then aim and resolve the trigger it put on the stack.
    state = apply_action(&state, &Action::PassPriority, &db);
    state = apply_action(&state, &Action::PassPriority, &db);
    let ability = pending_trigger_target_choice(&state).expect("the trigger owes a target");
    state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(ogre)],
        },
        &db,
    );
    state = apply_action(&state, &Action::PassPriority, &db);
    state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(pt(&state, &db, ogre), (6, 2));
    assert!(keywords(&state, &db, ogre).contains(&Keyword::FirstStrike));
}

/// Mystic Archaeologist's whole content is a repeatable draw; Fiery Finish's is seven
/// damage. Both are asserted through the pipeline rather than read off the data.
#[test]
fn mystic_archaeologist_draws_two_and_fiery_finish_burns_for_seven() {
    let db = db();
    let mut state = main_phase();
    for _ in 0..5 {
        let instance = state.new_instance(cid(&db, "forest"));
        state.players[0].library.push(instance);
    }
    let archaeologist = place(&mut state, &db, "mystic_archaeologist", PlayerId(0));
    let hand = state.players[0].hand.len();
    let drawn = activate(&state, &db, archaeologist, 0, Vec::new());
    assert_eq!(drawn.players[0].hand.len(), hand + 2);

    // Seven damage kills a 7/7-or-smaller creature outright.
    let wurm = place(&mut state, &db, "pelakka_wurm", PlayerId(1));
    let burned = cast(&state, &db, "fiery_finish", vec![Target::Permanent(wurm)]);
    let burned = apply_action(&burned, &Action::PassPriority, &db);
    let burned = apply_action(&burned, &Action::PassPriority, &db);
    assert!(!on_battlefield(&burned, wurm), "a 7/7 dies to seven damage");
}

/// Manalith's mana is a colour its controller chooses as it resolves, one point at a
/// time — the ability is a mana ability, so it never uses the stack.
#[test]
fn manalith_taps_for_a_colour_of_its_controllers_choosing() {
    let db = db();
    let mut state = main_phase();
    state.players[0].mana_pool = Default::default();
    let manalith = place(&mut state, &db, "manalith", PlayerId(0));

    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: manalith,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        state.stack.is_empty(),
        "a mana ability never uses the stack"
    );
    let state = apply_action(&state, &Action::AnswerColor { color: Color::Blue }, &db);
    assert_eq!(state.players[0].mana_pool.color_amount(Color::Blue), 1);
}

/// Mist-Cloaked Herald's evasion is a printed combat restriction, not a keyword.
#[test]
fn mist_cloaked_herald_cannot_be_blocked() {
    let db = db();
    let db_ref = &db;
    let data = db_ref
        .card(cid(db_ref, "mist_cloaked_herald"))
        .expect("bundled");
    assert!(data.has_type(CardType::Creature));
    assert!(
        data.keywords.is_empty(),
        "it is a restriction, not a keyword"
    );
    assert_eq!(
        data.restrictions,
        vec![sage_engine::CombatRestriction::CantBeBlocked]
    );
}

/// Dryad Greenseeker looks at one card and takes it only if it is a land — the
/// filtered look, at its smallest.
#[test]
fn dryad_greenseeker_takes_the_top_card_only_when_it_is_a_land() {
    let db = db();
    let mut state = main_phase();
    let greenseeker = place(&mut state, &db, "dryad_greenseeker", PlayerId(0));

    // A land on top: it is offered and taken.
    let mut with_land = state.clone();
    let land = with_land.new_instance(cid(&db, "forest"));
    with_land.players[0].library.push(land);
    let hand = with_land.players[0].hand.len();
    let after = apply_action(
        &with_land,
        &Action::ActivateAbility {
            permanent: greenseeker,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(
        &after,
        &Action::AnswerChoice {
            chosen: vec![land.id],
        },
        &db,
    );
    assert_eq!(after.players[0].hand.len(), hand + 1);

    // A nonland on top: the question has no legal answer, so it is never asked and the
    // ability resolves having taken nothing.
    let mut with_spell = state.clone();
    let spell = with_spell.new_instance(cid(&db, "shock"));
    with_spell.players[0].library.push(spell);
    let hand = with_spell.players[0].hand.len();
    let after = apply_action(
        &with_spell,
        &Action::ActivateAbility {
            permanent: greenseeker,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(after.players[0].hand.len(), hand);
    assert!(after.pending_choices.is_empty());
}
