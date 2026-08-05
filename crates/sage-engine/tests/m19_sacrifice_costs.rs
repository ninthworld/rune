//! Additional cast costs paid by sacrificing a creature (CR 601.2b / 701.17).
//!
//! A cost is not an effect, and every test here is really about that difference. Written
//! as an effect the sacrifice would happen on **resolution**, which would make both cards
//! castable with an empty board, castable while the sacrifice was countered away, and
//! respondable-to before the creature was gone. As a cost it is paid while the spell is
//! cast: the card is not even *offered* without a creature to spend, and paying it is
//! part of the cast rather than something the stack could interrupt.
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

/// A two-player game at player 0's precombat main, with pools stocked so payability never
/// decides a test that is about a cost, and libraries stocked so a three-card draw has
/// somewhere to draw from.
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

/// Put `slug` into player 0's hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    instance
}

/// The requirement form of casting `card` — the shape `valid_actions` advertises, with no
/// payment and no targets filled in.
fn offer(card: CardInstance) -> Action {
    Action::CastSpell {
        card,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// Whether `id` is on the battlefield.
fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// Cast `card` sacrificing `victim`, and let it resolve.
fn cast_sacrificing(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    victim: PermanentId,
) -> GameState {
    let action = Action::CastSpell {
        card,
        targets: Vec::new(),
        payment: vec![CostPayment::Sacrifice(victim)],
    };
    let state = apply_action(state, &action, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

// ----- the cost gates the offer ---------------------------------------------

#[test]
fn a_spell_with_a_sacrifice_cost_is_not_offered_without_a_creature_to_spend() {
    // The whole reason this is a cost and not an effect. An empty board does not make the
    // spell castable-and-then-free; it makes it uncastable (CR 601.2b).
    let db = db();
    let mut state = main_phase(&db);
    let spell = to_hand(&mut state, &db, "blood_divination");
    assert!(
        !valid_actions(&state, &db).contains(&offer(spell)),
        "no creature, no cast"
    );

    // A land is not a creature, and neither is an opponent's creature — CR 701.17b lets a
    // player sacrifice only what they control.
    place(&mut state, &db, "forest", PlayerId(0));
    place(&mut state, &db, "centaur_courser", PlayerId(1));
    assert!(!valid_actions(&state, &db).contains(&offer(spell)));

    place(&mut state, &db, "centaur_courser", PlayerId(0));
    assert!(
        valid_actions(&state, &db).contains(&offer(spell)),
        "one creature of their own is enough"
    );
}

#[test]
fn the_cost_is_charged_and_the_spell_still_does_what_it_says() {
    // Blood Divination: the creature dies and three cards are drawn. Both halves, because
    // a cost that was charged without the spell resolving would be a different bug from a
    // spell that resolved without the cost.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let spell = to_hand(&mut state, &db, "blood_divination");
    let hand_before = state.players[0].hand.len();

    let after = cast_sacrificing(&state, &db, spell, victim);
    assert!(
        !on_battlefield(&after, victim),
        "the creature was sacrificed"
    );
    assert_eq!(
        after.players[0].graveyard.len(),
        2,
        "the creature and the spell itself are both in the graveyard"
    );
    // Three drawn, minus the spell that left the hand to be cast.
    assert_eq!(after.players[0].hand.len(), hand_before - 1 + 3);
}

#[test]
fn a_creature_spell_pays_the_same_cost_and_arrives() {
    // Demon of Catastrophes: the same cost on a permanent spell, so the sacrifice happens
    // at announcement and the Demon arrives afterwards. A board that ends with the Demon
    // and *not* the sacrificed creature is the whole card.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let spell = to_hand(&mut state, &db, "demon_of_catastrophes");

    let after = cast_sacrificing(&state, &db, spell, victim);
    assert!(!on_battlefield(&after, victim));
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.printed.face(&db).map(|f| f.name()) == Some("Demon of Catastrophes")),
        "the Demon resolved onto the battlefield"
    );
}

// ----- what the payment may name --------------------------------------------

#[test]
fn a_payment_naming_the_wrong_permanent_is_refused() {
    // Every way of paying with something the cost did not ask for. Each is a submission a
    // client can really send, so each has to be rejected rather than merely unoffered.
    let db = db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let spell = to_hand(&mut state, &db, "blood_divination");

    let refused = |payment: Vec<CostPayment>| {
        let action = Action::CastSpell {
            card: spell,
            targets: Vec::new(),
            payment,
        };
        apply_action(&state, &action, &db) == state
    };

    assert!(refused(Vec::new()), "a cost is not paid by paying nothing");
    assert!(refused(vec![CostPayment::Sacrifice(theirs)]), "not theirs");
    assert!(
        refused(vec![CostPayment::Sacrifice(land)]),
        "not a creature"
    );
    assert!(
        refused(vec![
            CostPayment::Sacrifice(mine),
            CostPayment::Sacrifice(mine),
        ]),
        "the same creature cannot pay twice"
    );
    assert!(
        refused(vec![CostPayment::Sacrifice(PermanentId(9999))]),
        "nor may a permanent that is not there"
    );
}

#[test]
fn a_spell_with_no_such_cost_accepts_no_sacrifice_at_all() {
    // Exact in both directions: over-paying a cost is not something a player may choose to
    // do, and a spell that quietly ate a creature it never asked for would be worse than
    // one that refused.
    let db = db();
    let mut state = main_phase(&db);
    let bystander = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let spell = to_hand(&mut state, &db, "divination");

    let action = Action::CastSpell {
        card: spell,
        targets: Vec::new(),
        payment: vec![CostPayment::Sacrifice(bystander)],
    };
    assert_eq!(
        apply_action(&state, &action, &db),
        state,
        "Divination has no additional cost and pays none"
    );
}

// ----- the sacrifice is a real death ----------------------------------------

#[test]
fn the_sacrifice_is_a_real_death_that_dies_watchers_see() {
    // CR 701.17, and the reason this goes down the one leaves-battlefield seam rather than
    // quietly removing a permanent from a vector. Doomed Dissenter's own dies trigger has
    // to fire for a creature sacrificed to a cost, exactly as it would for one destroyed.
    let db = db();
    let mut state = main_phase(&db);
    let dissenter = place(&mut state, &db, "doomed_dissenter", PlayerId(0));
    let spell = to_hand(&mut state, &db, "blood_divination");

    let after = cast_sacrificing(&state, &db, spell, dissenter);
    assert!(!on_battlefield(&after, dissenter));
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.printed.face(&db).map(|f| f.name()) == Some("Zombie")),
        "the dies trigger fired and left a Zombie behind"
    );
}

#[test]
fn the_cost_is_paid_after_the_spell_is_on_the_stack() {
    // CR 601.2h: the spell goes on the stack, *then* costs are paid. Asserted at the
    // moment between the cast and the resolution, which is the only moment the ordering
    // is visible — by the time the spell has resolved, both orderings look the same.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let spell = to_hand(&mut state, &db, "blood_divination");

    let cast = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            targets: Vec::new(),
            payment: vec![CostPayment::Sacrifice(victim)],
        },
        &db,
    );
    assert!(
        cast.stack.iter().any(|object| matches!(object.kind,
                sage_engine::StackObjectKind::Spell { card } if card.id == spell.id)),
        "the spell reached the stack"
    );
    assert!(
        !on_battlefield(&cast, victim),
        "and the cost was charged in the same action, not left for resolution"
    );
}
