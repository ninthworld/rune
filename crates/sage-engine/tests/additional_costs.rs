//! Additional costs to cast a spell (CR 601.2b), driven through the real
//! [`apply_action`] pipeline.
//!
//! The whole point of modelling `As an additional cost to cast this spell, discard a
//! card` as a **cost** rather than as the first effect of the spell is what it forbids:
//! a player who cannot pay cannot cast. Authored as an effect it was a spell you could
//! cast with an empty hand and simply not pay for — strictly better than the printed
//! card. Every test here is about that difference.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, valid_actions, Action, CardDatabase, CardId, CardInstance,
    CostPayment, FunctionalId, GameState, PlayerId, StackObjectKind, Step,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// Player 0's precombat main, pools stocked so mana never decides anything here, and a
/// library deep enough that drawing two is not a loss.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in [
            sage_engine::Color::White,
            sage_engine::Color::Blue,
            sage_engine::Color::Black,
            sage_engine::Color::Red,
            sage_engine::Color::Green,
        ] {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    let filler = cid(db, "forest");
    for seat in [PlayerId(0), PlayerId(1)] {
        for _ in 0..20 {
            let instance = state.new_instance(filler);
            state.players[seat.0].library.push(instance);
        }
    }
    state
}

/// Give player 0 a hand of exactly these cards, in order.
fn hand_of(state: &mut GameState, db: &CardDatabase, slugs: &[&str]) -> Vec<CardInstance> {
    let instances: Vec<CardInstance> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[0].hand = instances.clone();
    instances
}

/// Whether player 0 is currently offered a cast of `card`.
fn offers_cast(state: &GameState, db: &CardDatabase, card: CardInstance) -> bool {
    valid_actions(state, db).contains(&Action::CastSpell {
        card,
        targets: Vec::new(),
        payment: Vec::new(),
    })
}

#[test]
fn tormenting_voice_is_uncastable_with_no_other_card_to_discard() {
    // The cost is a gate on the *offer*, which is the difference a cost makes. A hand
    // of nothing but the spell itself cannot pay for it: the card on its way to the
    // stack is not a card in hand to discard (CR 601.2b/601.2h).
    let db = db();
    let mut state = main_phase(&db);
    let hand = hand_of(&mut state, &db, &["tormenting_voice"]);
    let voice = hand[0];

    assert!(
        !offers_cast(&state, &db, voice),
        "a hand holding only the spell cannot pay its additional cost"
    );

    // Forging the action anyway changes nothing: `apply_action` re-derives the offer.
    let forged = apply_action(
        &state,
        &Action::CastSpell {
            card: voice,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(forged, state, "the illegal cast was a no-op");

    // One more card in hand and the same spell becomes castable.
    let with_fodder = {
        let mut state = state.clone();
        let extra = state.new_instance(cid(&db, "murder"));
        state.players[0].hand.push(extra);
        state
    };
    assert!(offers_cast(&with_fodder, &db, voice));
}

#[test]
fn the_discard_rides_in_the_payment_and_the_whole_cast_is_one_action() {
    // A cost is paid while the spell is cast, not when it resolves — and, since the
    // payment carries the choice, not in a question posed afterwards either. One
    // `apply_action`: the card is discarded, the spell is on the stack, and no seat ever
    // held a window in between because there was never a moment between.
    let db = db();
    let mut state = main_phase(&db);
    let hand = hand_of(&mut state, &db, &["tormenting_voice", "murder"]);
    let (voice, murder) = (hand[0], hand[1]);

    let cast = apply_action(
        &state,
        &Action::CastSpell {
            card: voice,
            targets: Vec::new(),
            payment: vec![CostPayment::Discard(murder.id)],
        },
        &db,
    );
    assert!(
        cast.stack.iter().any(|o| matches!(
            o.kind,
            StackObjectKind::Spell { card } if card.id == voice.id
        )),
        "the spell is on the stack"
    );
    assert!(cast.players[0].hand.is_empty(), "the Murder was discarded");
    assert!(cast.players[0].graveyard.iter().any(|c| c.id == murder.id));
    assert!(
        pending_player_choice(&cast).is_none(),
        "the cost was paid as part of casting, so nothing is owed afterwards"
    );

    // Only now does the spell resolve, and only then does it draw.
    let resolved = apply_action(&cast, &Action::PassPriority, &db);
    let resolved = apply_action(&resolved, &Action::PassPriority, &db);
    assert_eq!(resolved.players[0].hand.len(), 2, "drew two");
    assert!(resolved.players[0]
        .graveyard
        .iter()
        .any(|c| c.id == voice.id));
}

#[test]
fn a_cast_that_names_no_discard_is_a_no_op() {
    // **The undo property, for a non-mana cost.** A player part-way through assembling a
    // payment has named the spell but not yet what to pay with, and that submission does
    // nothing at all: no card leaves hand, nothing reaches the stack. Which is what makes
    // abandoning it free — there is nothing to take back.
    let db = db();
    let mut state = main_phase(&db);
    let hand = hand_of(&mut state, &db, &["tormenting_voice", "murder"]);
    let voice = hand[0];

    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: voice,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(after, state, "an unpaid additional cost casts nothing");
}

#[test]
fn the_spell_being_cast_cannot_pay_for_itself() {
    // CR 601.2b/601.2h: the card is on its way to the stack, so it is not a card in hand
    // to discard. Naming it is refused rather than half-applied.
    let db = db();
    let mut state = main_phase(&db);
    let hand = hand_of(&mut state, &db, &["tormenting_voice", "murder"]);
    let voice = hand[0];

    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: voice,
            targets: Vec::new(),
            payment: vec![CostPayment::Discard(voice.id)],
        },
        &db,
    );
    assert_eq!(
        after, state,
        "a spell cannot discard itself to its own cost"
    );
}

#[test]
fn a_discard_is_refused_on_a_card_that_owes_no_additional_cost() {
    // A cost is paid exactly, in both directions: a spell with no additional cost is not
    // paid *more* than it asks for, so a payment naming a spare discard is illegal rather
    // than generously accepted.
    let db = db();
    let mut state = main_phase(&db);
    let hand = hand_of(&mut state, &db, &["murder", "tormenting_voice"]);
    let (murder, voice) = (hand[0], hand[1]);
    // Give Murder something to kill so its target slot is fillable.
    let bear = state.new_instance(cid(&db, "colossal_dreadmaw"));
    let bear_id = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: bear_id,
        instance: bear.id,
        printed: bear.card.into(),
        controller: PlayerId(1),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: None,
        skips_untap: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
    });
    let victim = state.battlefield[0].id;

    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: murder,
            targets: vec![sage_engine::Target::Permanent(victim)],
            payment: vec![CostPayment::Discard(voice.id)],
        },
        &db,
    );
    assert_eq!(after, state, "Murder has no additional cost to pay");
}
