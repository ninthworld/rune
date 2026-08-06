//! Activation costs paid by sacrificing a chosen permanent, or by discarding
//! (CR 601.2b / 602.2b).
//!
//! The cast side of this landed first, and the difference here is only *where the source
//! is*: a spell is in hand on its way to the stack, an ability is on a permanent that is
//! already on the battlefield. Everything else is the same problem and gets the same
//! answer — a cost paid at announcement has no resolution to ask during and nothing to
//! take back once the ability is on the stack, so what pays it arrives on the action.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, CostPayment,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with pools stocked so mana never
/// decides a test that is about a cost, and libraries stocked so a draw has somewhere to
/// draw from.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let forest = cid(db, "forest");
    for seat in 0..2 {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            state.players[seat].mana_pool.add(color, 10);
        }
        state.players[seat].mana_pool.add_colorless(10);
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, past summoning
/// sickness so a `{T}` cost is payable.
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

/// Put `slug` into player 0's hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    instance
}

/// The requirement form of activating `permanent`'s ability `index` — the shape
/// `valid_actions` advertises, with neither targets nor payment filled in.
fn offer(permanent: PermanentId, index: usize) -> Action {
    Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// Whether `id` is on the battlefield.
fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// Activate `permanent`'s ability `index` with `payment`, and let the ability resolve.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    payment: Vec<CostPayment>,
) -> GameState {
    let action = Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
        payment,
    };
    let state = apply_action(state, &action, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

// ----- the cost gates the offer ---------------------------------------------

#[test]
fn an_ability_that_eats_another_creature_is_not_offered_alone() {
    // Ravenous Harpy: `Sacrifice another creature` with nothing else on the board is
    // unactivatable, not activatable-and-then-free (CR 602.2b). *Another* is the whole
    // point — the Harpy is a creature its controller controls, and it is still not a legal
    // payment for its own cost.
    let db = db();
    let mut state = main_phase(&db);
    let harpy = place(&mut state, &db, "ravenous_harpy", PlayerId(0));
    assert!(
        !valid_actions(&state, &db).contains(&offer(harpy, 0)),
        "the Harpy cannot eat itself"
    );

    // An opponent's creature is not a candidate either: CR 701.17b lets a player sacrifice
    // only what they control.
    place(&mut state, &db, "centaur_courser", PlayerId(1));
    assert!(!valid_actions(&state, &db).contains(&offer(harpy, 0)));

    place(&mut state, &db, "centaur_courser", PlayerId(0));
    assert!(
        valid_actions(&state, &db).contains(&offer(harpy, 0)),
        "one other creature of their own is enough"
    );
}

#[test]
fn a_discard_cost_is_not_offered_from_an_empty_hand() {
    // Dismissive Pyromancer's rummage. An empty hand cannot pay, so the ability is
    // withheld rather than offered as a free draw.
    let db = db();
    let mut state = main_phase(&db);
    let pyromancer = place(&mut state, &db, "dismissive_pyromancer", PlayerId(0));
    assert!(
        !valid_actions(&state, &db).contains(&offer(pyromancer, 0)),
        "nothing to discard, no activation"
    );

    to_hand(&mut state, &db, "shock");
    assert!(valid_actions(&state, &db).contains(&offer(pyromancer, 0)));
}

// ----- what the cost accepts ------------------------------------------------

#[test]
fn the_sacrifice_is_charged_and_the_ability_still_does_what_it_says() {
    // Both halves, because a cost charged without the ability resolving is a different bug
    // from an ability that resolved without the cost.
    let db = db();
    let mut state = main_phase(&db);
    let harpy = place(&mut state, &db, "ravenous_harpy", PlayerId(0));
    let food = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let life = state.players[0].life;

    let after = activate(&state, &db, harpy, 0, vec![CostPayment::Sacrifice(food)]);

    assert!(!on_battlefield(&after, food), "the creature was sacrificed");
    let harpy_now = after
        .battlefield
        .iter()
        .find(|perm| perm.id == harpy)
        .expect("the Harpy is still there");
    assert_eq!(
        harpy_now.counter_count(sage_engine::CounterKind::PlusOnePlusOne),
        1,
        "the ability resolved"
    );
    assert_eq!(after.players[0].life, life + 1);
}

#[test]
fn a_sacrifice_filtered_by_subtype_takes_that_subtype_and_the_source_itself() {
    // Goblin Trashmaster: `Sacrifice a Goblin` — no *another*, so the Trashmaster is its
    // own legal payment, and a creature that is not a Goblin is not.
    let db = db();
    let mut state = main_phase(&db);
    let trashmaster = place(&mut state, &db, "goblin_trashmaster", PlayerId(0));
    let courser = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let artifact = place(&mut state, &db, "diamond_mare", PlayerId(1));
    let target = sage_engine::Target::Permanent(artifact);

    // A non-Goblin cannot pay it.
    let refused = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: trashmaster,
            index: 1,
            targets: vec![target],
            payment: vec![CostPayment::Sacrifice(courser)],
        },
        &db,
    );
    assert_eq!(refused, state, "a Centaur is not a Goblin");

    // The Trashmaster is a Goblin, and may be fed to its own ability.
    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: trashmaster,
            index: 1,
            targets: vec![target],
            payment: vec![CostPayment::Sacrifice(trashmaster)],
        },
        &db,
    );
    assert!(
        !on_battlefield(&after, trashmaster),
        "it spent itself for the cost"
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(
        !on_battlefield(&after, artifact),
        "and the ability resolved anyway (CR 113.7a)"
    );
}

#[test]
fn a_goblin_token_pays_a_cost_that_names_the_subtype() {
    // The subtype is read off the printed face, and a token has one (ADR 0015): a Goblin
    // token from Goblin Instigator is a Goblin, and is a legal sacrifice.
    let db = db();
    let mut state = main_phase(&db);
    let trashmaster = place(&mut state, &db, "goblin_trashmaster", PlayerId(0));
    place(&mut state, &db, "diamond_mare", PlayerId(1));
    let instigator = to_hand(&mut state, &db, "goblin_instigator");

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instigator,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let mut state = apply_action(&state, &Action::PassPriority, &db);
    // The trigger that makes the token goes on the stack and resolves.
    while !state.stack.is_empty() {
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    let token = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card().is_none())
        .expect("the Goblin token arrived")
        .id;

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: trashmaster,
            index: 1,
            targets: vec![sage_engine::Target::Permanent(
                state
                    .battlefield
                    .iter()
                    .find(|perm| perm.controller == PlayerId(1))
                    .expect("the artifact")
                    .id,
            )],
            payment: vec![CostPayment::Sacrifice(token)],
        },
        &db,
    );
    assert!(!on_battlefield(&after, token), "the token paid the cost");
    assert!(on_battlefield(&after, trashmaster));
}

#[test]
fn the_discarded_card_is_the_one_the_action_named() {
    // Dismissive Pyromancer's rummage: the card the player picked goes to the graveyard
    // and the one they kept stays in hand.
    let db = db();
    let mut state = main_phase(&db);
    let pyromancer = place(&mut state, &db, "dismissive_pyromancer", PlayerId(0));
    let kept = to_hand(&mut state, &db, "murder");
    let spent = to_hand(&mut state, &db, "shock");
    let hand_before = state.players[0].hand.len();

    let after = activate(
        &state,
        &db,
        pyromancer,
        0,
        vec![CostPayment::Discard(spent.id)],
    );

    assert!(
        after.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == spent.id),
        "the chosen card was discarded"
    );
    assert!(after.players[0].hand.iter().any(|card| card.id == kept.id));
    // One discarded, one drawn: the hand size is unchanged.
    assert_eq!(after.players[0].hand.len(), hand_before);
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.id == pyromancer && perm.tapped),
        "and the {{T}} half of the cost was charged too"
    );
}

// ----- what the cost refuses ------------------------------------------------

#[test]
fn an_activation_that_names_the_wrong_payment_is_a_no_op() {
    // Each of these is a real submission a client could send, and each has to leave the
    // game exactly where it was — no counter placed, no life gained, and nothing
    // sacrificed on the way to discovering the payment was wrong.
    let db = db();
    let mut state = main_phase(&db);
    let harpy = place(&mut state, &db, "ravenous_harpy", PlayerId(0));
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let card = to_hand(&mut state, &db, "shock");

    let refuse = |payment: Vec<CostPayment>| {
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: harpy,
                index: 0,
                targets: Vec::new(),
                payment,
            },
            &db,
        );
        assert_eq!(after, state, "an illegal payment changes nothing");
    };

    refuse(Vec::new());
    refuse(vec![CostPayment::Sacrifice(theirs)]);
    refuse(vec![CostPayment::Sacrifice(land)]);
    refuse(vec![CostPayment::Sacrifice(harpy)]);
    refuse(vec![CostPayment::Sacrifice(PermanentId(9999))]);
    refuse(vec![
        CostPayment::Sacrifice(mine),
        CostPayment::Sacrifice(mine),
    ]);
    // A discard is not what this cost asks for, and a cost is paid for exactly what it
    // asks — over-paying is not a thing a player may choose to do.
    refuse(vec![
        CostPayment::Sacrifice(mine),
        CostPayment::Discard(card.id),
    ]);
}

#[test]
fn an_ability_with_no_chosen_cost_accepts_no_payment() {
    // The other direction of exactness: Llanowar Elves' `{T}: Add {G}` demands nothing a
    // player picks, so an action naming a sacrifice for it is refused rather than charged.
    let db = db();
    let mut state = main_phase(&db);
    let elves = place(&mut state, &db, "llanowar_elves", PlayerId(0));
    let courser = place(&mut state, &db, "centaur_courser", PlayerId(0));

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: elves,
            index: 0,
            targets: Vec::new(),
            payment: vec![CostPayment::Sacrifice(courser)],
        },
        &db,
    );
    assert_eq!(after, state);
}

#[test]
fn mana_is_never_named_on_an_activation() {
    // An activation pays mana from the pool (CR 602.2b), floated by activating mana
    // abilities as actions in their own right. A mana source named on the activation is
    // refused rather than silently dropped: a payment the engine ignored is one the player
    // believed they had made.
    let db = db();
    let mut state = main_phase(&db);
    let harpy = place(&mut state, &db, "ravenous_harpy", PlayerId(0));
    let food = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let swamp = place(&mut state, &db, "swamp", PlayerId(0));

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: harpy,
            index: 0,
            targets: Vec::new(),
            payment: vec![
                CostPayment::Sacrifice(food),
                CostPayment::Mana(sage_engine::ManaSource {
                    permanent: swamp,
                    index: 0,
                }),
            ],
        },
        &db,
    );
    assert_eq!(after, state);
}

// ----- the sacrifice is a real death ----------------------------------------

#[test]
fn a_creature_sacrificed_to_an_activation_cost_dies_for_every_watcher() {
    // Doomed Dissenter leaves a Zombie behind when it dies, and it dies here — the cost
    // goes down the one leaves-battlefield seam, so a dies trigger sees it exactly as it
    // sees a creature destroyed in combat.
    let db = db();
    let mut state = main_phase(&db);
    let harpy = place(&mut state, &db, "ravenous_harpy", PlayerId(0));
    let dissenter = place(&mut state, &db, "doomed_dissenter", PlayerId(0));

    let after = activate(
        &state,
        &db,
        harpy,
        0,
        vec![CostPayment::Sacrifice(dissenter)],
    );

    assert!(!on_battlefield(&after, dissenter));
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.printed.card().is_none()),
        "the dies trigger made its Zombie, so the sacrifice was a real death"
    );
}

// ----- the offer and the payment are one answer -----------------------------

#[test]
fn every_offered_activation_has_a_payment_that_exists() {
    // The invariant the cast side states over the catalog, on this side of the seam: an
    // ability the generator offers is one a payment can pay for. When these two answers
    // come apart, an automated player takes the offer, has it refused as a no-op, and takes
    // it again for ever — so the offer gate and the auto-payment go through one enumeration
    // rather than two that agree by inspection.
    let db = db();
    for board in [
        &["ravenous_harpy"][..],
        &["ravenous_harpy", "centaur_courser"][..],
        &["goblin_trashmaster"][..],
        &["goblin_trashmaster", "goblin_motivator"][..],
        &["dismissive_pyromancer"][..],
        &["dismissive_pyromancer", "ravenous_harpy", "diamond_mare"][..],
    ] {
        for hand in 0..2 {
            let mut state = main_phase(&db);
            for slug in board {
                place(&mut state, &db, slug, PlayerId(0));
            }
            for _ in 0..hand {
                to_hand(&mut state, &db, "shock");
            }
            // Something to aim a `Destroy target artifact` at, under the opponent.
            place(&mut state, &db, "diamond_mare", PlayerId(1));

            for action in valid_actions(&state, &db) {
                let Action::ActivateAbility {
                    permanent, index, ..
                } = action
                else {
                    continue;
                };
                assert!(
                    sage_engine::auto_activation_payment(&state, &db, permanent, index).is_some(),
                    "{board:?} with {hand} cards in hand offers an activation no payment can pay"
                );
            }
        }
    }
}
