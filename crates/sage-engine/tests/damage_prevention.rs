//! Damage prevention (CR 615), and Root Snare (M19 #199) — the card that raises a
//! blanket shield over every point of combat damage a turn would deal.
//!
//! Prevention is a replacement effect: the prevented damage is **never dealt**, so there
//! is no downstream cleanup to do and nothing anywhere to undo. These tests are written
//! against the four places that would have observed the damage if it had happened — a
//! life total (CR 120.3a), the damage marked on a permanent (CR 120.3d), the
//! lethal-damage state-based action that reads that mark (CR 704.5g), and the log — and
//! they assert the absence at each.
//!
//! The shield is consulted at the one seam damage is dealt, so combat damage and
//! non-combat damage go through the same code. The last test is the evidence: a
//! combat-only shield lets a burn spell through, and an unfiltered one stops it, both
//! without the burn path knowing prevention exists.
//!
//! Every test drives the **real** [`apply_action`] pipeline. Cards are named by their
//! authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, Attack, AttackTarget, Block, CardDatabase, CardId,
    CardInstance, Color, FunctionalId, GameEvent, GameState, Permanent, PermanentId, PlayerId,
    Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk a combat and a turn boundary; a step that has not arrived by
/// then is a hang, and failing beats spinning.
const SETTLE_LIMIT: usize = 300;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with both pools stocked and both
/// libraries deep enough to survive a turn boundary — neither payability nor a decking
/// loss should ever decide a test that is about prevention.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for color in Color::ALL {
        for seat in 0..2 {
            state.players[seat].mana_pool.add(color, 10);
        }
    }
    for seat in 0..2 {
        state.players[seat].mana_pool.add_colorless(10);
        for _ in 0..5 {
            let card = state.new_instance(cid(db, "forest"));
            state.players[seat].library.push(card);
        }
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free
/// of summoning sickness.
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

/// Cast `slug` from `seat`'s hand and let it resolve.
fn cast(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    let card = to_hand(&mut state, db, slug, seat);
    state.priority = seat;
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is.
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
                .find(|a| !matches!(a, Action::Concede))
                .expect("some action is always offered")
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the game never reached the awaited step");
}

/// Attack `defender` with `attackers` and walk to the declare-blockers step.
fn attack_with(
    state: &GameState,
    db: &CardDatabase,
    attackers: &[PermanentId],
    defender: PlayerId,
) -> GameState {
    let state = settle_until(state, db, |s| s.step == Step::DeclareAttackers);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: attackers
                .iter()
                .map(|&attacker| Attack {
                    attacker,
                    defender: AttackTarget::Player(defender),
                })
                .collect(),
        },
        db,
    );
    settle_until(&state, db, |s| s.step == Step::DeclareBlockers)
}

/// Everything after the declare-blockers step: the combat damage is dealt on the way.
fn through_combat(state: &GameState, db: &CardDatabase) -> GameState {
    settle_until(state, db, |s| s.step == Step::PostcombatMain)
}

fn life(state: &GameState, seat: PlayerId) -> i32 {
    state.players[seat.0].life
}

/// The damage marked on `id`, or `None` once it has left the battlefield.
fn damage(state: &GameState, id: PermanentId) -> Option<u32> {
    state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.damage)
}

/// How many damage events the log has recorded — the fourth observer of damage, and the
/// one that would report a hit nobody took.
fn damage_events(state: &GameState) -> usize {
    state
        .log
        .iter()
        .filter(|entry| matches!(entry.event, GameEvent::DamageDealt { .. }))
        .count()
}

// ----- the card -------------------------------------------------------------

#[test]
fn issue_736_root_snare_is_a_one_effect_instant_that_raises_a_shield() {
    // The printed shape: `{1}{G}` instant, one effect, no target. Casting it puts a
    // shield in force that belongs to nobody — no controller is recorded, because
    // `all combat damage` covers what either player would deal.
    let db = db();
    let state = main_phase(&db);
    let snare = db.card(cid(&db, "root_snare")).expect("a bundled card");
    assert_eq!(snare.spell_effects.len(), 1);

    let after = cast(&state, &db, "root_snare", PlayerId(0), Vec::new());
    assert_eq!(
        after.prevention.len(),
        1,
        "resolving Root Snare raises exactly one shield (CR 615.1)"
    );
    assert!(after.prevention[0].combat_only);
}

#[test]
fn issue_736_combat_damage_to_a_player_is_prevented() {
    // CR 615.1 + CR 120.3a: prevented damage is never dealt, so it never becomes life
    // loss. The comparison is the same attack without the shield.
    let db = db();
    let mut state = main_phase(&db);
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0)); // 4/2

    let unprevented = through_combat(&attack_with(&state, &db, &[ogre], PlayerId(1)), &db);
    assert_eq!(
        life(&unprevented, PlayerId(1)),
        16,
        "without the shield the 4/2 takes four life"
    );

    let shielded = cast(&state, &db, "root_snare", PlayerId(0), Vec::new());
    let shielded = through_combat(&attack_with(&shielded, &db, &[ogre], PlayerId(1)), &db);
    assert_eq!(
        life(&shielded, PlayerId(1)),
        20,
        "the shield prevents the combat damage, so no life is lost (CR 120.3a)"
    );
    assert_eq!(
        damage_events(&shielded),
        0,
        "damage that was never dealt is never reported as dealt"
    );
}

#[test]
fn issue_736_combat_damage_to_a_creature_is_prevented_and_nothing_is_marked() {
    // CR 615.1 + CR 120.3d: neither the attacker's damage nor the blocker's swing back
    // is marked, because neither was dealt. Both directions in one combat.
    let db = db();
    let mut state = main_phase(&db);
    let attacker = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0)); // 6/6 trample
    let blocker = place(&mut state, &db, "giant_spider", PlayerId(1)); // 2/4

    let state = cast(&state, &db, "root_snare", PlayerId(0), Vec::new());
    let state = attack_with(&state, &db, &[attacker], PlayerId(1));
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block { blocker, attacker }],
        },
        &db,
    );
    let state = through_combat(&state, &db);

    assert_eq!(
        damage(&state, blocker),
        Some(0),
        "the attacker's damage is prevented, so nothing is marked on the blocker"
    );
    assert_eq!(
        damage(&state, attacker),
        Some(0),
        "and the blocker's damage back is prevented too"
    );
    assert_eq!(
        life(&state, PlayerId(1)),
        20,
        "a trampler's excess is combat damage as well, and is prevented with the rest"
    );
}

#[test]
fn issue_736_a_creature_that_would_have_died_survives_with_no_damage_marked() {
    // CR 704.5g reads marked damage, and prevention leaves none to read: the lethal
    // damage was never dealt, so the state-based action has nothing to act on. The
    // comparison is the same block without the shield, where the blocker dies.
    let db = db();
    let mut state = main_phase(&db);
    let attacker = place(&mut state, &db, "onakke_ogre", PlayerId(0)); // 4/2
    let blocker = place(&mut state, &db, "walking_corpse", PlayerId(1)); // 2/2

    let block = |state: &GameState| {
        let state = attack_with(state, &db, &[attacker], PlayerId(1));
        let state = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block { blocker, attacker }],
            },
            &db,
        );
        through_combat(&state, &db)
    };

    let unprevented = block(&state);
    assert_eq!(
        damage(&unprevented, blocker),
        None,
        "without the shield the 2/2 takes four and dies (CR 704.5g)"
    );

    let shielded = block(&cast(&state, &db, "root_snare", PlayerId(0), Vec::new()));
    assert_eq!(
        damage(&shielded, blocker),
        Some(0),
        "the creature survives, and survives with no damage marked at all"
    );
}

#[test]
fn issue_736_the_shield_expires_in_the_cleanup_step() {
    // CR 514.2: `this turn` ends where a pump ends. The shield is gone when the next
    // turn begins, and the next turn's combat damage lands in full.
    let db = db();
    let mut state = main_phase(&db);
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(1)); // 4/2, seat 1's

    let state = cast(&state, &db, "root_snare", PlayerId(0), Vec::new());
    assert_eq!(state.prevention.len(), 1, "in force this turn");

    let next_turn = settle_until(&state, &db, |s| s.turn == 2);
    assert!(
        next_turn.prevention.is_empty(),
        "the cleanup step ended it (CR 514.2)"
    );

    let attacked = through_combat(&attack_with(&next_turn, &db, &[ogre], PlayerId(0)), &db);
    assert_eq!(
        life(&attacked, PlayerId(0)),
        16,
        "a turn later there is no shield, and the same four damage lands"
    );
}

// ----- the same seam, without combat ----------------------------------------

#[test]
fn issue_736_a_combat_only_shield_does_not_prevent_a_burn_spell() {
    // Root Snare says *combat* damage, and the filter is read at the same seam the
    // combat damage went through: a burn spell in the same turn deals its damage in
    // full. This is the half of "one code path" that has to be able to say no.
    let db = db();
    let state = main_phase(&db);
    let state = cast(&state, &db, "root_snare", PlayerId(0), Vec::new());
    let state = cast(
        &state,
        &db,
        "lightning_strike",
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );
    assert_eq!(
        life(&state, PlayerId(1)),
        17,
        "non-combat damage is not combat damage, and the shield lets it through"
    );
}

#[test]
fn issue_736_an_unfiltered_shield_prevents_non_combat_damage_too() {
    // The same seam, the other answer. M19 prints no `prevent all damage this turn`, so
    // the shape is exercised against an inline catalog (ADR 0009): an unfiltered shield
    // and a burn spell, with nothing about combat anywhere in the test.
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_ward","name":"Test Ward",
             "types":["instant"],"mana_cost":"{W}","colors":["white"],
             "spell_effects":[{"kind":"prevent_damage"}]},
            {"schema_version":1,"functional_id":"test_bolt","name":"Test Bolt",
             "types":["instant"],"mana_cost":"{R}","colors":["red"],
             "spell_effects":[{"kind":"deal_damage","target":"any_target","amount":3}]},
            {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
             "types":["creature"],"subtypes":["Bear"],"mana_cost":"{1}{G}","colors":["green"],
             "power":2,"toughness":2}
        ]"#,
    )
    .expect("an inline catalog");
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for color in Color::ALL {
        state.players[0].mana_pool.add(color, 10);
    }
    let bear = place(&mut state, &db, "test_bear", PlayerId(1));

    let unprevented = cast(
        &state,
        &db,
        "test_bolt",
        PlayerId(0),
        vec![Target::Permanent(bear)],
    );
    assert_eq!(
        damage(&unprevented, bear),
        None,
        "three damage kills the 2/2 outright when nothing prevents it"
    );

    let state = cast(&state, &db, "test_ward", PlayerId(0), Vec::new());
    assert_eq!(state.prevention.len(), 1);
    assert!(
        !state.prevention[0].combat_only,
        "an unfiltered shield covers every damage event"
    );
    let state = cast(
        &state,
        &db,
        "test_bolt",
        PlayerId(0),
        vec![Target::Permanent(bear)],
    );
    assert_eq!(
        damage(&state, bear),
        Some(0),
        "the burn is prevented at the one damage seam: the bear lives, unmarked"
    );

    let state = cast(
        &state,
        &db,
        "test_bolt",
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );
    assert_eq!(
        life(&state, PlayerId(1)),
        20,
        "and the same shield prevents the damage aimed at a player"
    );
}
