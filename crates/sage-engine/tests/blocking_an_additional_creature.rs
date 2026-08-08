//! A blocker that blocks more than one attacker (CR 509.1a), and the M19 card that
//! prints the permission.
//!
//! Until now a declaration mapped one blocker to one attacker, and the whole of
//! `Permanent::blocking` was that single assignment. Three things move together once a
//! blocker may be assigned to several attackers, and this file is the evidence for each:
//! the **declare-blockers legality gate** (how many attackers one creature may be given),
//! the **combat damage assignment** (one pool of power spread across them), and the
//! order that spread follows — the order the declaration named them in, which is the
//! declaring player's own CR 509.3 damage assignment order.
//!
//! Ghastbark Twins is the card, and it is deliberately a *permission*: nothing forces it
//! to use the extra block, so nothing here needs the CR 509.1c maximisation that attack
//! and block **requirements** would. Those remain unmodeled and remain excluded.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog: a
//! definition that parses is not evidence of anything. Cards are named by their authored
//! `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, blocks_allowed, valid_actions, Action, Attack, AttackTarget, Block, CardDatabase,
    CardId, CombatRestriction, FunctionalId, GameState, Keyword, Permanent, PermanentId, PlayerId,
    Step,
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

/// A two-player game at player 0's precombat main. Player 0 attacks throughout; player 1
/// blocks, so the declaration under test is always player 1's.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
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

/// The damage marked on `id`, or `None` once it has left the battlefield.
fn damage(state: &GameState, id: PermanentId) -> Option<u32> {
    state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.damage)
}

// ----- the card -------------------------------------------------------------

#[test]
fn ghastbark_twins_is_a_trampling_body_that_may_block_one_more() {
    // The printed shape: a permission carrying a count, beside an ordinary keyword. It
    // says nothing about attacking and imposes nothing on anyone else.
    let db = db();
    let card = db
        .card(cid(&db, "ghastbark_twins"))
        .expect("a bundled card");
    assert!(card.keywords.contains(&Keyword::Trample));
    assert_eq!(
        card.restrictions,
        vec![CombatRestriction::CanBlockAdditional(1)]
    );
}

#[test]
fn ghastbark_twins_may_be_assigned_to_two_attackers_and_no_more() {
    // The whole feature, as the declaration sees it: two assignments for one creature
    // are legal, three are not, and the count comes from the permission rather than from
    // anything about the attackers.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let second = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));
    let third = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));

    assert_eq!(
        blocks_allowed(&state, twins, &db),
        2,
        "one block, plus the one additional creature the permission names"
    );

    let state = attack_with(&state, &db, &[first, second, third]);
    assert!(blocks_are_legal(
        &state,
        &db,
        vec![
            Block {
                blocker: twins,
                attacker: first,
            },
            Block {
                blocker: twins,
                attacker: second,
            },
        ]
    ));
    assert!(
        !blocks_are_legal(
            &state,
            &db,
            vec![
                Block {
                    blocker: twins,
                    attacker: first,
                },
                Block {
                    blocker: twins,
                    attacker: second,
                },
                Block {
                    blocker: twins,
                    attacker: third,
                },
            ]
        ),
        "a third assignment is one more than the permission allows"
    );
    assert!(
        !blocks_are_legal(
            &state,
            &db,
            vec![
                Block {
                    blocker: twins,
                    attacker: first,
                },
                Block {
                    blocker: twins,
                    attacker: first,
                },
            ]
        ),
        "blocking the same attacker twice is the same block written down twice, not a \
         second one — and it would double that attacker's blocker count"
    );
}

#[test]
fn a_creature_without_the_permission_still_blocks_exactly_one_attacker() {
    // The control, and the assertion that the gate reads the permission rather than
    // having simply stopped counting: the same declaration shape with an ordinary body
    // is refused.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let second = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));
    let ordinary = place(&mut state, &db, "centaur_courser", PlayerId(1));

    assert_eq!(blocks_allowed(&state, ordinary, &db), 1);

    let state = attack_with(&state, &db, &[first, second]);
    assert!(
        blocks_are_legal(
            &state,
            &db,
            vec![Block {
                blocker: ordinary,
                attacker: first,
            }]
        ),
        "one block is what it has always been able to do"
    );
    assert!(
        !blocks_are_legal(
            &state,
            &db,
            vec![
                Block {
                    blocker: ordinary,
                    attacker: first,
                },
                Block {
                    blocker: ordinary,
                    attacker: second,
                },
            ]
        ),
        "the second assignment makes the whole declaration illegal (CR 509.1a)"
    );
}

#[test]
fn ghastbark_twins_records_its_blocks_in_the_order_it_declared_them() {
    // The order is stored state, and it is the declaration's own: it is what the damage
    // assignment follows, so it has to survive the declaration rather than be recovered
    // from battlefield order afterwards.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let second = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));
    let state = attack_with(&state, &db, &[first, second]);

    for order in [[first, second], [second, first]] {
        let declared = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: order
                    .iter()
                    .map(|&attacker| Block {
                        blocker: twins,
                        attacker,
                    })
                    .collect(),
            },
            &db,
        );
        let blocking = &declared
            .battlefield
            .iter()
            .find(|p| p.id == twins)
            .expect("the blocker is still on the battlefield")
            .blocking;
        assert_eq!(blocking, &order.to_vec());
    }
}

// ----- combat damage across two attackers -----------------------------------

#[test]
fn a_blocker_spreads_one_pool_of_power_across_the_attackers_it_blocks() {
    // CR 510.1c/e from the blocker's side: the Twins' 7 power is *one* pool, assigned
    // just-lethal to the first attacker in its order and the remainder to the last —
    // not 7 to each, which is what an attacker-driven loop would deal.
    //
    // A 3/3 first and a 6/6 second: 3 is lethal to the courser, and the 4 left over
    // reaches the dreadmaw and stays marked on it.
    let db = db();
    let mut state = main_phase();
    let courser = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let dreadmaw = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));

    let state = attack_with(&state, &db, &[courser, dreadmaw]);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![
                Block {
                    blocker: twins,
                    attacker: courser,
                },
                Block {
                    blocker: twins,
                    attacker: dreadmaw,
                },
            ],
        },
        &db,
    );
    let after = settle_until(&state, &db, |s| s.step == Step::EndCombat);

    assert_eq!(
        damage(&after, courser),
        None,
        "the first attacker in the order took lethal damage and died (CR 704.5g)"
    );
    assert_eq!(
        damage(&after, dreadmaw),
        Some(4),
        "and the remaining 4 reached the second, which survived it"
    );
    assert_eq!(
        damage(&after, twins),
        None,
        "both attackers struck the one blocker back, which is 9 on a 7/7"
    );
}

#[test]
fn the_blockers_order_decides_which_attacker_the_damage_reaches() {
    // The mirror of the test above with the declaration order reversed. The 6/6 is now
    // first, so 6 of the Twins' 7 is lethal to *it* and the single point left over is all
    // the 3/3 behind it takes. Nothing but the order changed, and it moved the death from
    // one attacker to the other, which is what makes the order load-bearing.
    let db = db();
    let mut state = main_phase();
    let courser = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let dreadmaw = place(&mut state, &db, "colossal_dreadmaw", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));

    let state = attack_with(&state, &db, &[courser, dreadmaw]);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![
                Block {
                    blocker: twins,
                    attacker: dreadmaw,
                },
                Block {
                    blocker: twins,
                    attacker: courser,
                },
            ],
        },
        &db,
    );
    let after = settle_until(&state, &db, |s| s.step == Step::EndCombat);

    assert_eq!(
        damage(&after, dreadmaw),
        None,
        "the first in the order took lethal damage and died (CR 704.5g)"
    );
    assert_eq!(
        damage(&after, courser),
        Some(1),
        "and the second took only what the first could not absorb"
    );
}

#[test]
fn one_blocked_attacker_still_takes_the_whole_of_its_blockers_power() {
    // The case every combat before this one was: a blocker assigned to a single attacker
    // deals its full power, not the lethal minimum. The spread is only a spread when
    // there is something to spread across.
    let db = db();
    let mut state = main_phase();
    // An 8/8 rather than the 6/6 the spread tests use: the whole pool is only *visible*
    // on an attacker that survives it, and 7 kills a 6/6.
    let mammoth = place(&mut state, &db, "aggressive_mammoth", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));

    let state = attack_with(&state, &db, &[mammoth]);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: twins,
                attacker: mammoth,
            }],
        },
        &db,
    );
    let after = settle_until(&state, &db, |s| s.step == Step::EndCombat);

    assert_eq!(
        damage(&after, mammoth),
        Some(7),
        "all 7 power on the one attacker it blocked"
    );
}

#[test]
fn blocking_two_attackers_leaves_neither_of_them_unblocked() {
    // The point of the extra block, from the defending player's side: both attackers are
    // blocked, so neither deals its damage to the player, and the trampler's overflow is
    // the only life lost.
    let db = db();
    let mut state = main_phase();
    let courser = place(&mut state, &db, "centaur_courser", PlayerId(0));
    // An 8/8 trampler, because 7 of anything less is swallowed whole by a 7/7 blocker and
    // there would be no overflow left to be the point of this test.
    let mammoth = place(&mut state, &db, "aggressive_mammoth", PlayerId(0));
    let twins = place(&mut state, &db, "ghastbark_twins", PlayerId(1));
    let before = state.players[1].life;

    let state = attack_with(&state, &db, &[courser, mammoth]);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![
                Block {
                    blocker: twins,
                    attacker: courser,
                },
                Block {
                    blocker: twins,
                    attacker: mammoth,
                },
            ],
        },
        &db,
    );
    let after = settle_until(&state, &db, |s| s.step == Step::EndCombat);

    // The courser has 3 power against a 7-toughness blocker, so it reaches nobody — the
    // trample the Mammoth grants it changes nothing, because there is no lethal-and-then-
    // some to spill. The Mammoth itself assigns 7 to the blocker and tramples the 1 that
    // is left (CR 702.19e), and that single point is the whole of the life lost.
    assert_eq!(after.players[1].life, before - 1);
}
