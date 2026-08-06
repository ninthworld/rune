//! Prevented damage was **never dealt** (CR 615.1), asked of the one fact that reaches
//! back further than any window: `Permanent::dealt_damage`, and the hexproof
//! Palladia-Mors, the Ruiner keeps for exactly as long as it holds.
//!
//! This is the seam between two things that were built apart — the static condition of
//! issue #727 and the prevention shield of issue #736 — and the rule that joins them is a
//! subtraction rather than an addition: a shield does not undo the damage, it stops the
//! damage from having happened, so nothing downstream of it may record that it did. The
//! flag is one of those downstream records, alongside the marked damage, the life loss,
//! the deathtouch mark, and the lifelink gain that `damage_prevention.rs` already pins.
//!
//! Every seam that writes the flag is exercised the same way, because a permanent is the
//! *source* of damage in three places and each has to reach the same answer: the
//! combat-damage batch, and a fight (the one non-combat damage whose source is a
//! permanent). Each test carries its own **control** — the identical action with no
//! shield up — so an assertion about an absence can never pass by the machinery simply
//! being silent.
//!
//! Everything drives the real [`apply_action`]. Cards are named by their authored
//! `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, target_requirements, valid_actions, Action, Attack,
    AttackTarget, CardDatabase, CardId, Color, DamageFilter, FunctionalId, GameState, Keyword,
    Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk a whole turn; a walk that has not arrived by then is a hang.
const ACTION_CAP: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at the start of turn 1, both libraries stocked so the walk never
/// trips the CR 704.5c decking loss.
fn fresh_game(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let library: Vec<_> = (0..12)
            .map(|_| state.new_instance(cid(db, "colossal_dreadmaw")))
            .collect();
        state.players[seat].library = library;
    }
    state
}

/// Put a permanent of `slug` on the battlefield under `controller`, entered on turn 0 so
/// it may attack from turn 1 (CR 302.6).
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

/// Fill `seat`'s pool with enough of every colour to cast anything below. The pool empties
/// at each step boundary (CR 500.4), so the cast has to happen in the same step.
fn with_mana(state: &mut GameState, seat: PlayerId) {
    for color in Color::ALL {
        state.players[seat.0].mana_pool.add(color, 10);
    }
    state.players[seat.0].mana_pool.add_colorless(10);
}

fn walk_action(state: &GameState) -> Action {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Action::DeclareAttackers {
            attackers: Vec::new(),
        },
        Step::DeclareBlockers if !state.blockers_declared => {
            Action::DeclareBlockers { blocks: Vec::new() }
        }
        _ => Action::PassPriority,
    }
}

fn walk_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..ACTION_CAP {
        if done(&state) {
            return state;
        }
        let next = apply_action(&state, &walk_action(&state), db);
        assert_ne!(next, state, "the walk made no progress");
        state = next;
    }
    panic!("the walk ran past its cap without reaching the goal");
}

/// Cast `slug` from `seat` at `targets` and let it resolve. The card is minted straight
/// into hand and the pool is stocked: what is under test is what the resolution does, not
/// which land paid which pip.
fn cast(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    with_mana(&mut state, seat);
    let card = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(card);
    state.priority = seat;
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    assert!(!state.stack.is_empty(), "{slug} did not reach the stack");
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Whether `id` currently has hexproof, read the way every rule reads a keyword: through
/// the computed characteristics (CR 613.1f), fresh on this call.
fn has_hexproof(state: &GameState, db: &CardDatabase, id: PermanentId) -> bool {
    characteristics(state, id, db)
        .keywords
        .contains(&Keyword::Hexproof)
}

/// The permanents seat 1 is offered a Murder against — the observable half of hexproof
/// (CR 702.11b), asked through the same [`valid_actions`] a client reads.
///
/// The same witness `m19_has_not_dealt_damage.rs` uses, and for the same reason: a
/// keyword nobody can act on would not be a keyword. Every caller below asserts against a
/// *control* creature in the same list, so an empty answer can never pass for hexproof.
fn murder_targets(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let mut state = state.clone();
    state.step = Step::PostcombatMain;
    state.stack.clear();
    state.priority = PlayerId(1);
    with_mana(&mut state, PlayerId(1));
    let murder = state.new_instance(cid(db, "murder"));
    state.players[1].hand.push(murder);
    let cast = valid_actions(&state, db)
        .into_iter()
        .find(|action| matches!(action, Action::CastSpell { card, .. } if card.id == murder.id))
        .expect("seat 1 can afford the Murder in its hand");
    target_requirements(&state, db, &cast)
        .into_iter()
        .flat_map(|requirement| requirement.candidates)
        .filter_map(|target| match target {
            Target::Permanent(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// Seat 0 attacks seat 1 with `attacker` on turn 1 and the game runs into the
/// combat-damage step, where the turn-based damage assignment has landed.
fn attack_into_damage(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let state = walk_until(state, db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    walk_until(&state, db, |s| s.step == Step::CombatDamage)
}

/// The board every combat test below starts from: Palladia-Mors under seat 0 and an
/// ordinary creature beside it, parked at seat 0's precombat main on turn 1.
fn dragon_and_control(db: &CardDatabase) -> (GameState, PermanentId, PermanentId) {
    let mut state = fresh_game(db);
    let dragon = place(&mut state, db, "palladia_mors_the_ruiner", PlayerId(0));
    let ogre = place(&mut state, db, "onakke_ogre", PlayerId(0));
    let state = walk_until(&state, db, |s| s.turn == 1 && s.step == Step::PrecombatMain);
    (state, dragon, ogre)
}

// ----- combat damage a shield stopped ---------------------------------------

/// **The crux.** Root Snare prevents the whole of the attack, so Palladia-Mors has not
/// dealt damage — and the hexproof its static ability grants while that is true is still
/// there afterwards, aimable-at by nobody.
///
/// The control is the same attack with no shield up, computed from the same base state:
/// there the damage lands, the condition stops holding, and the removal an opponent could
/// not aim a moment earlier is offered. One of the two answers has to be each way, or the
/// test is measuring nothing.
#[test]
fn cr_615_1_prevented_combat_damage_leaves_palladia_mors_hexproof() {
    let db = db();
    let (base, dragon, ogre) = dragon_and_control(&db);

    let unshielded = attack_into_damage(&base, &db, dragon);
    assert_eq!(
        unshielded.players[1].life, 14,
        "the control: six damage from a 6/6 landed"
    );
    assert!(
        !has_hexproof(&unshielded, &db, dragon),
        "and having dealt it, the condition stops holding"
    );
    assert!(
        murder_targets(&unshielded, &db).contains(&dragon),
        "so an opponent's removal may be aimed at it"
    );

    let shielded = cast(&base, &db, "root_snare", PlayerId(0), Vec::new());
    assert_eq!(shielded.prevention.len(), 1, "the shield is up");
    let shielded = attack_into_damage(&shielded, &db, dragon);

    assert_eq!(
        shielded.players[1].life, 20,
        "the shield prevented every point of it (CR 615.1)"
    );
    assert!(
        has_hexproof(&shielded, &db, dragon),
        "damage that was never dealt is damage this permanent has not dealt yet"
    );
    let offered = murder_targets(&shielded, &db);
    assert!(
        offered.contains(&ogre),
        "the control creature is still a legal target, so the offer machinery is awake"
    );
    assert!(
        !offered.contains(&dragon),
        "and the hexproof still keeps that spell off the dragon (CR 702.11b)"
    );
}

/// A shield the attacker's own controller did not raise makes no difference: the shield
/// belongs to nobody, and the question is only whether damage was dealt. Seat 1 — the
/// player being attacked — casts it here.
#[test]
fn cr_615_1_it_does_not_matter_who_raised_the_shield() {
    let db = db();
    let (base, dragon, _) = dragon_and_control(&db);

    let shielded = cast(&base, &db, "root_snare", PlayerId(1), Vec::new());
    let shielded = attack_into_damage(&shielded, &db, dragon);

    assert_eq!(shielded.players[1].life, 20);
    assert!(
        has_hexproof(&shielded, &db, dragon),
        "the defender's shield prevents the damage just as well"
    );
}

/// The flag is one-way and lifelong, and prevention does not turn that into a
/// *rewind*: the turn boundary neither takes the hexproof away from the dragon whose
/// damage was prevented nor gives it back to the one whose damage landed.
///
/// Both directions in one test, because the failure that matters is the pair disagreeing
/// — a shield that merely deferred the record would show up as the two answers converging
/// once the shield expired.
#[test]
fn cr_615_1_the_answer_survives_the_turn_boundary_in_both_directions() {
    let db = db();
    let (base, dragon, _) = dragon_and_control(&db);

    let landed = attack_into_damage(&base, &db, dragon);
    let prevented = attack_into_damage(
        &cast(&base, &db, "root_snare", PlayerId(0), Vec::new()),
        &db,
        dragon,
    );

    let later = |state: &GameState| {
        walk_until(state, &db, |s| s.turn == 3 && s.step == Step::PrecombatMain)
    };
    let landed = later(&landed);
    let prevented = later(&prevented);

    for state in [&landed, &prevented] {
        assert!(
            state.battlefield.iter().any(|perm| perm.id == dragon),
            "the dragon is still on the battlefield two turns later"
        );
        assert!(
            state.prevention.is_empty(),
            "and the shield ended in its own cleanup step (CR 514.2)"
        );
    }
    assert!(
        !has_hexproof(&landed, &db, dragon),
        "`yet` has no end: a turn boundary does not give the hexproof back"
    );
    assert!(
        has_hexproof(&prevented, &db, dragon),
        "and the turn boundary does not belatedly record damage nobody dealt"
    );
}

// ----- the same rule, without combat ----------------------------------------

/// The other seam a permanent is the **source** of damage at: a fight (CR 701.12). M19
/// prints no blanket `prevent all damage this turn`, so the shield is raised as the value
/// it is — the same one Root Snare's resolution produces, without the combat filter that
/// would make this test unable to fail.
///
/// Rabid Bite has Palladia-Mors deal its power to a creature seat 1 controls. Prevented,
/// the dragon has dealt nothing and keeps its hexproof; unprevented, the 2/2 dies and the
/// hexproof goes with it.
#[test]
fn cr_615_1_prevented_non_combat_damage_leaves_the_source_undealt() {
    let db = db();
    let mut base = fresh_game(&db);
    let dragon = place(&mut base, &db, "palladia_mors_the_ruiner", PlayerId(0));
    let ogre = place(&mut base, &db, "onakke_ogre", PlayerId(0));
    let prey = place(&mut base, &db, "walking_corpse", PlayerId(1)); // 2/2
    let base = walk_until(&base, &db, |s| s.turn == 1 && s.step == Step::PrecombatMain);
    let bite = vec![Target::Permanent(dragon), Target::Permanent(prey)];

    let unshielded = cast(&base, &db, "rabid_bite", PlayerId(0), bite.clone());
    assert!(
        !unshielded.battlefield.iter().any(|p| p.id == prey),
        "the control: six damage killed the 2/2 (CR 704.5g)"
    );
    assert!(
        !has_hexproof(&unshielded, &db, dragon),
        "and the dragon dealt it, so the condition stops holding"
    );

    let mut shielded = base.clone();
    shielded
        .prevention
        .push(DamageFilter { combat_only: false });
    let shielded = cast(&shielded, &db, "rabid_bite", PlayerId(0), bite);

    assert!(
        shielded.battlefield.iter().any(|p| p.id == prey),
        "the shield prevented the fight's damage, so nothing died"
    );
    assert!(
        has_hexproof(&shielded, &db, dragon),
        "and the dragon still has not dealt damage yet"
    );
    let offered = murder_targets(&shielded, &db);
    assert!(
        offered.contains(&ogre),
        "the control creature is aimable-at"
    );
    assert!(!offered.contains(&dragon), "the dragon is not (CR 702.11b)");
}

/// A **combat-only** shield is not a shield against a fight, and the flag follows the
/// damage rather than the shield: Root Snare leaves the bite alone, the damage lands, and
/// the hexproof goes. The half of the seam that has to be able to say no.
#[test]
fn cr_615_1_a_combat_only_shield_does_not_save_the_condition_from_a_fight() {
    let db = db();
    let mut base = fresh_game(&db);
    let dragon = place(&mut base, &db, "palladia_mors_the_ruiner", PlayerId(0));
    let prey = place(&mut base, &db, "walking_corpse", PlayerId(1));
    let base = walk_until(&base, &db, |s| s.turn == 1 && s.step == Step::PrecombatMain);

    let state = cast(&base, &db, "root_snare", PlayerId(0), Vec::new());
    let state = cast(
        &state,
        &db,
        "rabid_bite",
        PlayerId(0),
        vec![Target::Permanent(dragon), Target::Permanent(prey)],
    );

    assert!(
        !state.battlefield.iter().any(|p| p.id == prey),
        "a fight is not combat damage, and the shield lets it through"
    );
    assert!(
        !has_hexproof(&state, &db, dragon),
        "the damage was really dealt, so the condition really stops"
    );
}
