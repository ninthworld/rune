//! Ghostform (M19 #58): "up to two target creatures" on an effect that is **not**
//! `put_counters` (issue #748).
//!
//! The arity machinery already existed (ADR 0017 §5) — a group of `{spec, min: 0, max}`,
//! `max` wire slots of which the ones past `min` are optional, and a CR 608.2c re-check
//! per target. What this file proves is that it is the *effect's* field rather than one
//! verb's private trick: the same field on `restrict` yields the same three legal counts,
//! the same refusal of a third, and the same until-end-of-turn duration a single-target
//! restriction has always had (CR 514.2).
//!
//! Every test drives the real [`apply_action`] over the bundled catalog.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, permanent_restrictions, target_requirements, valid_actions, Action, Attack,
    AttackTarget, Block, CardDatabase, CardId, CardInstance, Color, CombatRestriction,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at seat 0's precombat main, pools stocked so payability never
/// decides a test that is about arity.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
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

/// Put Ghostform in seat 0's hand and return the state plus the instance, so a test can
/// aim it several ways from one board.
fn with_ghostform(state: &GameState, db: &CardDatabase) -> (GameState, CardInstance) {
    let mut state = state.clone();
    let card = to_hand(&mut state, db, "ghostform", PlayerId(0));
    (state, card)
}

/// Cast Ghostform at `targets` and let it resolve. Returns `None` when the announcement
/// was refused — `apply_action` returning the state it was handed is the engine's way of
/// saying "illegal", and a test about arity needs to tell that apart from "resolved and
/// did nothing".
fn cast_at(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    targets: Vec<Target>,
) -> Option<GameState> {
    let after = apply_action(
        state,
        &Action::CastSpell {
            card,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    if &after == state {
        return None;
    }
    let after = apply_action(&after, &Action::PassPriority, db);
    Some(apply_action(&after, &Action::PassPriority, db))
}

fn unblockable(state: &GameState, db: &CardDatabase, id: PermanentId) -> bool {
    permanent_restrictions(state, id, db).contains(&CombatRestriction::CantBeBlocked)
}

/// Walk the game forward one action at a time on behalf of whoever holds priority.
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

// ----- the three legal counts ------------------------------------------------

#[test]
fn issue_748_ghostform_offers_two_slots_of_which_neither_is_required() {
    // CR 601.2c through ADR 0017 §5: one group of `max` slots, and every slot past the
    // group's minimum carries `optional`. A minimum of zero means both of them do — and
    // it is why the spell is offered at all with nothing worth aiming at.
    let db = db();
    let mut state = main_phase();
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let theirs = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let (state, card) = with_ghostform(&state, &db);

    let slots = target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    assert_eq!(slots.len(), 2, "up to two, so two slots");
    assert!(
        slots.iter().all(|slot| slot.optional),
        "and no slot the player must fill"
    );
    // "Target creature" is not "target creature you control": either seat's is legal.
    assert!(slots[0].candidates.contains(&Target::Permanent(mine)));
    assert!(slots[0].candidates.contains(&Target::Permanent(theirs)));
}

#[test]
fn issue_748_ghostform_is_legal_at_zero_one_and_two_targets() {
    // The three counts a `{min: 0, max: 2}` group accepts, each cast from the same
    // board, each asserted by what it did rather than by what it was offered.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let second = place(&mut state, &db, "gallant_cavalry", PlayerId(0));
    let (state, card) = with_ghostform(&state, &db);

    let none = cast_at(&state, &db, card, Vec::new()).expect("no targets is a legal cast");
    assert!(!unblockable(&none, &db, first));
    assert!(!unblockable(&none, &db, second));
    assert!(none.stack.is_empty(), "and the spell finished resolving");

    let one = cast_at(&state, &db, card, vec![Target::Permanent(first)]).expect("one target");
    assert!(unblockable(&one, &db, first));
    assert!(
        !unblockable(&one, &db, second),
        "the creature it was not aimed at is untouched"
    );

    let two = cast_at(
        &state,
        &db,
        card,
        vec![Target::Permanent(first), Target::Permanent(second)],
    )
    .expect("two targets");
    assert!(unblockable(&two, &db, first));
    assert!(unblockable(&two, &db, second));
}

#[test]
fn issue_748_ghostform_refuses_a_third_target() {
    // The maximum is a maximum. A third target is not trimmed, ignored, or applied to
    // two of the three — the whole announcement is illegal and nothing happens.
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let second = place(&mut state, &db, "gallant_cavalry", PlayerId(0));
    let third = place(&mut state, &db, "walking_corpse", PlayerId(0));
    let (state, card) = with_ghostform(&state, &db);

    assert!(
        cast_at(
            &state,
            &db,
            card,
            vec![
                Target::Permanent(first),
                Target::Permanent(second),
                Target::Permanent(third),
            ],
        )
        .is_none(),
        "three targets fill a two-slot group and are refused"
    );
    // Repeating a target inside one group is the other way to overfill it (CR 601.2c).
    assert!(
        cast_at(
            &state,
            &db,
            card,
            vec![Target::Permanent(first), Target::Permanent(first)],
        )
        .is_none(),
        "and one creature cannot be both of the two",
    );
}

// ----- the duration ----------------------------------------------------------

#[test]
fn issue_748_ghostform_makes_both_creatures_unblockable_for_the_turn_only() {
    // The restriction a variable-arity `restrict` imposes is the same layer-6 grant a
    // single-target one imposes: it binds a real block declaration now (CR 509.1b) and
    // the cleanup step ends it (CR 514.2).
    let db = db();
    let mut state = main_phase();
    let first = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let second = place(&mut state, &db, "gallant_cavalry", PlayerId(0));
    let blocker = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let (state, card) = with_ghostform(&state, &db);

    let ghosted = cast_at(
        &state,
        &db,
        card,
        vec![Target::Permanent(first), Target::Permanent(second)],
    )
    .expect("two targets");

    // Attack with both; neither may be blocked.
    let mut attacking = ghosted.clone();
    attacking.step = Step::DeclareAttackers;
    attacking.priority = PlayerId(0);
    attacking.consecutive_passes = 0;
    let mut attacking = apply_action(
        &attacking,
        &Action::DeclareAttackers {
            attackers: vec![
                Attack {
                    attacker: first,
                    defender: AttackTarget::Player(PlayerId(1)),
                },
                Attack {
                    attacker: second,
                    defender: AttackTarget::Player(PlayerId(1)),
                },
            ],
        },
        &db,
    );
    while attacking.step != Step::DeclareBlockers {
        attacking = advance(&attacking, &db);
    }
    for attacker in [first, second] {
        let declaration = Action::DeclareBlockers {
            blocks: vec![Block { blocker, attacker }],
        };
        assert_eq!(
            &apply_action(&attacking, &declaration, &db),
            &attacking,
            "an unblockable attacker refuses the block",
        );
    }

    // CR 514.2: gone with the turn, both of them.
    let mut later = ghosted.clone();
    while later.turn == ghosted.turn {
        later = advance(&later, &db);
    }
    assert!(permanent_restrictions(&later, first, &db).is_empty());
    assert!(permanent_restrictions(&later, second, &db).is_empty());
}

#[test]
fn issue_748_one_dead_target_does_not_waste_the_other() {
    // CR 608.2c, which is the whole reason per-target re-checking matters more for a
    // variable-arity effect: a target that has gone is skipped and its legal sibling is
    // still restricted. Only if *every* target is illegal does the spell fizzle
    // (CR 608.2b), and a cast that chose none never had any to lose.
    let db = db();
    let mut state = main_phase();
    let doomed = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let survivor = place(&mut state, &db, "gallant_cavalry", PlayerId(0));
    let (state, card) = with_ghostform(&state, &db);

    let announced = apply_action(
        &state,
        &Action::CastSpell {
            card,
            targets: vec![Target::Permanent(doomed), Target::Permanent(survivor)],
            payment: Vec::new(),
        },
        &db,
    );
    let mut vanished = announced;
    vanished.battlefield.retain(|p| p.id != doomed);
    let resolved = apply_action(&vanished, &Action::PassPriority, &db);
    let resolved = apply_action(&resolved, &Action::PassPriority, &db);

    assert!(
        unblockable(&resolved, &db, survivor),
        "the surviving target still gets what the card promised it"
    );
    assert!(resolved.stack.is_empty(), "and the spell did not linger");
}
