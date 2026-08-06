//! Act of Treason, and the CR 613 layer-2 control change it is built on.
//!
//! The card is three sentences about one creature — take it, untap it, give it haste —
//! and the interesting claim is not any one of them but how *many* other rules read the
//! answer. A control change is the earliest layer the engine models, so proving it works
//! means proving that everything downstream of it moved at once: who may attack with the
//! creature, whose class selectors count it, where its combat damage lands, and — when
//! the effect ends at cleanup — that all of it goes back with nothing left behind.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog and
//! walks the real turn structure. Cards are named by their authored `functional_id`,
//! never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attacker_candidates, characteristics, controller_of_id, valid_actions, Action,
    Attack, AttackTarget, CardDatabase, CardId, Color, FunctionalId, GameState, Keyword,
    Modification, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk several whole turns; a settle that has not arrived by then is a
/// hang, and failing beats spinning.
const SETTLE_LIMIT: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability never
/// decides a test about an effect, and both libraries stocked so a multi-turn walk never
/// trips the CR 704.5c decking loss.
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

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free of
/// summoning sickness, and return its battlefield identity.
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

/// Cast `slug` from player 0's hand with `targets` and let it resolve. Goes through the
/// ordinary cast gate, so a spell `valid_actions` would not offer fails here rather than
/// silently doing nothing.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Steal `victim` for player 0 with Act of Treason.
fn steal(state: &GameState, db: &CardDatabase, victim: PermanentId) -> GameState {
    cast(state, db, "act_of_treason", vec![Target::Permanent(victim)])
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is.
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if done(&state) {
            return state;
        }
        let offered = valid_actions(&state, db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|action| action != &Action::Concede)
                .expect("some action is always available")
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the game never reached the state under test");
}

/// Who controls `id` right now (CR 613 layer 2), or `None` once it has left the
/// battlefield.
fn controller(state: &GameState, id: PermanentId) -> Option<PlayerId> {
    controller_of_id(state, id)
}

/// Whether `id` is on the battlefield and tapped.
fn tapped(state: &GameState, id: PermanentId) -> bool {
    state
        .battlefield
        .iter()
        .any(|perm| perm.id == id && perm.tapped)
}

/// Whether `id` currently has `keyword`, read through the computed characteristics.
fn has(state: &GameState, db: &CardDatabase, id: PermanentId, keyword: Keyword) -> bool {
    characteristics(state, id, db).keywords.contains(&keyword)
}

/// How many cards are in `seat`'s graveyard.
fn graveyard(state: &GameState, seat: PlayerId) -> usize {
    state.players[seat.0].graveyard.len()
}

// ----- the control change itself --------------------------------------------

#[test]
fn act_of_treason_takes_the_creature_untaps_it_and_gives_it_haste() {
    // All three sentences of the card, on one creature, because one effect names one
    // target. The victim starts tapped so the untap has something to do.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == victim)
        .expect("the creature")
        .tapped = true;

    assert_eq!(controller(&state, victim), Some(PlayerId(1)));

    let after = steal(&state, &db, victim);

    assert_eq!(
        controller(&after, victim),
        Some(PlayerId(0)),
        "control moved to the caster"
    );
    assert!(!tapped(&after, victim), "and it was untapped");
    assert!(has(&after, &db, victim, Keyword::Haste));
}

#[test]
fn the_stored_controller_is_untouched_so_it_can_still_answer_for_ownership() {
    // The whole design in one assertion: layer 2 is *computed*, so the permanent's stored
    // controller keeps standing in for its owner. That is what CR 400.7 reads when the
    // creature leaves the battlefield, and it is why nothing has to be put back at
    // cleanup.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let after = steal(&state, &db, victim);

    let perm = after
        .battlefield
        .iter()
        .find(|perm| perm.id == victim)
        .expect("still on the battlefield");
    assert_eq!(
        perm.controller,
        PlayerId(1),
        "the stored field is the owner"
    );
    assert_eq!(controller(&after, victim), Some(PlayerId(0)));
}

// ----- attacking with it the same turn --------------------------------------

/// Declare `attacker` as an attacker on `defender`, from wherever the game currently is.
fn attack_with(
    state: &GameState,
    db: &CardDatabase,
    attacker: PermanentId,
    defender: PlayerId,
) -> GameState {
    let state = settle_until(state, db, |s| s.step == Step::DeclareAttackers);
    apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(defender),
            }],
        },
        db,
    )
}

#[test]
fn the_stolen_creature_attacks_its_own_player_the_turn_it_is_taken() {
    // The point of the card. Three separate rules had to move for this to work: the
    // attacker candidate set reads the layer-2 controller, the untap made it untapped,
    // and haste lifted the summoning sickness the control change itself imposed. The
    // damage then lands on the seat that owns the creature, because that seat is now the
    // one being attacked.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let after = steal(&state, &db, victim);
    assert!(
        attacker_candidates(&after, &db).contains(&victim),
        "a stolen, untapped, hasty creature is an attacker its new controller may declare"
    );

    let declared = attack_with(&after, &db, victim, PlayerId(1));
    let damaged = settle_until(&declared, &db, |s| s.step == Step::EndCombat);
    assert_eq!(
        damaged.players[1].life, 17,
        "a 3/3 they own hit them for three"
    );
    assert_eq!(damaged.players[0].life, 20);
}

#[test]
fn without_the_haste_the_stolen_creature_is_summoning_sick() {
    // CR 302.6, and the reason the card grants haste at all: a creature that has just
    // changed hands has not been under its new controller's control since their turn
    // began. Stripping only the keyword grant off the resolved state — the control change
    // and the untap stay — leaves a creature that cannot be declared.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let mut after = steal(&state, &db, victim);
    after
        .static_effects
        .retain(|effect| effect.modification != Modification::GrantKeyword(Keyword::Haste));

    assert_eq!(controller(&after, victim), Some(PlayerId(0)));
    assert!(!has(&after, &db, victim, Keyword::Haste));
    assert!(
        !attacker_candidates(&after, &db).contains(&victim),
        "the control change re-triggered summoning sickness"
    );
}

// ----- and giving it back ---------------------------------------------------

#[test]
fn control_returns_in_the_cleanup_step_and_takes_the_haste_with_it() {
    // "Until end of turn" is one duration on two modifications, so both end at the same
    // CR 514.2 turn-based action. Nothing is written back: the effect is simply gone and
    // the stored controller answers again.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let after = steal(&state, &db, victim);
    assert_eq!(controller(&after, victim), Some(PlayerId(0)));

    let next_turn = settle_until(&after, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        controller(&next_turn, victim),
        Some(PlayerId(1)),
        "the loan ended at cleanup"
    );
    assert!(!has(&next_turn, &db, victim, Keyword::Haste));
    assert!(
        next_turn.static_effects.is_empty(),
        "and nothing was left behind to prune later"
    );
}

#[test]
fn the_owner_may_attack_with_it_again_on_their_next_turn() {
    // The restamped `entered_turn` must not outlive the theft. Control returns during the
    // cleanup of turn 1, and turn 2 is seat 1's — which began after the return, so CR
    // 302.6 is satisfied and the creature is theirs to attack with.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let after = steal(&state, &db, victim);
    let their_turn = settle_until(&after, &db, |s| {
        s.turn == 2 && s.step == Step::DeclareAttackers
    });
    assert_eq!(their_turn.active_player, PlayerId(1));
    assert!(
        attacker_candidates(&their_turn, &db).contains(&victim),
        "it came home unsick"
    );
}

#[test]
fn a_creature_that_dies_while_stolen_goes_to_its_owners_graveyard() {
    // CR 400.7. The thief controls it, blocks with the owner's own creature, and kills
    // it — and the card still lands in the graveyard of the seat it came from. This is
    // the case the computed layer buys outright: the stored controller never moved, so
    // the departure seam was already right.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let blocker = place(&mut state, &db, "colossal_dreadmaw", PlayerId(1));
    let before = graveyard(&state, PlayerId(1));

    let after = steal(&state, &db, victim);
    let declared = attack_with(&after, &db, victim, PlayerId(1));
    let blocked = settle_until(&declared, &db, |s| s.step == Step::DeclareBlockers);
    let blocked = apply_action(
        &blocked,
        &Action::DeclareBlockers {
            blocks: vec![sage_engine::Block {
                blocker,
                attacker: victim,
            }],
        },
        &db,
    );
    let resolved = settle_until(&blocked, &db, |s| s.step == Step::EndCombat);

    assert!(
        controller(&resolved, victim).is_none(),
        "the 3/3 died to a 6/6"
    );
    assert_eq!(
        graveyard(&resolved, PlayerId(1)),
        before + 1,
        "it went home to its owner, not to the player who had control of it"
    );
    assert_eq!(
        graveyard(&resolved, PlayerId(0)),
        1,
        "and the only thing in the thief's graveyard is the spell they cast"
    );
}

// ----- what "creatures you control" now means -------------------------------

#[test]
fn the_stolen_creature_counts_for_its_new_controller_and_not_its_old_one() {
    // A `creatures you control` static ability is read against the layer-2 answer,
    // because layer 2 is applied first. Two anthems, one on each side, make the swap
    // visible from both directions in one board: the thief's Aggressive Mammoth starts
    // granting trample to the creature, and the owner's stops.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "aggressive_mammoth", PlayerId(0));
    place(&mut state, &db, "aggressive_mammoth", PlayerId(1));
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let bystander = place(&mut state, &db, "centaur_courser", PlayerId(1));

    assert!(
        has(&state, &db, victim, Keyword::Trample),
        "their own mammoth grants it"
    );

    let after = steal(&state, &db, victim);
    assert!(
        has(&after, &db, victim, Keyword::Trample),
        "now it is the thief's mammoth granting it"
    );
    assert!(
        has(&after, &db, bystander, Keyword::Trample),
        "the creature that stayed put is unaffected"
    );

    // The other half of the claim, on a board where only the *owner* has an anthem: the
    // grant stops the moment control moves.
    let mut lone = main_phase(&db);
    place(&mut lone, &db, "aggressive_mammoth", PlayerId(1));
    let taken = place(&mut lone, &db, "centaur_courser", PlayerId(1));
    assert!(has(&lone, &db, taken, Keyword::Trample));
    let after = steal(&lone, &db, taken);
    assert!(
        !has(&after, &db, taken, Keyword::Trample),
        "an anthem does not follow a creature out of its controller's board"
    );
}

#[test]
fn a_count_of_creatures_you_control_includes_the_one_you_took() {
    // The other `creatures you control` vocabulary — the permanent count an amount scales
    // with. Dwarven Priest gains its controller 1 life for each creature they control, so
    // the difference between stealing and not stealing is exactly one point.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let bare = cast(&state, &db, "dwarven_priest", Vec::new());
    let bare = settle_until(&bare, &db, |s| {
        s.stack.is_empty() && s.players[0].life != 20
    });
    assert_eq!(bare.players[0].life, 21, "the Priest counts only itself");

    let stolen = steal(&state, &db, victim);
    let stolen = cast(&stolen, &db, "dwarven_priest", Vec::new());
    let stolen = settle_until(&stolen, &db, |s| {
        s.stack.is_empty() && s.players[0].life != 20
    });
    assert_eq!(
        stolen.players[0].life, 22,
        "the stolen creature is one of the caster's creatures now"
    );
}

#[test]
fn the_thief_may_activate_the_stolen_creatures_abilities() {
    // Activation is offered to whoever controls the permanent, which is the layer-2
    // answer. Druid of the Cowl taps for mana; stolen with haste, its ability belongs to
    // the thief this turn and to nobody else.
    let db = db();
    let mut state = main_phase(&db);
    let druid = place(&mut state, &db, "druid_of_the_cowl", PlayerId(1));

    let ability = Action::ActivateAbility {
        permanent: druid,
        index: 0,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(
        !valid_actions(&state, &db).contains(&ability),
        "an opponent's mana creature is not the active player's to tap"
    );

    let after = steal(&state, &db, druid);
    assert!(
        valid_actions(&after, &db).contains(&ability),
        "once it is theirs, its ability is theirs"
    );
}

// ----- the targeting shape --------------------------------------------------

#[test]
fn act_of_treason_declares_exactly_one_creature_slot() {
    // Three sentences, one target group: the untap and the haste ride on the same effect
    // so a player cannot steal one creature and haste another. An announcement with the
    // slot unfilled is refused rather than resolving against nothing.
    let db = db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let instance = state.new_instance(cid(&db, "act_of_treason"));
    state.players[0].hand.push(instance);

    let requirements = sage_engine::target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    assert_eq!(requirements.len(), 1, "one slot, and only one");
    let candidates = &requirements[0].candidates;
    assert!(candidates.contains(&Target::Permanent(theirs)));
    assert!(
        candidates.contains(&Target::Permanent(mine)),
        "the card says target creature, not target creature an opponent controls"
    );

    let unaimed = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(unaimed, state, "an unaimed Act of Treason is not a cast");
}
