//! The two cost shapes issue #721 finishes: exiling a card from a graveyard and
//! sacrificing a **fixed** number of permanents greater than one — plus the amount that
//! reads a cost's own payment back.
//!
//! Every test here is about the same distinction the sacrifice-cost tests are about: a
//! cost is not an effect. It gates the offer, it is paid as the object goes on the stack
//! (CR 601.2h), and nothing on the stack can take it back. The amount is the sharp end of
//! that — by the time `Thud` resolves, the creature whose power it throws is in a
//! graveyard, so the number has to have been written down when it was paid (CR 608.2h).
//! Sacrificing a number the *player* picks is the other side of that line and lives in
//! `scapeshift_sacrifices_on_resolution.rs`: it is a decision, so it belongs to a
//! resolution rather than to a payment.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    activation_exile_cost, apply_action, characteristics, valid_actions, Action, CardDatabase,
    CardId, CardInstance, CardInstanceId, Color, CostPayment, FunctionalId, GameState, Permanent,
    PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, pools stocked so payability never
/// decides a test that is about a cost.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
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

/// Put `slug` into `seat`'s hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Put `slug` into `seat`'s graveyard and return the instance.
fn to_graveyard(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// Give `seat` a library of `slugs`, in order.
fn library_of(state: &mut GameState, db: &CardDatabase, seat: PlayerId, slugs: &[&str]) {
    let cards: Vec<CardInstance> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[seat.0].library = cards;
}

/// Pass priority twice — both seats — so the top of the stack resolves.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let once = apply_action(state, &Action::PassPriority, db);
    apply_action(&once, &Action::PassPriority, db)
}

/// The requirement form of casting `card` — the shape `valid_actions` advertises.
fn offer(card: CardInstance) -> Action {
    Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// The requirement form of activating `permanent`'s ability `index`.
fn activation(permanent: PermanentId, index: usize) -> Action {
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

/// Whether `seat`'s graveyard holds `card`.
fn in_graveyard(state: &GameState, seat: PlayerId, card: CardInstanceId) -> bool {
    state.players[seat.0]
        .graveyard
        .iter()
        .any(|held| held.id == card)
}

/// The index of the activated ability of `permanent` the offer actually carries — found
/// from `valid_actions` rather than assumed, since an index is a property of the card.
fn offered_index(state: &GameState, db: &CardDatabase, permanent: PermanentId) -> usize {
    valid_actions(state, db)
        .into_iter()
        .find_map(|action| match action {
            Action::ActivateAbility {
                permanent: source,
                index,
                ..
            } if source == permanent => Some(index),
            _ => None,
        })
        .expect("the ability is offered")
}

// ----- 1. exiling a card as a cost (Graveyard Marshal) ----------------------

#[test]
fn issue_721_an_exile_cost_with_nothing_to_exile_withholds_the_activation() {
    // CR 601.2b, the same rule a sacrifice cost obeys: a cost that cannot be paid makes
    // the ability unactivatable rather than activatable-and-then-free. An empty graveyard
    // is the plainest way to be unable to pay this one — and a graveyard holding only a
    // land is the interesting one, because there *is* something there and it is the wrong
    // class.
    let db = db();
    let mut state = main_phase();
    let marshal = place(&mut state, &db, "graveyard_marshal", PlayerId(0));
    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == marshal)),
        "an empty graveyard pays nothing"
    );

    to_graveyard(&mut state, &db, "forest", PlayerId(0));
    // And the opponent's creature card is not the activator's to exile.
    to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(1));
    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == marshal)),
        "a land in your graveyard and a creature card in theirs pay nothing either"
    );

    to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    assert!(
        valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == marshal)),
        "one creature card of your own is the whole cost"
    );
}

#[test]
fn issue_721_the_exile_cost_is_chosen_from_a_server_computed_candidate_list() {
    // The candidates are advertised exactly as a target slot's are: the engine enumerates
    // them and the client picks from the list, computing no legality of its own
    // (ADR 0004). Here that means the creature cards in the *activator's own* graveyard
    // and nothing else.
    let db = db();
    let mut state = main_phase();
    let marshal = place(&mut state, &db, "graveyard_marshal", PlayerId(0));
    let ogre = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let digger = to_graveyard(&mut state, &db, "gravedigger", PlayerId(0));
    to_graveyard(&mut state, &db, "forest", PlayerId(0));
    to_graveyard(&mut state, &db, "shock", PlayerId(0));
    let theirs = to_graveyard(&mut state, &db, "daybreak_chaplain", PlayerId(1));

    let index = offered_index(&state, &db, marshal);
    let cost = activation_exile_cost(&state, &db, marshal, index).expect("an exile cost");
    assert_eq!(cost.count, 1, "the cost exiles one card");
    assert_eq!(
        cost.candidates,
        vec![ogre.id, digger.id],
        "creature cards of your own graveyard, in pile order — not the land, not the \
         instant, and not the opponent's creature"
    );
    assert!(!cost.candidates.contains(&theirs.id));
}

#[test]
fn issue_721_a_forged_exile_choice_is_refused_by_apply_action() {
    // The independent apply-time gate. Every named card is re-derived from current state,
    // so an id naming the wrong class, the wrong graveyard, or nothing at all pays
    // nothing — and the whole activation is a no-op rather than a free one.
    let db = db();
    let mut state = main_phase();
    let marshal = place(&mut state, &db, "graveyard_marshal", PlayerId(0));
    to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let land = to_graveyard(&mut state, &db, "forest", PlayerId(0));
    let theirs = to_graveyard(&mut state, &db, "daybreak_chaplain", PlayerId(1));
    let index = offered_index(&state, &db, marshal);

    for forged in [land.id, theirs.id] {
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: marshal,
                index,
                targets: Vec::new(),
                payment: vec![CostPayment::Exile(forged)],
            },
            &db,
        );
        assert!(after.stack.is_empty(), "the activation was refused");
        assert_eq!(
            after.players[0].exile.len(),
            0,
            "and nothing was exiled trying"
        );
        assert_eq!(
            after.players[0].mana_pool.black, state.players[0].mana_pool.black,
            "nor was any mana spent"
        );
    }

    // Paying nothing at all is refused for the same reason: a cost is paid for what it
    // asks, and an empty payment is not that.
    let unpaid = apply_action(&state, &activation(marshal, index), &db);
    assert!(unpaid.stack.is_empty());
}

#[test]
fn issue_721_paying_the_exile_cost_moves_the_card_to_exile_and_the_ability_resolves() {
    // The cost is paid as the ability is activated (CR 602.2b): the card is in exile
    // before anyone gets priority, so responding to the ability cannot get it back.
    let db = db();
    let mut state = main_phase();
    let marshal = place(&mut state, &db, "graveyard_marshal", PlayerId(0));
    let ogre = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let index = offered_index(&state, &db, marshal);

    let announced = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: marshal,
            index,
            targets: Vec::new(),
            payment: vec![CostPayment::Exile(ogre.id)],
        },
        &db,
    );
    assert_eq!(announced.stack.len(), 1, "the ability is on the stack");
    assert!(
        !in_graveyard(&announced, PlayerId(0), ogre.id),
        "the card left the graveyard as the cost was paid"
    );
    assert!(
        announced.players[0]
            .exile
            .iter()
            .any(|card| card.id == ogre.id),
        "…and is in exile, not in a graveyard and not nowhere"
    );

    let after = settle(&announced, &db);
    let token = after
        .battlefield
        .iter()
        .find(|perm| perm.printed.is_token())
        .expect("a Zombie token");
    assert!(token.tapped, "the token enters tapped, as the card says");
    assert_eq!(characteristics(&after, token.id, &db).power, Some(2));
    assert_eq!(characteristics(&after, token.id, &db).toughness, Some(2));
}

// ----- 2. sacrificing more than one permanent (Sai, Master Thopterist) ------

#[test]
fn issue_721_a_two_artifact_sacrifice_is_refused_with_only_one_artifact() {
    // The point of a *count* rather than two costs: two artifacts is one cost taking a
    // pair, so one artifact does not half-pay it. Sai is itself not an artifact, so a lone
    // Manalith is the whole of what the board could offer.
    let db = db();
    let mut state = main_phase();
    let sai = place(&mut state, &db, "sai_master_thopterist", PlayerId(0));
    let manalith = place(&mut state, &db, "manalith", PlayerId(0));
    // An opponent's artifact is not the activator's to sacrifice (CR 701.17b).
    place(&mut state, &db, "millstone", PlayerId(1));

    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == sai)),
        "one artifact of your own is not two"
    );

    // …and a forged payment naming the one is refused by the apply-time gate too.
    let forged = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: sai,
            index: 1,
            targets: Vec::new(),
            payment: vec![CostPayment::Sacrifice(manalith)],
        },
        &db,
    );
    assert!(forged.stack.is_empty(), "the activation was refused");
    assert!(
        on_battlefield(&forged, manalith),
        "and the one artifact was not eaten trying"
    );
}

#[test]
fn issue_721_sai_eats_two_artifacts_at_once_and_draws() {
    let db = db();
    let mut state = main_phase();
    let sai = place(&mut state, &db, "sai_master_thopterist", PlayerId(0));
    let manalith = place(&mut state, &db, "manalith", PlayerId(0));
    let millstone = place(&mut state, &db, "millstone", PlayerId(0));
    let keeper = place(&mut state, &db, "fountain_of_renewal", PlayerId(0));
    library_of(&mut state, &db, PlayerId(0), &["forest", "island"]);
    let index = offered_index(&state, &db, sai);

    let announced = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: sai,
            index,
            targets: Vec::new(),
            payment: vec![
                CostPayment::Sacrifice(manalith),
                CostPayment::Sacrifice(millstone),
            ],
        },
        &db,
    );
    assert!(!on_battlefield(&announced, manalith));
    assert!(!on_battlefield(&announced, millstone));
    assert!(
        on_battlefield(&announced, keeper),
        "the third artifact is untouched — a cost takes what it asks for and no more"
    );
    assert!(
        on_battlefield(&announced, sai),
        "the source is not part of its own cost"
    );

    let after = settle(&announced, &db);
    assert_eq!(after.players[0].hand.len(), 1, "one card drawn");
    // A sacrifice is a real death down the one leaves-battlefield seam, so both artifacts
    // are in the graveyard rather than merely gone.
    assert_eq!(after.players[0].graveyard.len(), 2);
}

#[test]
fn issue_721_naming_the_same_artifact_twice_does_not_pay_for_two() {
    // One permanent may not pay two units of one cost. Naming it twice is the same
    // submission as naming one, and is refused rather than counted twice.
    let db = db();
    let mut state = main_phase();
    let sai = place(&mut state, &db, "sai_master_thopterist", PlayerId(0));
    let manalith = place(&mut state, &db, "manalith", PlayerId(0));
    place(&mut state, &db, "millstone", PlayerId(0));
    let index = offered_index(&state, &db, sai);

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: sai,
            index,
            targets: Vec::new(),
            payment: vec![
                CostPayment::Sacrifice(manalith),
                CostPayment::Sacrifice(manalith),
            ],
        },
        &db,
    );
    assert!(after.stack.is_empty());
    assert!(on_battlefield(&after, manalith));
}

// ----- Sai's cast trigger, filtered to artifact spells -----------------------

#[test]
fn issue_721_sais_trigger_fires_for_an_artifact_spell_and_not_for_a_creature_spell() {
    // The shared `spell_matches_class` predicate, read off the printed types. It fires as
    // the spell goes on the stack — before it resolves — and a green creature spell is not
    // an artifact spell however it is cast.
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "sai_master_thopterist", PlayerId(0));
    let artifact = to_hand(&mut state, &db, "manalith", PlayerId(0));
    let creature = to_hand(&mut state, &db, "highland_game", PlayerId(0));

    let thopters = |state: &GameState| {
        state
            .battlefield
            .iter()
            .filter(|perm| perm.printed.is_token())
            .count()
    };

    let after_creature = apply_action(&state, &offer(creature), &db);
    assert_eq!(
        after_creature.stack.len(),
        1,
        "the creature spell is on the stack and nothing joined it"
    );
    let after_creature = settle(&after_creature, &db);
    assert_eq!(thopters(&after_creature), 0, "no Thopter for a 2/1 Elk");

    let after_artifact = apply_action(&state, &offer(artifact), &db);
    assert_eq!(
        after_artifact.stack.len(),
        2,
        "the artifact spell and the trigger it fired"
    );
    let after_artifact = settle(&settle(&after_artifact, &db), &db);
    assert_eq!(thopters(&after_artifact), 1);
    let thopter = after_artifact
        .battlefield
        .iter()
        .find(|perm| perm.printed.is_token())
        .unwrap();
    assert_eq!(
        characteristics(&after_artifact, thopter.id, &db).power,
        Some(1)
    );
    assert!(characteristics(&after_artifact, thopter.id, &db)
        .keywords
        .contains(&sage_engine::Keyword::Flying));
}

// ----- 3. an amount read off the payment, after the payer has gone (Thud) ---

#[test]
fn issue_721_thud_is_not_offered_without_a_creature_to_throw() {
    let db = db();
    let mut state = main_phase();
    let thud = to_hand(&mut state, &db, "thud", PlayerId(0));
    // An opponent's creature is not the caster's to sacrifice (CR 701.17b).
    place(&mut state, &db, "onakke_ogre", PlayerId(1));
    assert!(!valid_actions(&state, &db).contains(&offer(thud)));

    place(&mut state, &db, "onakke_ogre", PlayerId(0));
    assert!(valid_actions(&state, &db).contains(&offer(thud)));
}

#[test]
fn issue_721_thuds_damage_is_the_power_the_sacrificed_creature_had() {
    // CR 608.2h, and the whole reason the payment is recorded rather than re-derived: by
    // the time the spell resolves the Ogre is in a graveyard, with no `PermanentId` and no
    // computed characteristics. The assertion that matters is the one made *after* it has
    // left — 4 damage from a creature that is not there.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let thud = to_hand(&mut state, &db, "thud", PlayerId(0));

    let announced = apply_action(
        &state,
        &Action::CastSpell {
            card: thud,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: vec![CostPayment::Sacrifice(ogre)],
        },
        &db,
    );
    assert_eq!(announced.stack.len(), 1, "the spell is on the stack");
    assert!(
        !on_battlefield(&announced, ogre),
        "the creature was already gone when the spell was announced"
    );
    assert_eq!(
        announced.players[1].life, 20,
        "and nothing has resolved yet"
    );

    let after = settle(&announced, &db);
    assert!(
        !on_battlefield(&after, ogre),
        "still gone, and still gone when the damage was dealt"
    );
    assert_eq!(
        after.players[1].life, 16,
        "4 damage — the power a creature that no longer exists had"
    );
}

#[test]
fn issue_721_thuds_amount_is_the_power_at_payment_time_including_a_pump() {
    // The number is taken from the *computed* power as the cost is paid, so a creature
    // sacrificed while pumped throws its pumped power — and the pump wearing off later
    // could not change a number already written down.
    let db = db();
    let mut state = main_phase();
    let elf = place(&mut state, &db, "llanowar_elves", PlayerId(0));
    let thud = to_hand(&mut state, &db, "thud", PlayerId(0));
    let pump = to_hand(&mut state, &db, "titanic_growth", PlayerId(0));

    // +4/+4 on a 1/1: a 5/5 Elf, then thrown.
    let pumped = apply_action(
        &state,
        &Action::CastSpell {
            card: pump,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(elf)],
            payment: Vec::new(),
        },
        &db,
    );
    let pumped = settle(&pumped, &db);
    assert_eq!(characteristics(&pumped, elf, &db).power, Some(5));

    let after = settle(
        &apply_action(
            &pumped,
            &Action::CastSpell {
                card: thud,
                mode: None,
                x: None,
                targets: vec![Target::Player(PlayerId(1))],
                payment: vec![CostPayment::Sacrifice(elf)],
            },
            &db,
        ),
        &db,
    );
    assert_eq!(after.players[1].life, 15, "5 damage, not 1");
}

#[test]
fn issue_721_a_cost_paid_at_announcement_cannot_be_countered_away() {
    // CR 601.2h: the cost is paid as the spell is cast, so countering the spell undoes
    // nothing about the payment. The creature stays dead and the damage simply never
    // happens — which is exactly the trade a card like this makes, and the opposite of
    // what the same text written as an effect would do.
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let thud = to_hand(&mut state, &db, "thud", PlayerId(0));
    let cancel = to_hand(&mut state, &db, "cancel", PlayerId(1));

    let announced = apply_action(
        &state,
        &Action::CastSpell {
            card: thud,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: vec![CostPayment::Sacrifice(ogre)],
        },
        &db,
    );
    assert!(!on_battlefield(&announced, ogre));
    let thud_on_stack = announced.stack[0].id;

    // Priority passes to the opponent, who counters it.
    let passed = apply_action(&announced, &Action::PassPriority, &db);
    let countered = apply_action(
        &passed,
        &Action::CastSpell {
            card: cancel,
            mode: None,
            x: None,
            targets: vec![Target::Spell(thud_on_stack)],
            payment: Vec::new(),
        },
        &db,
    );
    let after = settle(&settle(&countered, &db), &db);

    assert!(after.stack.is_empty(), "both spells have left the stack");
    assert_eq!(after.players[1].life, 20, "no damage was dealt");
    assert!(
        !on_battlefield(&after, ogre),
        "and the creature is still dead: the cost was paid, not promised"
    );
    assert!(
        after.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == thud.id),
        "the countered spell is in its owner's graveyard"
    );
}
