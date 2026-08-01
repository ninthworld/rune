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
    apply_action, choice_candidates, pending_player_choice, valid_actions, Action, CardDatabase,
    CardId, CardInstance, CardInstanceId, FunctionalId, GameState, PlayerId, StackObjectKind, Step,
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
fn the_discard_is_paid_at_cast_time_before_anyone_can_respond() {
    // A cost is paid while the spell is cast, not when it resolves: the card is gone
    // from hand the moment the spell hits the stack, and no seat gets priority in
    // between — the pending choice is the only legal action anyone has.
    let db = db();
    let mut state = main_phase(&db);
    let hand = hand_of(&mut state, &db, &["tormenting_voice", "murder"]);
    let (voice, murder) = (hand[0], hand[1]);

    let cast = apply_action(
        &state,
        &Action::CastSpell {
            card: voice,
            targets: Vec::new(),
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

    // The cost is owed, by its caster, and it is all anyone may do.
    let pending = pending_player_choice(&cast).expect("the discard is owed");
    assert_eq!(pending.chooser, PlayerId(0));
    assert_eq!(cast.priority, PlayerId(0));
    let offered = valid_actions(&cast, &db);
    assert!(
        !offered.contains(&Action::PassPriority),
        "nobody may pass, respond, or cast anything until the cost is paid"
    );

    // The spell itself is not a candidate for its own cost — it is on the stack.
    let candidates = choice_candidates(&cast, pending.question.cards().unwrap(), &db);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, murder.id);

    let chosen: Vec<CardInstanceId> = vec![murder.id];
    let paid = apply_action(&cast, &Action::AnswerChoice { chosen }, &db);
    assert!(paid.players[0].hand.is_empty(), "the Murder was discarded");
    assert!(paid.players[0].graveyard.iter().any(|c| c.id == murder.id));
    assert!(
        paid.stack.iter().any(|o| matches!(
            o.kind,
            StackObjectKind::Spell { card } if card.id == voice.id
        )),
        "paying a cost does not resolve the spell — it is still on the stack"
    );

    // Only now does the spell resolve, and only then does it draw.
    let resolved = apply_action(&paid, &Action::PassPriority, &db);
    let resolved = apply_action(&resolved, &Action::PassPriority, &db);
    assert_eq!(resolved.players[0].hand.len(), 2, "drew two");
    assert!(resolved.players[0]
        .graveyard
        .iter()
        .any(|c| c.id == voice.id));
}
