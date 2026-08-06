//! Costs that spend the source, and the classes an effect names without targeting:
//! sacrifice and counter-removal activation costs, the attacking-creatures class, the
//! printed-colour card filter, and the two one-sided permanent target specs.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! A cost is only paid if the offer and the charge agree about it, so each of these
//! asserts on both: what [`valid_actions`] offers, and what the state looks like after.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_trigger_target_choice, target_requirements,
    valid_actions, Action, Attack, CardDatabase, CardId, CardInstance, Color, CounterKind,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

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

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
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

fn offers(state: &GameState, db: &CardDatabase, permanent: PermanentId, index: usize) -> bool {
    valid_actions(state, db).contains(&Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
        payment: Vec::new(),
    })
}

fn pt(state: &GameState, db: &CardDatabase, id: PermanentId) -> (i32, i32) {
    let c = characteristics(state, id, db);
    (c.power.unwrap_or(0), c.toughness.unwrap_or(0))
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

// ----- sacrifice as a cost --------------------------------------------------

/// Explosive Apparatus spends itself to pay for its own ability. The two halves of
/// that are independent (CR 113.7a): the artifact is in the graveyard before the
/// ability resolves, and the ability resolves anyway.
#[test]
fn explosive_apparatus_sacrifices_itself_and_the_ability_still_resolves() {
    let db = db();
    let mut state = main_phase();
    let apparatus = place(&mut state, &db, "explosive_apparatus", PlayerId(0));
    let target = place(&mut state, &db, "walking_corpse", PlayerId(1));

    assert!(offers(&state, &db, apparatus, 0));
    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: apparatus,
            index: 0,
            targets: vec![Target::Permanent(target)],
            payment: Vec::new(),
        },
        &db,
    );
    // The cost is paid on activation: the source is already gone while its ability
    // waits on the stack.
    assert!(!on_battlefield(&after, apparatus));
    assert_eq!(after.stack.len(), 1);
    assert_eq!(
        after.players[0].graveyard.len(),
        1,
        "a sacrifice is a real trip to the graveyard"
    );

    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(!on_battlefield(&after, target), "two damage killed the 2/2");
}

/// Catalyst Elemental's whole ability is a mana verb, so it is a mana ability
/// (CR 605.1a) even though its cost sacrifices the source: it resolves immediately,
/// off the stack, and the pool gains {R}{R}.
#[test]
fn catalyst_elemental_is_a_mana_ability_that_eats_itself() {
    let db = db();
    let mut state = main_phase();
    state.players[0].mana_pool = Default::default();
    let elemental = place(&mut state, &db, "catalyst_elemental", PlayerId(0));

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: elemental,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        after.stack.is_empty(),
        "a mana ability never uses the stack"
    );
    assert!(!on_battlefield(&after, elemental));
    assert_eq!(after.players[0].mana_pool.color_amount(Color::Red), 2);
}

/// Sacrificing is a death (CR 701.17b): a watcher that observes creatures its
/// controller controls dying sees a creature sacrificed to its own cost.
#[test]
fn a_creature_sacrificed_to_a_cost_triggers_a_death_watcher() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "open_the_graves", PlayerId(0));
    let elemental = place(&mut state, &db, "catalyst_elemental", PlayerId(0));

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: elemental,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    // The mana ability itself used no stack; the death it caused put a trigger there.
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(
        after.battlefield.iter().any(
            |p| p.printed.is_token() && p.printed.face(&db).map(|f| f.name()) == Some("Zombie")
        ),
        "Open the Graves saw the sacrifice"
    );
}

// ----- counters as a cost ---------------------------------------------------

/// Dragon's Hoard banks a gold counter per Dragon and spends one per card. The offer
/// and the charge agree: with no counter the draw is not offered at all.
#[test]
fn dragons_hoard_banks_a_gold_counter_per_dragon_and_spends_one_per_card() {
    let db = db();
    let mut state = main_phase();
    for _ in 0..5 {
        let instance = state.new_instance(cid(&db, "forest"));
        state.players[0].library.push(instance);
    }
    let hoard = place(&mut state, &db, "dragon_s_hoard", PlayerId(0));

    // Ability 1 is the draw; with no gold counter it is unpayable and unoffered.
    assert!(!offers(&state, &db, hoard, 1));

    // A Dragon entering banks one.
    let mut with_dragon = state.clone();
    let dragon = to_hand(&mut with_dragon, &db, "volcanic_dragon", PlayerId(0));
    let mut state = apply_action(
        &with_dragon,
        &Action::CastSpell {
            card: dragon,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    for _ in 0..4 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    let counters = |s: &GameState| {
        s.battlefield
            .iter()
            .find(|p| p.id == hoard)
            .map_or(0, |p| p.counter_count(CounterKind::Gold))
    };
    assert_eq!(counters(&state), 1, "one Dragon, one gold counter");

    // Now the draw is offered, and taking it spends the counter and taps the Hoard.
    assert!(offers(&state, &db, hoard, 1));
    let hand = state.players[0].hand.len();
    let drawn = activate(&state, &db, hoard, 1, Vec::new());
    assert_eq!(drawn.players[0].hand.len(), hand + 1);
    assert_eq!(counters(&drawn), 0);
    assert!(drawn.battlefield.iter().any(|p| p.id == hoard && p.tapped));
    assert!(!offers(&drawn, &db, hoard, 1), "and not a second time");
}

// ----- classes and specs ----------------------------------------------------

/// Trumpet Blast names the attacking creatures, whoever controls them, and locks the
/// class in on resolution (CR 611.2c).
#[test]
fn trumpet_blast_pumps_every_attacker_and_nothing_else() {
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let home = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let theirs = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    // Out of combat the class is empty, and the spell is a legal but idle cast.
    let idle = cast(&state, &db, "trumpet_blast", Vec::new());
    assert_eq!(pt(&idle, &db, mine), (4, 2));

    // Declare one attacker, then cast: only the attacker is in the class.
    let mut combat = state.clone();
    combat.step = Step::DeclareAttackers;
    combat.priority = PlayerId(0);
    let combat = apply_action(
        &combat,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: mine,
                defender: sage_engine::AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    let after = cast(&combat, &db, "trumpet_blast", Vec::new());
    assert_eq!(pt(&after, &db, mine), (6, 2), "the attacker is pumped");
    assert_eq!(pt(&after, &db, home), (4, 2), "the one at home is not");
    assert_eq!(pt(&after, &db, theirs), (4, 2), "nor the defender's");
}

/// Meteor Golem's removal is one-sided: its controller's own permanents are never
/// candidates, and a land is never a candidate either.
#[test]
fn meteor_golem_can_only_aim_at_a_nonland_permanent_an_opponent_controls() {
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let their_creature = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let their_land = place(&mut state, &db, "forest", PlayerId(1));

    let state = resolve_creature(&state, &db, "meteor_golem");
    let ability = pending_trigger_target_choice(&state).expect("the ETB owes a target");
    let slots = target_requirements(
        &state,
        &db,
        &Action::ChooseTriggerTargets {
            ability,
            targets: Vec::new(),
        },
    );
    assert_eq!(slots.len(), 1);
    let candidates = &slots[0].candidates;
    assert!(candidates.contains(&Target::Permanent(their_creature)));
    assert!(
        !candidates.contains(&Target::Permanent(mine)),
        "not your own"
    );
    assert!(
        !candidates.contains(&Target::Permanent(their_land)),
        "not a land"
    );

    // And aiming it really destroys the creature.
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(their_creature)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!on_battlefield(&state, their_creature));
}

/// Cast  as a creature spell and resolve it, leaving any trigger it put on the
/// stack still owing its targets.
fn resolve_creature(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// Aethershield Artificer's begin-combat trigger names an artifact creature, so an
/// ordinary creature is not a candidate.
#[test]
fn aethershield_artificer_shields_only_an_artifact_creature_you_control() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "aethershield_artificer", PlayerId(0));
    let flesh = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let metal = place(&mut state, &db, "skyscanner", PlayerId(0));
    let theirs = place(&mut state, &db, "skyscanner", PlayerId(1));

    let state = walk_to_combat(&state, &db);
    let ability = pending_trigger_target_choice(&state).expect("the trigger owes a target");
    let slots = target_requirements(
        &state,
        &db,
        &Action::ChooseTriggerTargets {
            ability,
            targets: Vec::new(),
        },
    );
    let candidates = &slots[0].candidates;
    assert!(candidates.contains(&Target::Permanent(metal)));
    assert!(
        !candidates.contains(&Target::Permanent(flesh)),
        "not a bare creature"
    );
    assert!(
        !candidates.contains(&Target::Permanent(theirs)),
        "not an opponent's"
    );
}

/// Pass priority until the beginning-of-combat trigger is on the stack.
fn walk_to_combat(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..8 {
        if pending_trigger_target_choice(&state).is_some() || state.step == Step::DeclareAttackers {
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

/// The colour filter reads a card's printed colour indicator. Skalla Wolf may take a
/// green card off the top five and nothing else.
#[test]
fn skalla_wolf_reveals_a_green_card_and_only_a_green_one() {
    let db = db();
    let mut state = main_phase();
    // Bottom-to-top: the top five are the last five pushed.
    for slug in ["shock", "shock", "shock", "shock", "llanowar_elves"] {
        let instance = state.new_instance(cid(&db, slug));
        state.players[0].library.push(instance);
    }
    let hand = state.players[0].hand.len();

    let mut state = state.clone();
    let wolf = to_hand(&mut state, &db, "skalla_wolf", PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: wolf,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    for _ in 0..4 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    let pending = sage_engine::pending_player_choice(&state).expect("the look is owed");
    let request = pending.question.cards().expect("a card selection");
    let candidates = sage_engine::choice_candidates(&state, request, &db);
    assert_eq!(
        candidates.len(),
        1,
        "only the one green card among the top five is takeable"
    );
    let green = candidates[0].id;
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![green],
        },
        &db,
    );
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert!(state.players[0].hand.iter().any(|c| c.id == green));
}

/// Ajani's Influence is two effects and one target slot: the counters target, the look
/// does not.
#[test]
fn ajanis_influence_counters_one_creature_then_digs_for_white() {
    let db = db();
    let mut state = main_phase();
    for slug in ["shock", "shock", "shock", "shock", "daybreak_chaplain"] {
        let instance = state.new_instance(cid(&db, slug));
        state.players[0].library.push(instance);
    }
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));

    let mut state = state.clone();
    let spell = to_hand(&mut state, &db, "ajani_s_influence", PlayerId(0));
    let requirements = target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card: spell,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    assert_eq!(requirements.len(), 1, "one slot: the counters'");

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            targets: vec![Target::Permanent(ogre)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(pt(&state, &db, ogre), (6, 4), "two +1/+1 counters");

    let pending = sage_engine::pending_player_choice(&state).expect("the look is owed");
    let request = pending.question.cards().expect("a card selection");
    let candidates = sage_engine::choice_candidates(&state, request, &db);
    assert_eq!(candidates.len(), 1, "one white card among the top five");
}

/// A begin-combat trigger with no legal target is never put on the stack (CR 603.3c),
/// so Aethershield Artificer alone on the board simply does nothing.
#[test]
fn aethershield_artificer_withholds_its_trigger_with_no_artifact_creature_out() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "aethershield_artificer", PlayerId(0));
    place(&mut state, &db, "onakke_ogre", PlayerId(0));

    for _ in 0..8 {
        if state.step == Step::DeclareAttackers {
            break;
        }
        let offered = valid_actions(&state, &db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|a| a != &Action::Concede)
                .expect("some action")
        };
        state = apply_action(&state, &action, &db);
    }
    assert!(pending_trigger_target_choice(&state).is_none());
    assert!(state.stack.is_empty());
}
