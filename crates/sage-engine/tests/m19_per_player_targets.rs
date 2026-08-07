//! Vaevictis Asmadi, the Dire (issue #706): the first ability whose **number of target
//! slots comes from the table** rather than from the card.
//!
//! `For each player, choose target permanent that player controls` is the one sentence in
//! the catalog that cannot be written down as a fixed list of slots. Every other targeting
//! effect declares what it declares — one slot, two, up to two — and the answer is a
//! property of the effect. This one declares as many as there are seats, so
//! [`Effect::target_groups`] is given the seat count and builds them.
//!
//! Nothing about it is written for two players. The groups are a `map` over the seat
//! range, each carrying the seat it belongs to; the sacrifice reads each target's
//! controller rather than assuming whose it is; and the reveal is driven by the list of
//! players who actually lost something. A third seat would declare a third slot and
//! sacrifice a third permanent without a line of this changing.
//!
//! The other half of the card is CR 608.2b, which is where the per-seat shape stops being
//! cosmetic: a fight's two slots stand or fall together (CR 701.12c), but these are one
//! sentence about *separate* people, so one seat's target being gone in response says
//! nothing about the others and the ability does as much as it still can.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, Color, Effect, FunctionalId,
    GameState, Permanent, PermanentId, PlayerId, Step, Target, TargetSpec,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// Seat 0's precombat main phase, with mana and a library for each seat.
fn main_phase(_db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
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
    state
}

/// Put `count` copies of `slug` on top of `seat`'s library — the **end** of the Vec is the
/// top, so the last one pushed is the first one revealed.
fn stack_library(state: &mut GameState, db: &CardDatabase, seat: usize, slug: &str, count: usize) {
    let card = cid(db, slug);
    for _ in 0..count {
        let instance = state.new_instance(card);
        state.players[seat].library.push(instance);
    }
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

/// Vaevictis on the battlefield and able to attack.
fn vaevictis(state: &mut GameState, db: &CardDatabase) -> PermanentId {
    let id = place(state, db, "vaevictis_asmadi_the_dire", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == id) {
        perm.entered_turn = 0;
    }
    id
}

/// Walk to declare-attackers, attack with `attacker`, and stop at the first question.
fn attack_with(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let mut state = state.clone();
    for _ in 0..40 {
        if state.step == Step::DeclareAttackers && !state.attackers_declared {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    assert_eq!(state.step, Step::DeclareAttackers, "combat was reached");
    apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![sage_engine::Attack {
                attacker,
                defender: sage_engine::AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    )
}

/// The `ChooseTriggerTargets` action currently on offer, and its slots.
fn aiming_action(state: &GameState, db: &CardDatabase) -> Action {
    valid_actions(state, db)
        .into_iter()
        .find(|action| matches!(action, Action::ChooseTriggerTargets { .. }))
        .expect("the attack trigger is waiting to be aimed")
}

/// Pass priority until the stack empties or somebody is asked something.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..16 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

fn permanents_of(state: &GameState, seat: PlayerId) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.controller == seat)
        .count()
}

/// **The crux, stated without a board.** The effect declares one required slot per seat,
/// each naming that seat's permanents — and it says so for two seats, three, and five,
/// because the answer is a `map` over the seat range and nothing else.
#[test]
fn issue_706_the_effect_declares_one_target_group_per_seat() {
    let effect = Effect::SacrificeChosenPerPlayer { reveal_top: true };
    for seats in [1usize, 2, 3, 5] {
        let groups = effect.target_groups(seats);
        assert_eq!(
            groups.len(),
            seats,
            "one group per seat at a table of {seats}"
        );
        for (seat, group) in groups.iter().enumerate() {
            assert_eq!(
                group.spec,
                TargetSpec::PermanentThatPlayerControls { seat },
                "group {seat} names seat {seat}'s permanents"
            );
            assert_eq!(
                (group.min, group.max),
                (1, 1),
                "and it is one required slot"
            );
        }
    }
}

/// Every other effect ignores the seat count entirely — the parameter changed the
/// vocabulary's *shape*, not any existing card's meaning.
#[test]
fn issue_706_every_other_effect_answers_the_same_at_any_table_size() {
    let db = db();
    for (_, data) in db.all() {
        for seats in [2usize, 4] {
            assert_eq!(
                data.cast_target_groups(None, 2),
                data.cast_target_groups(None, seats),
                "{} declares the same cast slots at any table size",
                data.functional_id
            );
        }
    }
}

/// **The crux on a board.** Attacking asks for one permanent per seat, and both are
/// sacrificed — each by the player who controls it, not by the attacker.
#[test]
fn issue_706_each_player_sacrifices_the_permanent_chosen_for_them() {
    let db = db();
    let mut state = main_phase(&db);
    let dragon = vaevictis(&mut state, &db);
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    let theirs = place(&mut state, &db, "millstone", PlayerId(1));
    // Neither library has a permanent card on top, so the reveal is inert here and the
    // sacrifice is the whole of what is measured.
    stack_library(&mut state, &db, 0, "shock", 3);
    stack_library(&mut state, &db, 1, "shock", 3);

    let state = attack_with(&state, &db, dragon);
    let Action::ChooseTriggerTargets { ability, .. } = aiming_action(&state, &db) else {
        panic!("the trigger is aimed by ChooseTriggerTargets");
    };
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
        },
        &db,
    );
    let state = settle(&state, &db);

    assert!(!on_battlefield(&state, mine), "seat 0 lost the one chosen");
    assert!(!on_battlefield(&state, theirs), "and so did seat 1");
    assert!(
        on_battlefield(&state, dragon),
        "the attacker itself was never a slot"
    );
    assert_eq!(
        state.players[0].graveyard.len() + state.players[1].graveyard.len(),
        2,
        "each permanent reached its own owner's graveyard"
    );
}

/// A slot may only name the seat it belongs to: seat 0's slot cannot be aimed at seat 1's
/// permanent, however the attacker would prefer it.
#[test]
fn issue_706_a_slot_can_only_name_its_own_seats_permanents() {
    let db = db();
    let mut state = main_phase(&db);
    let dragon = vaevictis(&mut state, &db);
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    let theirs = place(&mut state, &db, "millstone", PlayerId(1));
    stack_library(&mut state, &db, 0, "shock", 3);
    stack_library(&mut state, &db, 1, "shock", 3);

    let state = attack_with(&state, &db, dragon);
    let Action::ChooseTriggerTargets { ability, .. } = aiming_action(&state, &db) else {
        panic!("the trigger is aimed by ChooseTriggerTargets");
    };
    // Both slots aimed at the opponent's board — the second slot's spec allows it, the
    // first's does not.
    let swapped = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(theirs), Target::Permanent(theirs)],
        },
        &db,
    );
    assert_eq!(
        swapped, state,
        "an illegal aim is refused outright, leaving the state untouched"
    );
    // And the honest aim is accepted, so the refusal above is about the seats and not
    // about the action being unavailable.
    let aimed = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
        },
        &db,
    );
    assert_ne!(aimed, state, "one permanent per seat is the legal aim");
}

/// **The half a fight does not have.** One seat's chosen permanent leaving in response
/// does not spare the other's: this is one sentence about separate people, so CR 608.2b's
/// ordinary rule applies and the ability does as much as it still can.
#[test]
fn issue_706_a_target_lost_in_response_does_not_spare_the_others() {
    let db = db();
    let mut state = main_phase(&db);
    let dragon = vaevictis(&mut state, &db);
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    let theirs = place(&mut state, &db, "millstone", PlayerId(1));
    stack_library(&mut state, &db, 0, "shock", 3);
    stack_library(&mut state, &db, 1, "shock", 3);

    let state = attack_with(&state, &db, dragon);
    let Action::ChooseTriggerTargets { ability, .. } = aiming_action(&state, &db) else {
        panic!("the trigger is aimed by ChooseTriggerTargets");
    };
    let mut state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
        },
        &db,
    );
    // Seat 0's own chosen permanent is removed while the trigger is still on the stack —
    // the slot it filled is now illegal, and the other slot is untouched.
    state.battlefield.retain(|perm| perm.id != mine);
    let state = settle(&state, &db);

    assert!(
        !on_battlefield(&state, theirs),
        "the seat whose target survived still lost it"
    );
    assert!(
        on_battlefield(&state, dragon),
        "and the ability resolved rather than being removed"
    );
}

/// The second sentence: a player who sacrificed reveals the top card of their library and
/// puts it onto the battlefield when it is a permanent card.
#[test]
fn issue_706_a_revealed_permanent_card_replaces_what_was_sacrificed() {
    let db = db();
    let mut state = main_phase(&db);
    let dragon = vaevictis(&mut state, &db);
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    let theirs = place(&mut state, &db, "millstone", PlayerId(1));
    // A permanent card on top of each library — the last pushed is the top.
    stack_library(&mut state, &db, 0, "shock", 2);
    stack_library(&mut state, &db, 0, "grasping_scoundrel", 1);
    stack_library(&mut state, &db, 1, "shock", 2);
    stack_library(&mut state, &db, 1, "grasping_scoundrel", 1);
    let before = (
        permanents_of(&state, PlayerId(0)),
        permanents_of(&state, PlayerId(1)),
    );

    let state = attack_with(&state, &db, dragon);
    let Action::ChooseTriggerTargets { ability, .. } = aiming_action(&state, &db) else {
        panic!("the trigger is aimed by ChooseTriggerTargets");
    };
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
        },
        &db,
    );
    let state = settle(&state, &db);

    assert!(!on_battlefield(&state, mine));
    assert!(!on_battlefield(&state, theirs));
    assert_eq!(
        (
            permanents_of(&state, PlayerId(0)),
            permanents_of(&state, PlayerId(1))
        ),
        before,
        "one permanent out and one in, for each seat"
    );
    for seat in 0..2 {
        assert_eq!(
            state.players[seat].library.len(),
            2,
            "the revealed card left seat {seat}'s library"
        );
    }
}

/// A revealed **nonpermanent** card is revealed and nothing more: the card says what
/// happens to a permanent card and says nothing about any other, so it stays on top.
#[test]
fn issue_706_a_revealed_nonpermanent_card_stays_on_top() {
    let db = db();
    let mut state = main_phase(&db);
    let dragon = vaevictis(&mut state, &db);
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    let theirs = place(&mut state, &db, "millstone", PlayerId(1));
    stack_library(&mut state, &db, 0, "shock", 3);
    stack_library(&mut state, &db, 1, "shock", 3);

    let state = attack_with(&state, &db, dragon);
    let Action::ChooseTriggerTargets { ability, .. } = aiming_action(&state, &db) else {
        panic!("the trigger is aimed by ChooseTriggerTargets");
    };
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
        },
        &db,
    );
    let state = settle(&state, &db);

    for seat in 0..2 {
        assert_eq!(
            state.players[seat].library.len(),
            3,
            "seat {seat}'s revealed instant went back where it was"
        );
        assert!(
            state.players[seat].graveyard.len() == 1,
            "and only the sacrificed permanent is in the graveyard"
        );
    }
}

/// CR 603.3c: a player controlling nothing leaves their slot with no legal choice, and
/// the whole ability is removed from the stack rather than resolving for the seats that
/// could have answered.
#[test]
fn issue_706_a_seat_with_no_permanents_removes_the_whole_trigger() {
    let db = db();
    let mut state = main_phase(&db);
    let dragon = vaevictis(&mut state, &db);
    let mine = place(&mut state, &db, "bogstomper", PlayerId(0));
    stack_library(&mut state, &db, 0, "shock", 3);
    stack_library(&mut state, &db, 1, "shock", 3);
    // Seat 1 controls nothing at all.

    let state = attack_with(&state, &db, dragon);
    assert!(
        !state
            .stack
            .iter()
            .any(|object| object.controller == PlayerId(0)
                && matches!(object.kind, sage_engine::StackObjectKind::Ability { .. })),
        "the trigger never went on the stack"
    );
    let state = settle(&state, &db);
    assert!(
        on_battlefield(&state, mine),
        "and seat 0 kept the permanent its own slot would have named"
    );
}
