//! Evasion that names a **subtype**, and a permission to aim as though **hexproof** were
//! not there (issue #742).
//!
//! Two halves of what used to be one large exclusion, and neither is protection. The
//! first is an ordinary pairwise evasion stated backwards — everything the subtype does
//! not name is forbidden — so it is judged from one attacker/blocker pair and needs no
//! room in the blocker slot's prompt. The second is a permission, not a characteristic:
//! hexproof is enforced in exactly one predicate, which both the announcement gate and
//! the CR 608.2b resolution re-check run, so the permission is consulted in one place and
//! honoured in both without either learning about it.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3);
//! the restriction no bundled card prints yet is imposed the way a spell imposes one.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, blocker_can_block_attacker, target_requirements, valid_actions, Action, Attack,
    AttackTarget, Block, CardDatabase, CardId, CardInstance, Color, CombatRestriction, Duration,
    EffectAffects, FunctionalId, GameState, Modification, Permanent, PermanentId, PlayerId,
    StaticEffect, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with both pools stocked so payability
/// never decides a test that is about a restriction or a permission.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
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

/// Put a permanent of `slug` onto the battlefield under `controller`, already free of
/// summoning sickness so it can attack, block, or tap for a cost in the turn it is placed.
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
    state.players[controller.0].turn_began = state.turn;
    id
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Impose `restriction` on `id` until end of turn, the way a spell or an activated
/// ability does — the only way to put a restriction no bundled card prints yet in front
/// of the real gates.
fn impose(state: &mut GameState, id: PermanentId, restriction: CombatRestriction) {
    let source = state.mint_id();
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(id),
        modification: Modification::GrantRestriction(restriction),
        duration: Duration::UntilEndOfTurn,
    });
}

/// Take whatever the pipeline offers, preferring a pass.
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

/// Walk the pipeline until `step` is reached on turn `turn` (or the game ends).
fn advance_to_turn(state: &GameState, db: &CardDatabase, turn: u32, step: Step) -> GameState {
    let mut state = state.clone();
    for _ in 0..400 {
        if (state.turn, state.step) == (turn, step) || state.result().is_some() {
            return state;
        }
        state = advance(&state, db);
    }
    panic!("the pipeline never reached turn {turn} {step:?}");
}

/// Resolve everything on the stack, leaving priority where the pipeline puts it.
fn resolve_stack(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..40 {
        if state.stack.is_empty() {
            return state;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    panic!("the stack never emptied");
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// Whether the announcement gate would let `card` be aimed at `victim` — the candidate
/// set [`target_requirements`] enumerates for the cast, which is the same
/// `target_is_legal` predicate the resolution re-check runs.
fn aimable(state: &GameState, db: &CardDatabase, card: CardInstance, victim: PermanentId) -> bool {
    let announce = Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    target_requirements(state, db, &announce)
        .iter()
        .any(|slot| slot.candidates.contains(&Target::Permanent(victim)))
}

// ----- evasion naming a subtype ---------------------------------------------

/// The pairwise gate, read from both sides: a creature that can't be blocked except by
/// Spirits refuses a Soldier and accepts a Spirit, and the ordinary attacker beside it is
/// untouched — which is what makes this a per-pair judgment rather than a filter on the
/// candidate set.
#[test]
fn issue_742_a_subtype_evasion_admits_that_subtype_and_refuses_every_other() {
    let db = db();
    let mut state = main_phase();
    let evasive = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let ordinary = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let spirit = place(&mut state, &db, "remorseful_cleric", PlayerId(1));
    let soldier = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    // Before the imposition every pairing is legal.
    for blocker in [spirit, soldier] {
        assert!(blocker_can_block_attacker(&state, evasive, blocker, &db));
    }

    impose(
        &mut state,
        evasive,
        CombatRestriction::CantBeBlockedExceptBy("Spirit".to_string()),
    );

    assert!(
        blocker_can_block_attacker(&state, evasive, spirit, &db),
        "a Spirit is what the restriction names, so it blocks"
    );
    assert!(
        !blocker_can_block_attacker(&state, evasive, soldier, &db),
        "everything the restriction does not name is refused"
    );
    assert!(
        blocker_can_block_attacker(&state, ordinary, soldier, &db),
        "the unaffected neighbour is still blockable by anything"
    );
}

/// The same restriction at the action seam: the real declare-blockers gate rejects the
/// wrong subtype and accepts the right one, so the pairwise check is what the player
/// actually runs into rather than a predicate nothing consults.
#[test]
fn issue_742_the_declare_blockers_gate_enforces_the_subtype_evasion() {
    let db = db();
    let mut state = main_phase();
    let evasive = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let spirit = place(&mut state, &db, "remorseful_cleric", PlayerId(1));
    let soldier = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    impose(
        &mut state,
        evasive,
        CombatRestriction::CantBeBlockedExceptBy("Spirit".to_string()),
    );

    let mut state = advance_to_turn(&state, &db, 1, Step::DeclareAttackers);
    state.priority = PlayerId(0);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: evasive,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    let state = advance_to_turn(&state, &db, 1, Step::DeclareBlockers);

    let block_with = |blocker| Action::DeclareBlockers {
        blocks: vec![Block {
            blocker,
            attacker: evasive,
        }],
    };
    // The Soldier is a perfectly good blocker — it is only this pairing that is refused,
    // which is what makes the rejection below about the evasion and not about the creature.
    let candidates = sage_engine::blocker_candidates_for(&state, PlayerId(1), &db);
    assert!(candidates.contains(&soldier) && candidates.contains(&spirit));
    assert_eq!(
        apply_action(&state, &block_with(soldier), &db),
        state,
        "the declaration naming the Soldier is rejected outright (an illegal action is a no-op)"
    );

    // And the declaration naming the Spirit really happens: the 2/1 trades into the 3/3.
    let state = apply_action(&state, &block_with(spirit), &db);
    assert!(
        state.blockers_declared,
        "the declaration naming the Spirit was accepted"
    );
    let state = advance_to_turn(&state, &db, 1, Step::PostcombatMain);
    assert!(
        !on_battlefield(&state, spirit),
        "the Spirit blocked and died"
    );
    assert_eq!(
        state.players[1].life, 20,
        "the attacker was blocked, so nothing got through"
    );
}

// ----- Detection Tower ------------------------------------------------------

/// The requirement form of Detection Tower's second ability — index 1, after the mana
/// ability it prints first.
fn tower_offer(tower: PermanentId) -> Action {
    Action::ActivateAbility {
        permanent: tower,
        index: 1,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// Cast Murder at `victim` as an [`Action`], with nothing else filled in.
fn murder_at(card: CardInstance, victim: PermanentId) -> Action {
    Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: vec![Target::Permanent(victim)],
        payment: Vec::new(),
    }
}

/// The whole card in one pass: a hexproof creature an opponent controls is off limits
/// (CR 702.11b), Detection Tower's `{1}, {T}` makes it a legal aim, and the spell aimed
/// at it resolves — the CR 608.2b re-check honouring the same permission the announcement
/// gate did.
#[test]
fn issue_742_detection_tower_opens_a_hexproof_creature_to_its_controller() {
    let db = db();
    let mut state = main_phase();
    let tower = place(&mut state, &db, "detection_tower", PlayerId(0));
    let mare = place(&mut state, &db, "vine_mare", PlayerId(1));
    let murder = to_hand(&mut state, &db, "murder", PlayerId(0));

    assert!(
        !aimable(&state, &db, murder, mare),
        "a hexproof creature an opponent controls is not a legal aim (CR 702.11b)"
    );
    assert_eq!(
        apply_action(&state, &murder_at(murder, mare), &db),
        state,
        "and the announcement itself is rejected, not merely unoffered"
    );

    // Pay the {1} and tap the Tower; the permission arrives when the ability resolves.
    assert!(valid_actions(&state, &db).contains(&tower_offer(tower)));
    let state = apply_action(&state, &tower_offer(tower), &db);
    assert!(
        !aimable(&state, &db, murder, mare),
        "the ability is still on the stack, so nothing has been permitted yet"
    );
    let state = resolve_stack(&state, &db);
    assert_eq!(
        state.ignoring_hexproof.len(),
        1,
        "one permission, recorded for one seat"
    );
    assert_eq!(state.ignoring_hexproof[0].player, PlayerId(0));
    assert_eq!(state.ignoring_hexproof[0].turn, state.turn);

    // The announcement gate now offers the aim it refused a moment ago...
    let cast = murder_at(murder, mare);
    assert!(
        aimable(&state, &db, murder, mare),
        "the permission is consulted where hexproof is enforced"
    );
    let state = apply_action(&state, &cast, &db);
    assert_eq!(state.stack.len(), 1, "Murder is on the stack, aimed");

    // ...and the CR 608.2b re-check, which runs the same predicate, keeps it legal.
    let state = resolve_stack(&state, &db);
    assert!(
        !on_battlefield(&state, mare),
        "the spell resolved rather than fizzling on a re-checked hexproof target"
    );
}

/// The permission belongs to the seat that bought it. Both players control a hexproof
/// creature and both hold a Murder; only the seat that activated the Tower may aim at the
/// other's.
#[test]
fn issue_742_the_permission_is_only_the_granting_player_s() {
    let db = db();
    let mut state = main_phase();
    let tower = place(&mut state, &db, "detection_tower", PlayerId(0));
    let mine = place(&mut state, &db, "vine_mare", PlayerId(0));
    let theirs = place(&mut state, &db, "vine_mare", PlayerId(1));
    let my_murder = to_hand(&mut state, &db, "murder", PlayerId(0));
    let their_murder = to_hand(&mut state, &db, "murder", PlayerId(1));

    let state = apply_action(&state, &tower_offer(tower), &db);
    let mut state = resolve_stack(&state, &db);

    assert!(
        aimable(&state, &db, my_murder, theirs),
        "the granting player aims through hexproof"
    );

    // The other seat holds priority with the same spell and the same kind of target.
    state.priority = PlayerId(1);
    state.consecutive_passes = 0;
    assert!(
        !aimable(&state, &db, their_murder, mine),
        "the opponent bought nothing, so hexproof still stops them"
    );
    assert_eq!(
        apply_action(&state, &murder_at(their_murder, mine), &db),
        state,
        "and their announcement is rejected too"
    );

    // And the permission never made anyone's own creature more targetable than it was:
    // a controller was never stopped by their own creature's hexproof to begin with.
    assert!(
        aimable(&state, &db, their_murder, theirs),
        "a controller may always aim at their own hexproof creature (CR 702.11b)"
    );
}

/// "This turn" is a comparison of turn numbers, and the turn boundary drops the entry:
/// the same aim that was legal on the turn the Tower was activated is illegal on the next
/// one, with nothing to tick down.
#[test]
fn issue_742_the_permission_lapses_at_the_turn_boundary() {
    let db = db();
    let mut state = main_phase();
    let tower = place(&mut state, &db, "detection_tower", PlayerId(0));
    let mare = place(&mut state, &db, "vine_mare", PlayerId(1));
    let murder = to_hand(&mut state, &db, "murder", PlayerId(0));

    let state = apply_action(&state, &tower_offer(tower), &db);
    let state = resolve_stack(&state, &db);
    assert!(aimable(&state, &db, murder, mare));

    // Player 1's turn: the list is empty and the aim is refused again.
    let mut state = advance_to_turn(&state, &db, 2, Step::PrecombatMain);
    assert!(
        state.ignoring_hexproof.is_empty(),
        "the turn boundary drops every per-turn permission"
    );
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    assert!(
        !aimable(&state, &db, murder, mare),
        "the permission did not outlive the turn it was granted on"
    );
}

/// The Tower is still a land: its first ability makes mana, and the permission ability is
/// the second one rather than a replacement for it.
#[test]
fn issue_742_detection_tower_still_taps_for_colorless() {
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    let tower = place(&mut state, &db, "detection_tower", PlayerId(0));

    let mana = Action::ActivateAbility {
        permanent: tower,
        index: 0,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(valid_actions(&state, &db).contains(&mana));
    assert!(
        !valid_actions(&state, &db).contains(&tower_offer(tower)),
        "the second ability costs {{1}}, and no mana has been made yet"
    );
    let state = apply_action(&state, &mana, &db);
    assert_eq!(
        state.players[0].mana_pool.colorless, 1,
        "a mana ability uses no stack (CR 605.3a)"
    );
    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.id == tower && perm.tapped),
        "the Tower tapped for it"
    );
}
