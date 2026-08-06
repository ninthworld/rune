//! `as long as it hasn't dealt damage yet` — the static-side condition, and the one
//! whose window is the permanent's whole life (issue #727, CR 604.3 / CR 613.1f).
//!
//! Palladia-Mors, the Ruiner has hexproof until the moment it deals damage, and this is
//! deliberately **not** an intervening if: there is no resolution to read, no window to
//! measure from, and nothing goes on the stack in either direction. The grant is derived
//! on every read of the permanent's characteristics, so it appears and disappears with
//! the answer and leaves nothing behind — which is what the tests below assert as
//! directly as they assert the hexproof itself.
//!
//! Every transition here goes through the real [`apply_action`] pipeline, because the
//! interesting instant is inside one: the combat-damage step is a turn-based action, and
//! the hexproof has to be gone by the time the state-based actions after it run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, target_requirements, valid_actions, Action, Attack,
    AttackTarget, CardDatabase, CardId, Color, FunctionalId, GameState, Keyword, Modification,
    Permanent, PermanentId, PlayerId, Step, Target,
};

/// Enough actions to walk a whole turn; a walk that has not arrived by then is a hang.
const ACTION_CAP: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at the start of turn 1, both libraries stocked so the walk never
/// trips the CR 704.5c decking loss.
fn fresh_game(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let library: Vec<_> = (0..12)
            .map(|_| state.new_instance(cid(db, "colossal_dreadmaw")))
            .collect();
        state.players[seat].library = library;
    }
    state
}

/// Put a permanent of `slug` on the battlefield under `controller`, entered on turn 0 so
/// it may attack from turn 1 (CR 302.6).
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

fn walk_action(state: &GameState) -> Action {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Action::DeclareAttackers {
            attackers: Vec::new(),
        },
        Step::DeclareBlockers if !state.blockers_declared => {
            Action::DeclareBlockers { blocks: Vec::new() }
        }
        _ => Action::PassPriority,
    }
}

fn walk_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..ACTION_CAP {
        if done(&state) {
            return state;
        }
        let next = apply_action(&state, &walk_action(&state), db);
        assert_ne!(next, state, "the walk made no progress");
        state = next;
    }
    panic!("the walk ran past its cap without reaching the goal");
}

/// Whether `id` currently has hexproof, read the way every rule reads a keyword: through
/// the computed characteristics (CR 613.1f), fresh on this call.
fn has_hexproof(state: &GameState, db: &CardDatabase, id: PermanentId) -> bool {
    characteristics(state, id, db)
        .keywords
        .contains(&Keyword::Hexproof)
}

/// Fill `seat`'s pool with enough of every colour to cast anything below. The pool empties
/// at each step boundary (CR 500.4), so the cast has to happen in the same step.
fn with_mana(state: &mut GameState, seat: PlayerId) {
    for color in [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ] {
        state.players[seat.0].mana_pool.add(color, 10);
    }
    state.players[seat.0].mana_pool.add_colorless(10);
}

/// The permanents seat 1 is offered a Murder against — the observable half of hexproof
/// (CR 702.11b), asked through the same [`valid_actions`] a client reads.
///
/// The state is rewound to a main phase with seat 1 holding priority, because that is
/// where a client would be looking; nothing about the dragon changes on the way, and the
/// tests below assert against a *control* creature in the same list so an empty answer can
/// never pass for hexproof.
fn murder_targets(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let mut state = state.clone();
    state.step = Step::PostcombatMain;
    state.stack.clear();
    state.priority = PlayerId(1);
    with_mana(&mut state, PlayerId(1));
    let murder = state.new_instance(cid(db, "murder"));
    state.players[1].hand.push(murder);
    let cast = valid_actions(&state, db)
        .into_iter()
        .find(|action| matches!(action, Action::CastSpell { card, .. } if card.id == murder.id))
        .expect("seat 1 can afford the Murder in its hand");
    target_requirements(&state, db, &cast)
        .into_iter()
        .flat_map(|requirement| requirement.candidates)
        .filter_map(|target| match target {
            Target::Permanent(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// Seat 0 attacks seat 1 with `attacker` on turn 1 and the game runs into the
/// combat-damage step, where the turn-based damage assignment has landed.
fn attack_into_damage(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let state = walk_until(state, db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    walk_until(&state, db, |s| s.step == Step::CombatDamage)
}

/// Before it has dealt anything, the condition holds and the keyword is really there —
/// an opponent's removal cannot even be aimed at it.
#[test]
fn issue_727_palladia_mors_has_hexproof_before_it_deals_damage() {
    let db = db();
    let mut state = fresh_game(&db);
    let dragon = place(&mut state, &db, "palladia_mors_the_ruiner", PlayerId(0));
    // The control: an identically-placed creature with no hexproof, so "no offer" cannot
    // pass for "hexproof" if the offer machinery were simply silent here.
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));

    assert!(has_hexproof(&state, &db, dragon));
    let offered = murder_targets(&state, &db);
    assert!(
        offered.contains(&ogre),
        "an opponent's removal is offered against a creature without hexproof"
    );
    assert!(
        !offered.contains(&dragon),
        "hexproof keeps that same spell off the dragon (CR 702.11b)"
    );
}

/// And it stops the instant the damage lands. Nothing resolved in between: the
/// combat-damage step is a turn-based action, so the answer changed with no object on the
/// stack and no window to have latched it in.
#[test]
fn issue_727_the_hexproof_stops_the_instant_palladia_mors_deals_damage() {
    let db = db();
    let mut state = fresh_game(&db);
    let dragon = place(&mut state, &db, "palladia_mors_the_ruiner", PlayerId(0));

    let before = walk_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    assert!(
        has_hexproof(&before, &db, dragon),
        "declaring an attack is not dealing damage"
    );

    let after = attack_into_damage(&state, &db, dragon);
    assert_eq!(
        after.players[1].life, 14,
        "six damage from a 6/6 landed on the defending player"
    );
    assert!(
        !has_hexproof(&after, &db, dragon),
        "it has dealt damage, so the condition no longer holds"
    );
    assert!(
        murder_targets(&after, &db).contains(&dragon),
        "and the removal an opponent could not aim a moment ago is offered now"
    );
}

/// The condition is re-derived on **every read**, never latched by a resolution. Two
/// witnesses: nothing is written into the stored continuous effects in either state — so
/// there is no grant to have gone stale — and the answer flips across a transition in
/// which no object resolved at all.
#[test]
fn issue_727_the_hexproof_is_derived_on_every_read_rather_than_stored() {
    let db = db();
    let mut state = fresh_game(&db);
    let dragon = place(&mut state, &db, "palladia_mors_the_ruiner", PlayerId(0));

    let granted_hexproof = |s: &GameState| {
        s.static_effects
            .iter()
            .any(|effect| effect.modification == Modification::GrantKeyword(Keyword::Hexproof))
    };
    assert!(
        !granted_hexproof(&state),
        "a static ability is derived, so nothing is stored for it (ADR 0005 §1)"
    );
    assert!(has_hexproof(&state, &db, dragon));
    assert!(
        has_hexproof(&state, &db, dragon),
        "asking twice gives the same answer, from the state and not from a cache"
    );

    let after = attack_into_damage(&state, &db, dragon);
    assert!(
        !granted_hexproof(&after),
        "and nothing was stored to be taken away either"
    );
    assert!(!has_hexproof(&after, &db, dragon));
}

/// A permanent that has dealt damage keeps that fact for the rest of its life on the
/// battlefield: "yet" has no end, and no turn boundary gives the hexproof back.
#[test]
fn issue_727_having_dealt_damage_does_not_wear_off_at_the_turn_boundary() {
    let db = db();
    let mut state = fresh_game(&db);
    let dragon = place(&mut state, &db, "palladia_mors_the_ruiner", PlayerId(0));

    let state = attack_into_damage(&state, &db, dragon);
    assert!(!has_hexproof(&state, &db, dragon));

    let state = walk_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    assert!(
        state.battlefield.iter().any(|perm| perm.id == dragon),
        "the dragon is still on the battlefield two turns later"
    );
    assert!(
        !has_hexproof(&state, &db, dragon),
        "the window is the object's whole life, not a turn"
    );
}
