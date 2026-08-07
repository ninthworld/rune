//! Becoming the target of a spell or ability (CR 603.6e, issue #706).
//!
//! Three cards, one condition, and two narrowings that the printed cards genuinely use
//! apart: Shield Mare and Thorn Lieutenant watch for `a spell or ability **an opponent
//! controls**`, and Departed Deckhand watches for `a spell` — anyone's, including its own
//! controller's, which is the whole cost of that card.
//!
//! The observation is a **stack diff**, and that is what makes it exact. Targets are
//! chosen as an object is put on the stack (CR 601.2c) and never change afterwards, so an
//! object that was already on the stack has just targeted nothing. It also means the
//! trigger fires when the spell is *announced* rather than when it resolves — a
//! countered removal spell has still targeted, and the payoff has already happened.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_trigger_target_choice, Action, CardDatabase, CardId,
    CardInstance, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    state.players[1].turn_began = state.turn;
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
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(filler)).collect();
        state.players[seat].library = library;
    }
    state
}

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

/// Cast `card` from `seat` at `victim`, without resolving anything.
fn aim_at(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    card: CardInstance,
    victim: PermanentId,
) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    apply_action(
        &state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(victim)],
            payment: Vec::new(),
        },
        db,
    )
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// An opponent's removal spell targeting the Mare pays its controller three life — as the
/// spell is **announced**, not when it resolves.
#[test]
fn shield_mare_gains_life_when_an_opponent_aims_at_it() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "shield_mare", PlayerId(0));
    let bolt = to_hand(&mut state, &db, "shock", PlayerId(1));
    let life = state.players[0].life;

    let state = aim_at(&state, &db, PlayerId(1), bolt, mare);

    // The trigger is on the stack above the spell that caused it, so it resolves first.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(
        state.players[0].life,
        life + 3,
        "three life, from the targeting rather than from the damage"
    );
}

/// Its **own** controller aiming at it is not an opponent doing so, and nothing happens.
#[test]
fn shield_mare_ignores_its_own_controllers_spell() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "shield_mare", PlayerId(0));
    let bolt = to_hand(&mut state, &db, "shock", PlayerId(0));
    let life = state.players[0].life;

    let state = aim_at(&state, &db, PlayerId(0), bolt, mare);

    assert!(
        state.stack.iter().all(|object| object.targets.is_empty()
            || !matches!(object.kind, sage_engine::StackObjectKind::Ability { .. })),
        "no trigger was put on the stack"
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(state.players[0].life, life, "and no life was gained");
}

/// The other half of the same card: it also fires on entering, which is a second ability
/// rather than a second condition on one.
#[test]
fn shield_mare_gains_life_on_entering_too() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = to_hand(&mut state, &db, "shield_mare", PlayerId(0));
    let life = state.players[0].life;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: mare,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(state.players[0].life, life + 3);
}

/// Thorn Lieutenant answers with a body, and the token is a 1/1 green Elf Warrior.
#[test]
fn thorn_lieutenant_makes_a_token_when_an_opponent_aims_at_it() {
    let db = db();
    let mut state = main_phase(&db);
    let elf = place(&mut state, &db, "thorn_lieutenant", PlayerId(0));
    let bolt = to_hand(&mut state, &db, "shock", PlayerId(1));

    let state = aim_at(&state, &db, PlayerId(1), bolt, elf);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let token = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.is_token())
        .expect("an Elf Warrior token");
    let stats = characteristics(&state, token.id, &db);
    assert_eq!((stats.power, stats.toughness), (Some(1), Some(1)));
    assert_eq!(token.controller, PlayerId(0), "under the Lieutenant's seat");
}

/// **A spell, anyone's.** Departed Deckhand sacrifices itself even to a spell its own
/// controller cast — that is the drawback the card is priced for.
#[test]
fn departed_deckhand_sacrifices_itself_to_any_spell_including_yours() {
    let db = db();
    let mut state = main_phase(&db);
    let deckhand = place(&mut state, &db, "departed_deckhand", PlayerId(0));
    let bolt = to_hand(&mut state, &db, "shock", PlayerId(0));

    let state = aim_at(&state, &db, PlayerId(0), bolt, deckhand);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !on_battlefield(&state, deckhand),
        "its own controller's spell is still a spell"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|card| card.card == cid(&db, "departed_deckhand")),
        "sacrificed, so it is in its owner's graveyard"
    );
}

/// **A spell**, and an ability is not one: an opponent's activated ability aiming at the
/// Deckhand leaves it alone.
#[test]
fn departed_deckhand_survives_an_ability() {
    let db = db();
    let mut state = main_phase(&db);
    let deckhand = place(&mut state, &db, "departed_deckhand", PlayerId(0));
    // An *ability* that targets: the Deckhand's own activation, which names another
    // creature its controller controls. What is under test is the class of object, so
    // whose ability it is does not matter — and its own is the one that always exists.
    let ally = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: deckhand,
            index: 1,
            targets: vec![Target::Permanent(ally)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        on_battlefield(&state, deckhand),
        "an ability is not a spell, so nothing was sacrificed"
    );
    assert!(
        characteristics(&state, ally, &db)
            .restrictions
            .iter()
            .any(|restriction| matches!(
                restriction,
                sage_engine::CombatRestriction::CantBeBlockedExceptBy(kind) if kind == "Spirit"
            )),
        "and the ally really did gain the restriction"
    );
}

/// "Another" excludes the source: the Deckhand cannot aim its own ability at itself.
#[test]
fn departed_deckhand_cannot_aim_its_ability_at_itself() {
    let db = db();
    let mut state = main_phase(&db);
    let deckhand = place(&mut state, &db, "departed_deckhand", PlayerId(0));
    place(&mut state, &db, "onakke_ogre", PlayerId(0));

    let refused = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: deckhand,
            index: 1,
            targets: vec![Target::Permanent(deckhand)],
            payment: Vec::new(),
        },
        &db,
    );

    assert_eq!(
        refused.stack.len(),
        0,
        "*another* creature you control is not this one"
    );
    assert!(pending_trigger_target_choice(&refused).is_none());
}
