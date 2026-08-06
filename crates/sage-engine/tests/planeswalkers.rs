//! Planeswalkers: loyalty, loyalty abilities, and being attacked (issue #608,
//! CR 306 / 606 / 704.5i).
//!
//! Every test drives the **real** [`apply_action`] pipeline. A planeswalker that
//! parses proves nothing; what has to be true is that it is an ordinary permanent
//! everywhere the rules already touch one — it enters, it can be targeted, it can be
//! attacked, it takes damage — and that the four places it is *not* ordinary behave:
//! loyalty counters instead of toughness, a sorcery-speed once-per-turn activation
//! whose cost is that resource, damage that removes loyalty instead of being marked,
//! and death at zero.
//!
//! **The cards are inline definitions** (ADR 0009), not M19 planeswalkers. When this file
//! was written none of the five was authorable — each needs an emblem, and four need a
//! second subsystem besides — and the definitions here are the loyalty mechanism reduced
//! to itself. They stay that way now that the five are authored (issue #620,
//! `tests/m19_planeswalkers.rs`): a test of the mechanism should fail for one reason, and
//! a shipped card would drag its emblem, its tokens, and its choice prompts in with it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attacker_candidates, blocker_candidates_for, characteristics,
    defender_candidates, target_requirements, valid_actions, Action, Attack, AttackTarget, Block,
    CardDatabase, CardId, CardInstance, CardType, Color, CounterKind, FunctionalId, GameState,
    Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// The inline catalog every test here runs on: a planeswalker with the three loyalty
/// abilities a real one has (a plus, a minus it can afford, and an ultimate it cannot),
/// a second planeswalker sharing nothing but its type, and the creatures and burn the
/// combat and damage tests need.
fn db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_warden","name":"Test Warden",
         "supertypes":["legendary"],"types":["planeswalker"],"subtypes":["Warden"],
         "mana_cost":"{2}{W}{W}","colors":["white"],"loyalty":4,
         "abilities":[
           {"type":"activated","cost":[{"kind":"loyalty","amount":1}],
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]},
           {"type":"activated","cost":[{"kind":"loyalty","amount":-2}],
            "effects":[{"kind":"deal_damage","target":"any_target","amount":2}]},
           {"type":"activated","cost":[{"kind":"loyalty","amount":-7}],
            "effects":[{"kind":"draw_card","count":3}]}]},
        {"schema_version":1,"functional_id":"test_seer","name":"Test Seer",
         "supertypes":["legendary"],"types":["planeswalker"],"subtypes":["Seer"],
         "mana_cost":"{1}{U}{U}","colors":["blue"],"loyalty":3,
         "abilities":[
           {"type":"activated","cost":[{"kind":"loyalty","amount":0}],
            "effects":[{"kind":"draw_card","count":1}]}]},
        {"schema_version":1,"functional_id":"test_sentinel","name":"Test Sentinel",
         "types":["creature"],"subtypes":["Human","Soldier"],"mana_cost":"{1}{W}",
         "colors":["white"],"power":2,"toughness":2},
        {"schema_version":1,"functional_id":"test_ogre","name":"Test Ogre",
         "types":["creature"],"subtypes":["Ogre"],"mana_cost":"{2}{R}","colors":["red"],
         "power":4,"toughness":2},
        {"schema_version":1,"functional_id":"test_dreadmaw","name":"Test Dreadmaw",
         "types":["creature"],"subtypes":["Dinosaur"],"mana_cost":"{4}{G}{G}",
         "colors":["green"],"power":6,"toughness":6,"keywords":["trample"]},
        {"schema_version":1,"functional_id":"test_bolt","name":"Test Bolt",
         "types":["instant"],"mana_cost":"{R}","colors":["red"],
         "spell_effects":[{"kind":"deal_damage","target":"any_target","amount":3}]},
        {"schema_version":1,"functional_id":"test_smite","name":"Test Smite",
         "types":["instant"],"mana_cost":"{W}","colors":["white"],
         "spell_effects":[{"kind":"destroy","target":"any_creature"}]},
        {"schema_version":1,"functional_id":"test_meteor","name":"Test Meteor",
         "types":["sorcery"],"mana_cost":"{4}{R}","colors":["red"],
         "spell_effects":[{"kind":"deal_damage","target":"any_target","amount":5}]}
    ]"#;
    CardDatabase::from_json(json).expect("the inline catalog is well-formed")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a card in the inline catalog")
}

/// A two-player game parked at player 0's precombat main with both pools stocked, so
/// payability never decides a test that is about a rule.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.turn = 3;
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        player.turn_began = 3;
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

/// Put a permanent of `slug` onto the battlefield under `controller`, applying the
/// battlefield-entry replacements a real entry applies — so a planeswalker placed this
/// way carries its printed loyalty rather than arriving at zero.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    let mut permanent = Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        ..Default::default()
    };
    if let Some(loyalty) = db.card(card).and_then(|c| c.loyalty) {
        permanent.counters.insert(CounterKind::Loyalty, loyalty);
    }
    state.battlefield.push(permanent);
    id
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast `slug` from `seat`'s hand with `targets` and let the stack empty.
fn cast(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    slug: &str,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, seat);
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    while !state.stack.is_empty() {
        state = apply_action(&state, &Action::PassPriority, db);
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// Take whatever action the pipeline currently offers that moves the game on.
fn advance(state: &GameState, db: &CardDatabase) -> GameState {
    let offered = valid_actions(state, db);
    let chosen = if offered.contains(&Action::PassPriority) {
        Action::PassPriority
    } else {
        offered
            .into_iter()
            .find(|a| a != &Action::Concede)
            .expect("some action is always available")
    };
    let after = apply_action(state, &chosen, db);
    assert_ne!(&after, state, "the pipeline stalled on {chosen:?}");
    after
}

/// Walk the engine's own turn-structure FSM ([`GameState::advance`]) until the turn
/// number changes, then park the new turn at its active player's precombat main.
///
/// Deliberately not a `turn += 1`: the per-turn resets — the loyalty allowance among
/// them — are produced by `begin_next_turn`, so a test that reaches the next turn this
/// way is asserting against a reset the engine performed rather than one it wrote down
/// itself.
fn next_turn(state: &GameState) -> GameState {
    let start = state.turn;
    let mut next = state.clone();
    while next.turn == start {
        next = next.advance();
    }
    next.step = Step::PrecombatMain;
    next.priority = next.active_player;
    next.consecutive_passes = 0;
    next
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

/// The loyalty counters on `id` right now, or `0` once it has left the battlefield.
fn loyalty(state: &GameState, id: PermanentId) -> u32 {
    state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.counter_count(CounterKind::Loyalty))
}

/// The activation of `permanent`'s ability `index`, in the requirement form
/// `valid_actions` advertises.
fn activation(permanent: PermanentId, index: usize) -> Action {
    Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

// ----- loyalty as a resource ------------------------------------------------

/// CR 306.5b / 704.5i, the two ends of a planeswalker's life: it arrives with its
/// printed loyalty as counters — through the *real* cast path, so the entry
/// replacement is the engine's and not the test's — and it is put into its owner's
/// graveyard the moment that reaches zero.
///
/// The zero end is driven by damage rather than by writing a counter down, because the
/// interesting claim is that the two rules meet: 5 damage to a 4-loyalty planeswalker
/// bottoms it out and the state-based action takes it from there, in the same
/// transition.
#[test]
fn issue_608_a_planeswalker_enters_with_its_printed_loyalty_and_dies_at_zero() {
    let db = db();
    let state = main_phase();

    // Cast it: it resolves onto the battlefield already carrying four loyalty.
    let state = cast(&state, &db, PlayerId(0), "test_warden", Vec::new());
    let warden = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(cid(&db, "test_warden")))
        .expect("the planeswalker resolved onto the battlefield")
        .id;
    assert_eq!(
        loyalty(&state, warden),
        4,
        "it enters with counters equal to its printed loyalty (CR 306.5b)"
    );
    assert_eq!(
        characteristics(&state, warden, &db).loyalty,
        Some(4),
        "printed loyalty is a characteristic and never changes"
    );

    // Five damage takes it past zero, and the CR 704.5i state-based action fires in
    // the same transition — no separate step, no window where it sits at 0.
    let state = cast(
        &state,
        &db,
        PlayerId(0),
        "test_meteor",
        vec![Target::Permanent(warden)],
    );
    assert!(
        !on_battlefield(&state, warden),
        "a planeswalker at zero loyalty is put into its owner's graveyard (CR 704.5i)"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        2,
        "the planeswalker and the sorcery that finished it are both in the graveyard"
    );
}

/// CR 120.3c: damage dealt to a planeswalker **removes that much loyalty**; it is not
/// marked on it. The distinction is not cosmetic — marked damage clears at cleanup
/// (CR 514.2), so a planeswalker whose damage were merely marked would heal every turn.
#[test]
fn issue_608_damage_removes_loyalty_rather_than_being_marked() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(1));

    let state = cast(
        &state,
        &db,
        PlayerId(0),
        "test_bolt",
        vec![Target::Permanent(warden)],
    );

    assert_eq!(loyalty(&state, warden), 1, "4 loyalty less 3 damage");
    let perm = state.battlefield.iter().find(|p| p.id == warden).unwrap();
    assert_eq!(
        perm.damage, 0,
        "nothing is marked on a planeswalker, so nothing heals at cleanup"
    );
    assert!(on_battlefield(&state, warden), "1 loyalty is still alive");
}

/// CR 115.4: "any target" means a creature, a player, **or a planeswalker**. Before
/// this issue the spec documented its own gap in prose; the enumeration is the proof it
/// is closed — and the neighbouring specs are the proof it was closed deliberately
/// rather than by making every permanent targetable.
#[test]
fn issue_608_any_target_offers_planeswalkers_alongside_creatures_and_players() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(1));
    let sentinel = place(&mut state, &db, "test_sentinel", PlayerId(1));
    let bolt = to_hand(&mut state, &db, "test_bolt", PlayerId(0));

    let requirements = target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card: bolt,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    let [slot] = requirements.as_slice() else {
        panic!("a bolt fills exactly one target slot");
    };
    assert!(
        slot.candidates.contains(&Target::Permanent(warden)),
        "a planeswalker is an `any target` candidate (CR 115.4)"
    );
    assert!(slot.candidates.contains(&Target::Permanent(sentinel)));
    assert!(slot.candidates.contains(&Target::Player(PlayerId(0))));
    assert!(slot.candidates.contains(&Target::Player(PlayerId(1))));

    // And a *creature* spec still excludes it: widening `any_target` did not widen
    // everything that names a type. Same board, same seat, one slot apart.
    let smite = to_hand(&mut state, &db, "test_smite", PlayerId(0));
    let requirements = target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card: smite,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    let [slot] = requirements.as_slice() else {
        panic!("a destroy-target-creature spell fills exactly one target slot");
    };
    assert!(
        !slot.candidates.contains(&Target::Permanent(warden)),
        "`any_creature` is not `any_target`: a planeswalker is not a creature"
    );
    assert!(slot.candidates.contains(&Target::Permanent(sentinel)));
}

// ----- loyalty abilities ----------------------------------------------------

/// CR 606.1 / 606.3: a loyalty ability is activatable at sorcery speed, its cost
/// changes the source's loyalty, and once one has been activated no other loyalty
/// ability of that permanent may be activated for the rest of the turn.
///
/// All three through the offer set and the real pipeline, in one test, because they are
/// one rule from a player's side: you get one, on your main phase, and it costs.
#[test]
fn issue_608_a_loyalty_ability_costs_loyalty_and_may_be_activated_once_a_turn() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(0));

    // The plus and the affordable minus are both offered; the ultimate is not.
    let offered = valid_actions(&state, &db);
    assert!(offered.contains(&activation(warden, 0)), "+1 is offered");
    assert!(offered.contains(&activation(warden, 1)), "-2 is offered");

    // Activating the plus adds a counter and gains 2 life once it resolves.
    let life_before = state.players[0].life;
    let mut state = apply_action(&state, &activation(warden, 0), &db);
    assert_eq!(
        loyalty(&state, warden),
        5,
        "a `+1` cost adds a loyalty counter (CR 606.1)"
    );
    while !state.stack.is_empty() {
        state = apply_action(&state, &Action::PassPriority, &db);
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    assert_eq!(state.players[0].life, life_before + 2);

    // CR 606.3: no second loyalty ability this turn, of any of them.
    let offered = valid_actions(&state, &db);
    assert!(
        !offered.contains(&activation(warden, 0)) && !offered.contains(&activation(warden, 1)),
        "one loyalty ability per planeswalker per turn (CR 606.3)"
    );

    // The allowance refreshes with the turn — walked through the engine's own turn
    // FSM, so the reset is `begin_next_turn`'s and not this test's. Two boundaries,
    // because the controller's *own* next turn is what matters.
    let state = next_turn(&next_turn(&state));
    assert_eq!((state.turn, state.active_player), (5, PlayerId(0)));
    assert!(
        valid_actions(&state, &db).contains(&activation(warden, 0)),
        "the once-per-turn allowance refreshes when the turn does"
    );
}

/// CR 606.3: a loyalty ability whose cost would remove more loyalty than the permanent
/// has is not offered, and becomes offered the moment the loyalty is there.
///
/// Tested both ways round on the *same* ability, so the absence is the cost talking and
/// not the ability being unreachable for some other reason.
#[test]
fn issue_608_an_unpayable_loyalty_cost_is_not_offered() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(0));

    assert!(
        !valid_actions(&state, &db).contains(&activation(warden, 2)),
        "a `-7` ability is not offered to a planeswalker at 4 (CR 606.3)"
    );

    // Give it the loyalty and the same ability appears.
    state
        .battlefield
        .iter_mut()
        .find(|p| p.id == warden)
        .unwrap()
        .counters
        .insert(CounterKind::Loyalty, 7);
    assert!(
        valid_actions(&state, &db).contains(&activation(warden, 2)),
        "at 7 loyalty the same ability is payable and offered"
    );

    // And paying it leaves the planeswalker at zero, which CR 704.5i then collects.
    let state = apply_action(&state, &activation(warden, 2), &db);
    assert!(
        !on_battlefield(&state, warden),
        "spending its last loyalty puts the planeswalker into the graveyard (CR 704.5i)"
    );
    assert_eq!(
        state.stack.len(),
        1,
        "the ability it paid for is on the stack and resolves without its source"
    );
}

/// The hardening path (the shape `activation_clears_summoning_sickness` uses): the
/// three CR 606.3 restrictions are re-derived in `apply_action`, so an action handed
/// straight to the engine — one `valid_actions` never offered — is refused rather than
/// applied.
///
/// Each of the three is checked on its own, because "the offer withheld it" and "the
/// apply path refuses it" are different guarantees and only the second survives a
/// forged or stale action id.
#[test]
fn issue_608_apply_rejects_a_forged_loyalty_activation() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(0));

    // 1. An unpayable cost.
    let after = apply_action(&state, &activation(warden, 2), &db);
    assert_eq!(after, state, "an unpayable `-7` changes nothing");

    // 2. Instant speed: the same ability, submitted while the stack is not empty.
    let mut with_stack = state.clone();
    let sentinel = to_hand(&mut with_stack, &db, "test_sentinel", PlayerId(0));
    let with_stack = apply_action(
        &with_stack,
        &Action::CastSpell {
            card: sentinel,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(!with_stack.stack.is_empty());
    let after = apply_action(&with_stack, &activation(warden, 0), &db);
    assert_eq!(
        after, with_stack,
        "a loyalty ability is sorcery speed; with a spell on the stack it is a no-op"
    );

    // 3. A second activation in one turn. The `-2` aims at the opponent, so this is
    //    a fully-formed action that fails on the once-per-turn rule alone.
    let minus_two = Action::ActivateAbility {
        permanent: warden,
        index: 1,
        targets: vec![Target::Player(PlayerId(1))],
        payment: Vec::new(),
    };
    let once = apply_action(&state, &activation(warden, 0), &db);
    assert_eq!(loyalty(&once, warden), 5);
    let twice = apply_action(&once, &minus_two, &db);
    assert_eq!(
        twice, once,
        "a second loyalty activation this turn changes nothing (CR 606.3)"
    );

    // And it really was the *loyalty* gate: on a fresh state the same action applies.
    let fresh = apply_action(&state, &minus_two, &db);
    assert_ne!(fresh, state);
    assert_eq!(loyalty(&fresh, warden), 2, "the `-2` was paid");
}

/// A planeswalker's controller is the only seat that may activate its loyalty
/// abilities, and only on their own turn — the sorcery-speed rule measured from the
/// *controller* rather than from whoever happens to hold priority.
#[test]
fn issue_608_an_opponent_is_never_offered_your_planeswalkers_abilities() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(1));

    // Seat 0 is the active player holding priority in their own main phase; the
    // planeswalker belongs to seat 1.
    assert!(
        !valid_actions(&state, &db).contains(&activation(warden, 0)),
        "a permanent's abilities are never offered to another seat"
    );

    // Seat 1 holding priority at instant speed during seat 0's main phase is offered
    // nothing either: sorcery speed means *their* turn.
    state.priority = PlayerId(1);
    assert!(
        !valid_actions(&state, &db).contains(&activation(warden, 0)),
        "sorcery speed is measured from the controller's own turn (CR 606.3)"
    );
}

// ----- being attacked -------------------------------------------------------

/// CR 508.1a: a planeswalker is a legal attack target, and its **controller** declares
/// blockers for the attackers attacking it (CR 509.1) — the split between "what is
/// attacked" and "who answers for it" that the widening exists for.
///
/// The blocked half matters as much as the declaration: a chump block absorbs the whole
/// hit, so the planeswalker takes nothing.
#[test]
fn issue_608_a_planeswalker_can_be_attacked_and_blocked_for() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(1));
    let ogre = place(&mut state, &db, "test_ogre", PlayerId(0));
    let blocker = place(&mut state, &db, "test_sentinel", PlayerId(1));

    // The planeswalker is offered as something to attack, beside its controller.
    state.step = Step::DeclareAttackers;
    assert_eq!(
        defender_candidates(&state, &db),
        vec![
            AttackTarget::Player(PlayerId(1)),
            AttackTarget::Planeswalker(warden)
        ],
        "an opponent and their planeswalker are both attackable (CR 508.1a)"
    );
    assert!(attacker_candidates(&state, &db).contains(&ogre));

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Planeswalker(warden),
            }],
        },
        &db,
    );
    let mut state = state;
    while state.step != Step::DeclareBlockers {
        state = advance(&state, &db);
    }

    // Its controller owes the declaration and may block with their own creature.
    assert_eq!(
        sage_engine::pending_blocker_declarer(&state),
        Some(PlayerId(1)),
        "the attacked planeswalker's controller declares blockers (CR 509.1)"
    );
    assert!(blocker_candidates_for(&state, PlayerId(1), &db).contains(&blocker));

    let mut state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker,
                attacker: ogre,
            }],
        },
        &db,
    );
    while state.step != Step::End {
        state = advance(&state, &db);
    }

    assert_eq!(
        loyalty(&state, warden),
        4,
        "a blocked attacker deals nothing to the planeswalker it was attacking"
    );
    assert!(
        !on_battlefield(&state, blocker),
        "the 2/2 chump blocker died to the 4/2"
    );
    assert_eq!(state.players[1].life, 20, "and no life was lost either");
}

/// CR 510.1c with CR 120.3c: an unblocked attacker's combat damage removes loyalty from
/// the planeswalker it is attacking, and takes nothing from its controller's life —
/// the two are different resources and an attack chooses between them.
#[test]
fn issue_608_unblocked_combat_damage_removes_loyalty_not_life() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(1));
    let ogre = place(&mut state, &db, "test_ogre", PlayerId(0));

    state.step = Step::DeclareAttackers;
    let mut state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Planeswalker(warden),
            }],
        },
        &db,
    );
    while state.step != Step::End {
        state = advance(&state, &db);
    }

    assert_eq!(
        state.players[1].life, 20,
        "the defending player loses no life to an attack on their planeswalker"
    );
    assert!(
        !on_battlefield(&state, warden),
        "4 combat damage to a 4-loyalty planeswalker finishes it (CR 120.3c / 704.5i)"
    );
}

/// CR 702.19e over CR 120.3c: a blocked trampler assigns just-lethal to its blocker and
/// tramples the remainder into the **planeswalker** it is attacking, not into the
/// defending player. Trample follows the attack, and the attack chose the planeswalker.
#[test]
fn issue_608_trample_overflow_goes_to_the_attacked_planeswalker() {
    let db = db();
    let mut state = main_phase();
    let warden = place(&mut state, &db, "test_warden", PlayerId(1));
    let maw = place(&mut state, &db, "test_dreadmaw", PlayerId(0)); // 6/6 trample
    let chump = place(&mut state, &db, "test_sentinel", PlayerId(1)); // 2/2

    state.step = Step::DeclareAttackers;
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: maw,
                defender: AttackTarget::Planeswalker(warden),
            }],
        },
        &db,
    );
    let mut state = state;
    while state.step != Step::DeclareBlockers {
        state = advance(&state, &db);
    }
    let mut state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: chump,
                attacker: maw,
            }],
        },
        &db,
    );
    while state.step != Step::End {
        state = advance(&state, &db);
    }

    // 2 lethal to the blocker, 4 over the top into the planeswalker's loyalty.
    assert!(!on_battlefield(&state, chump));
    assert_eq!(
        loyalty(&state, warden),
        0,
        "the 4 trample overflow removed all four loyalty"
    );
    assert!(!on_battlefield(&state, warden));
    assert_eq!(
        state.players[1].life, 20,
        "no trample damage leaked to the defending player"
    );
}

/// CR 506.4: a planeswalker that leaves the battlefield mid-combat leaves its attacker
/// attacking nothing, so that attacker deals its damage nowhere — and, in particular,
/// does *not* fall through onto the defending player.
///
/// The redirection rule is gone from current rules, so "the attack simply misses" is
/// the whole behavior, and the life total is what proves it.
#[test]
fn issue_608_an_attacker_whose_planeswalker_dies_hits_nothing() {
    let db = db();
    let mut state = main_phase();
    // A 3-loyalty planeswalker, so one instant finishes it inside the combat window.
    let seer = place(&mut state, &db, "test_seer", PlayerId(1));
    let ogre = place(&mut state, &db, "test_ogre", PlayerId(0));

    state.step = Step::DeclareAttackers;
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Planeswalker(seer),
            }],
        },
        &db,
    );

    // Seat 0 burns down their own victim before damage — the planeswalker dies and the
    // attack has nothing left to hit.
    let mut state = cast(
        &state,
        &db,
        PlayerId(0),
        "test_bolt",
        vec![Target::Permanent(seer)],
    );
    assert!(!on_battlefield(&state, seer));
    let attacker = state.battlefield.iter().find(|p| p.id == ogre).unwrap();
    assert_eq!(
        attacker.attacking, None,
        "an attacker whose planeswalker has gone is removed from combat (CR 506.4)"
    );

    while state.step != Step::End {
        state = advance(&state, &db);
    }
    assert_eq!(
        state.players[1].life, 20,
        "the damage is not redirected to the defending player — that rule is gone"
    );
}

/// Two-player combat with nothing to choose is unchanged: no planeswalker on the other
/// side means the sole opponent is the only attack target, exactly as before this
/// issue, and the declaration stays choice-free.
#[test]
fn issue_608_a_board_without_planeswalkers_still_has_one_attack_target() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "test_ogre", PlayerId(0));
    state.step = Step::DeclareAttackers;

    assert_eq!(
        defender_candidates(&state, &db),
        vec![AttackTarget::Player(PlayerId(1))],
        "the sole opponent is the only candidate"
    );
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    let mut state = state;
    while state.step != Step::End {
        state = advance(&state, &db);
    }
    assert_eq!(
        state.players[1].life, 16,
        "an ordinary attack on a player is unaffected by the widening"
    );
}

/// A player may never attack their own planeswalker (CR 508.1a — an attack names an
/// opponent or a permanent an opponent controls), and the declaration is refused rather
/// than merely unadvertised.
#[test]
fn issue_608_you_cannot_attack_your_own_planeswalker() {
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "test_warden", PlayerId(0));
    let ogre = place(&mut state, &db, "test_ogre", PlayerId(0));
    state.step = Step::DeclareAttackers;

    assert!(
        !defender_candidates(&state, &db).contains(&AttackTarget::Planeswalker(mine)),
        "your own planeswalker is not an attack candidate"
    );
    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: ogre,
                defender: AttackTarget::Planeswalker(mine),
            }],
        },
        &db,
    );
    assert_eq!(after, state, "declaring it is a no-op");
}

// ----- the legend rule ------------------------------------------------------

/// CR 704.5j: a player controlling two legendary permanents with the same name keeps
/// one and puts the rest into their graveyard. Two *different* legends coexist, and so
/// do two copies under *different* controllers — the rule is per-player and per-name,
/// and both halves are checked so the implementation cannot be "delete duplicates".
///
/// Which copy survives is a player *choice* in the rules and a deterministic policy
/// here (the newest stays); `data/exclusions.json` records that substitution.
#[test]
fn issue_608_the_legend_rule_leaves_one_copy_per_player() {
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "test_warden", PlayerId(0));
    let second = place(&mut state, &db, "test_warden", PlayerId(0));
    // A different legend under the same controller, and the same legend under the
    // other: neither is a duplicate of anything.
    let seer = place(&mut state, &db, "test_seer", PlayerId(0));
    let theirs = place(&mut state, &db, "test_warden", PlayerId(1));

    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !on_battlefield(&state, first),
        "the older copy goes to its owner's graveyard (CR 704.5j)"
    );
    assert!(on_battlefield(&state, second), "the newer copy stays");
    assert!(
        on_battlefield(&state, seer),
        "a differently named legend is untouched"
    );
    assert!(
        on_battlefield(&state, theirs),
        "the rule is per-player: the opponent's copy is untouched"
    );
}

// ----- the permanent model --------------------------------------------------

/// A planeswalker is not a creature anywhere the engine asks: it cannot attack, it
/// cannot block, it has no power or toughness, and the lethal-damage and
/// zero-toughness state-based actions have nothing to say about it.
///
/// The negative half of the model, and the one most likely to rot: every rule added to
/// combat or to the toughness checks has to keep answering "not this object".
#[test]
fn issue_608_a_planeswalker_is_not_a_creature() {
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "test_warden", PlayerId(0));
    let theirs = place(&mut state, &db, "test_seer", PlayerId(1));
    state.step = Step::DeclareAttackers;

    assert!(
        !attacker_candidates(&state, &db).contains(&mine),
        "a planeswalker cannot attack"
    );
    assert!(
        !blocker_candidates_for(&state, PlayerId(1), &db).contains(&theirs),
        "a planeswalker cannot block"
    );
    let current = characteristics(&state, mine, &db);
    assert_eq!(current.power, None);
    assert_eq!(current.toughness, None);
    assert_eq!(current.types, vec![CardType::Planeswalker]);
    // With no toughness, neither CR 704.5f nor CR 704.5g can reach it — it survives an
    // SBA pass at full loyalty with damage that would kill any creature it could be.
    let settled = apply_action(&state, &Action::PassPriority, &db);
    assert!(on_battlefield(&settled, mine));
    assert_eq!(loyalty(&settled, mine), 4);
}
