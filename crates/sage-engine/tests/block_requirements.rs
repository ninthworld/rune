//! A declaration a player is not free to leave out (CR 509.1c, issue #739), and the M19
//! card that grants one.
//!
//! Every combat rule before this narrowed a declaration: a pair the evasion gate refuses,
//! a count a floor or a ceiling refuses. A **requirement** is the first that judges a
//! declaration by what it *omits* — the declaration chosen must obey the maximum possible
//! number of requirements without violating any restriction — and "the maximum possible"
//! is a fact about the declarations that were **not** submitted. That is why validating
//! one is a search ([`max_block_requirements_met`]) rather than a per-pair check, and why
//! the two halves of CR 509.1c get a test each here: a restriction always wins, and a
//! requirement no legal declaration can meet is simply not met.
//!
//! Declare Dominance is the card, and it grants the requirement rather than printing it —
//! so every test that uses it also proves the grant binds through the computed
//! characteristics at CR 613 layer 6, exactly as a printed restriction would.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! Cards are named by their authored `functional_id`, never by an interned handle
//! (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, max_block_requirements_met, must_be_blocked_by_all_able,
    valid_actions, Action, Attack, AttackTarget, Block, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk a combat; a step that has not arrived by then is a hang, and
/// failing beats spinning.
const SETTLE_LIMIT: usize = 200;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with both pools stocked so payability
/// never decides a test that is about a requirement. Player 0 attacks throughout; player 1
/// blocks, so the declaration under test is always player 1's.
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

/// Cast `card` at the creature `target` and resolve it — the real announcement gate and
/// the real CR 608.2b re-check, not a hand-placed static effect.
fn cast_at(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    target: PermanentId,
) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card,
            targets: vec![Target::Permanent(target)],
            payment: Vec::new(),
        },
        db,
    );
    let mut state = state;
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    assert!(state.stack.is_empty(), "the spell never resolved");
    state
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is (the
/// empty combat declaration a declare step owes).
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

/// Declare every one of `attackers` against player 1 and walk to the declare-blockers
/// step, where player 1 owes the declaration.
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
    settle_until(&state, db, |s| s.step == Step::DeclareBlockers)
}

/// Whether a blocker declaration is accepted by the pipeline — submitted as a real
/// declaration, so an illegal one is a no-op rather than an error.
fn blocks_are_legal(state: &GameState, db: &CardDatabase, blocks: Vec<Block>) -> bool {
    &apply_action(state, &Action::DeclareBlockers { blocks }, db) != state
}

/// The one-pair declaration, spelled out once so the tests read as the choices a player
/// makes rather than as struct literals.
fn block(blocker: PermanentId, attacker: PermanentId) -> Block {
    Block { blocker, attacker }
}

// ----- the card -------------------------------------------------------------

#[test]
fn declare_dominance_pumps_and_grants_the_requirement_to_the_one_creature() {
    // Both halves of one sentence about one creature, which is why they are one effect:
    // a card that pumped through one slot and bound through another would let a player
    // pump one creature and bind a different one.
    let db = db();
    let mut state = main_phase();
    let hero = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let bystander = place(&mut state, &db, "walking_corpse", PlayerId(0));
    let spell = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    assert!(!must_be_blocked_by_all_able(&state, hero, &db));
    let state = cast_at(&state, &db, spell, hero);

    assert_eq!(characteristics(&state, hero, &db).power, Some(3 + 3));
    assert_eq!(characteristics(&state, hero, &db).toughness, Some(3 + 3));
    assert!(
        must_be_blocked_by_all_able(&state, hero, &db),
        "the granted requirement binds exactly as a printed restriction does"
    );
    assert!(
        !must_be_blocked_by_all_able(&state, bystander, &db),
        "and it reaches nothing the spell did not name"
    );
    assert_eq!(characteristics(&state, bystander, &db).power, Some(2));
}

// ----- the requirement ------------------------------------------------------

#[test]
fn every_creature_able_to_block_the_pumped_creature_must_do_so() {
    // The whole feature at the action seam. Two creatures can block, so two requirements
    // exist, and no declaration that meets fewer than both is legal — including the empty
    // one, which is the declaration a player who has not read the card would send.
    let db = db();
    let mut state = main_phase();
    let hero = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let first = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let second = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let spell = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, spell, hero);
    let state = attack_with(&state, &db, &[hero]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 2);
    assert!(
        !blocks_are_legal(&state, &db, Vec::new()),
        "declaring no blockers leaves two requirements unmet"
    );
    assert!(
        !blocks_are_legal(&state, &db, vec![block(first, hero)]),
        "one blocker is fewer requirements than a legal declaration could meet"
    );
    assert!(blocks_are_legal(
        &state,
        &db,
        vec![block(first, hero), block(second, hero)]
    ));
}

#[test]
fn a_creature_that_could_not_block_anyway_is_not_required_to() {
    // Only the *able* are required, and ability is judged by the gates that were already
    // there: a tapped creature is not a blocker candidate, so the requirement never
    // reaches it and the declaration that leaves it out is the maximum.
    let db = db();
    let mut state = main_phase();
    let hero = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let ready = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let tapped = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == tapped)
        .expect("the creature is on the battlefield")
        .tapped = true;
    let spell = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, spell, hero);
    let state = attack_with(&state, &db, &[hero]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);
    assert!(blocks_are_legal(&state, &db, vec![block(ready, hero)]));
}

#[test]
fn a_pairwise_restriction_beats_the_requirement() {
    // CR 509.1c, first half: a creature that cannot legally block the attacker is not
    // required to. The pumped creature flies, so only the blocker with reach is able —
    // and the declaration naming just it is complete, not one short.
    let db = db();
    let mut state = main_phase();
    let flyer = place(&mut state, &db, "air_elemental", PlayerId(0));
    let grounded = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let spider = place(&mut state, &db, "giant_spider", PlayerId(1));
    let spell = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, spell, flyer);
    let state = attack_with(&state, &db, &[flyer]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);
    assert!(
        blocks_are_legal(&state, &db, vec![block(spider, flyer)]),
        "the only able creature blocking is every requirement met"
    );
    assert!(
        !blocks_are_legal(&state, &db, Vec::new()),
        "the reach blocker is still required"
    );
    assert!(
        !blocks_are_legal(
            &state,
            &db,
            vec![block(spider, flyer), block(grounded, flyer)]
        ),
        "and a requirement never makes an illegal block legal"
    );
}

#[test]
fn a_count_restriction_beats_the_requirement() {
    // CR 509.1c, second half: menace's floor of two makes a lone blocker illegal, so with
    // one creature on the other side the requirement cannot be met at all — and a
    // requirement that cannot be met is not met. Declaring nothing is the legal answer.
    let db = db();
    let mut state = main_phase();
    let brute = place(&mut state, &db, "boggart_brute", PlayerId(0));
    let sole = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let spell = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, spell, brute);
    let state = attack_with(&state, &db, &[brute]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 0);
    assert!(
        blocks_are_legal(&state, &db, Vec::new()),
        "no declaration can meet the requirement, so none has to"
    );
    assert!(
        !blocks_are_legal(&state, &db, vec![block(sole, brute)]),
        "and the restriction still refuses the block the requirement asked for"
    );
}

#[test]
fn two_required_attackers_and_one_blocker_owe_a_single_block() {
    // The case a per-pair check gets wrong in the other direction: each attacker requires
    // the defender's only creature, and it can block one of them. The maximum is one, so
    // either answer is legal and neither is a violation of the other's requirement.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let second = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let sole = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let one = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));
    let two = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, one, first);
    let state = cast_at(&state, &db, two, second);
    let state = attack_with(&state, &db, &[first, second]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);
    assert!(!blocks_are_legal(&state, &db, Vec::new()));
    for attacker in [first, second] {
        assert!(
            blocks_are_legal(&state, &db, vec![block(sole, attacker)]),
            "one block is as many requirements as any declaration could meet"
        );
    }
}

#[test]
fn a_blocker_that_may_block_two_is_required_to_block_both() {
    // The maximum is what the *board* allows, not what one creature usually does: the
    // permission to block an additional creature raises the maximum from one to two, and
    // the declaration that stops at one is short by exactly the assignment the permission
    // made possible.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let second = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));
    let one = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));
    let two = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, one, first);
    let state = cast_at(&state, &db, two, second);
    let state = attack_with(&state, &db, &[first, second]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 2);
    assert!(!blocks_are_legal(&state, &db, vec![block(twins, first)]));
    assert!(blocks_are_legal(
        &state,
        &db,
        vec![block(twins, first), block(twins, second)]
    ));
}

#[test]
fn a_combat_with_no_requirement_is_declared_exactly_as_before() {
    // The control every one of these tests needs: without a requirement the empty
    // declaration is legal, a partial block is legal, and the search costs the ordinary
    // combat nothing it can observe.
    let db = db();
    let mut state = main_phase();
    let attacker = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let blocker = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let state = attack_with(&state, &db, &[attacker]);

    assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 0);
    assert!(blocks_are_legal(&state, &db, Vec::new()));
    assert!(blocks_are_legal(
        &state,
        &db,
        vec![block(blocker, attacker)]
    ));
}

#[test]
fn the_requirement_is_gone_the_turn_after() {
    // It is granted until end of turn, so the creature that had to be blocked on one turn
    // is an ordinary attacker on the next — the cleanup step removes the grant and the
    // computed restrictions simply stop including it (CR 514.2).
    let db = db();
    let mut state = main_phase();
    let hero = place(&mut state, &db, "centaur_courser", PlayerId(0));
    place(&mut state, &db, "walking_corpse", PlayerId(1));
    let spell = to_hand(&mut state, &db, "declare_dominance", PlayerId(0));

    let state = cast_at(&state, &db, spell, hero);
    assert!(must_be_blocked_by_all_able(&state, hero, &db));

    let later = settle_until(&state, &db, |s| s.turn > state.turn);
    assert!(!must_be_blocked_by_all_able(&later, hero, &db));
    assert_eq!(max_block_requirements_met(&later, PlayerId(1), &db), 0);
}
