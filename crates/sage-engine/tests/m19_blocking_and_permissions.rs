//! Four cards whose rules are about **when you may**, rather than about what happens
//! (issue #706): an Aura that punishes a block, an ability that may only be activated
//! while a particular permanent is on the battlefield, a class of card named as one
//! choice, and a creature whose spell cannot be countered.
//!
//! The two interesting seams are restrictions rather than effects. `Activate only if you
//! control a Vivien planeswalker` is a restriction on **announcing** (CR 602.5c), so it is
//! asked where the offer is made and again where the action is checked, and never at
//! resolution — an ability already on the stack resolves whatever the board has become.
//! `When enchanted creature blocks` watches the declaration itself (CR 509.1), once per
//! declaration however many attackers were blocked.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, Attack, AttackTarget, Block, CardDatabase, CardId,
    CardInstance, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
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

/// Put `slug` into seat `seat`'s graveyard and hand back the instance.
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

/// Whether the priority holder is currently offered the graveyard activation of `card`.
fn graveyard_activation_offered(state: &GameState, db: &CardDatabase, card: CardInstance) -> bool {
    valid_actions(state, db).iter().any(|action| {
        matches!(
            action,
            Action::ActivateAbilityFromGraveyard { card: named, .. } if *named == card
        )
    })
}

// ----- Dwindle: the block is the event -----------------------------------------

/// **The crux.** The Aura watches its *host's* block declaration and destroys it — the
/// attacking creature stays blocked, which is what the reminder text says and what the
/// combat state already does on its own.
#[test]
fn issue_706_dwindle_destroys_the_creature_that_blocks() {
    let db = db();
    let mut state = main_phase(&db);
    let attacker = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let blocker = place(&mut state, &db, "bogstomper", PlayerId(1));
    let aura = place(&mut state, &db, "dwindle", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == aura) {
        perm.attached_to = Some(blocker);
    }

    let state = swing_into(&state, &db, attacker, Some(blocker));

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == blocker),
        "the blocker was destroyed"
    );
}

/// A host that never blocks is never destroyed: the trigger is about the declaration, not
/// about the Aura sitting there.
#[test]
fn issue_706_dwindle_leaves_a_creature_that_stays_home() {
    let db = db();
    let mut state = main_phase(&db);
    let attacker = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let bystander = place(&mut state, &db, "bogstomper", PlayerId(1));
    let aura = place(&mut state, &db, "dwindle", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == aura) {
        perm.attached_to = Some(bystander);
    }

    let state = swing_into(&state, &db, attacker, None);

    assert!(
        state.battlefield.iter().any(|perm| perm.id == bystander),
        "it did not block, so nothing triggered"
    );
}

/// Walk an attack by `attacker` through the blocker declaration, with `blocker` blocking
/// it if one is given, and stop once the trigger has resolved.
fn swing_into(
    state: &GameState,
    db: &CardDatabase,
    attacker: PermanentId,
    blocker: Option<PermanentId>,
) -> GameState {
    let mut state = state.clone();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    state.attackers_declared = false;
    state.blockers_declared = false;
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
        perm.attacking = None;
        perm.blocking.clear();
        perm.tapped = false;
    }
    let mut state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    for _ in 0..20 {
        if state.step == Step::CombatDamage || state.step == Step::EndCombat {
            return state;
        }
        let action = if state.step == Step::DeclareBlockers && !state.blockers_declared {
            state.priority = PlayerId(1);
            Action::DeclareBlockers {
                blocks: blocker
                    .map(|blocker| vec![Block { blocker, attacker }])
                    .unwrap_or_default(),
            }
        } else {
            Action::PassPriority
        };
        let next = apply_action(&state, &action, db);
        assert_ne!(next, state, "the walk stalled at {:?}", state.step);
        state = next;
    }
    state
}

// ----- Vivien's Jaguar: a permission that can go away --------------------------

/// **The second crux.** The graveyard activation is offered only while its controller has
/// the planeswalker the card names (CR 602.5c).
#[test]
fn issue_706_the_jaguar_needs_a_vivien_to_come_back() {
    let db = db();
    let mut state = main_phase(&db);
    let jaguar = to_graveyard(&mut state, &db, "vivien_s_jaguar", PlayerId(0));

    assert!(
        !graveyard_activation_offered(&state, &db, jaguar),
        "no Vivien, no offer"
    );

    let mut with_vivien = state.clone();
    place(&mut with_vivien, &db, "vivien_reid", PlayerId(0));
    assert!(
        graveyard_activation_offered(&with_vivien, &db, jaguar),
        "with one, the ability is offered"
    );
}

/// And it is *re-derived* rather than trusted: an action assembled while the planeswalker
/// was there is refused once it has gone.
#[test]
fn issue_706_a_stale_jaguar_activation_is_refused() {
    let db = db();
    let mut state = main_phase(&db);
    let jaguar = to_graveyard(&mut state, &db, "vivien_s_jaguar", PlayerId(0));
    let vivien = place(&mut state, &db, "vivien_reid", PlayerId(0));
    let action = Action::ActivateAbilityFromGraveyard {
        card: jaguar,
        index: 0,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert_ne!(
        apply_action(&state, &action, &db),
        state,
        "the offer stands while the Vivien is there"
    );

    // The planeswalker leaves; the same action is now a permission nobody has.
    let mut without = state.clone();
    without.battlefield.retain(|perm| perm.id != vivien);
    assert_eq!(
        apply_action(&without, &action, &db),
        without,
        "the activation is refused, not applied"
    );
}

// ----- Two cards the vocabulary could already say ------------------------------

/// Lightning Mare needs no new machinery at all — an uncounterable spell, a colour-based
/// blocking restriction, and a pump — and is here so the claim is tested rather than
/// asserted.
#[test]
fn issue_706_the_mare_cannot_be_blocked_by_blue_creatures() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "lightning_mare", PlayerId(0));
    let blue = place(&mut state, &db, "frilled_sea_serpent", PlayerId(1));
    let green = place(&mut state, &db, "bogstomper", PlayerId(1));
    let attacking = declared_attack(&state, &db, mare);

    let block = |blocker: PermanentId| Action::DeclareBlockers {
        blocks: vec![Block {
            blocker,
            attacker: mare,
        }],
    };
    // An illegal declaration is refused outright: the state comes back untouched.
    assert_eq!(
        apply_action(&attacking, &block(blue), &db),
        attacking,
        "a blue creature cannot block it"
    );
    assert_ne!(
        apply_action(&attacking, &block(green), &db),
        attacking,
        "and one that is not blue may"
    );
}

/// The Mare's spell cannot be countered — the trait rides the stack object, so a counter
/// aimed at it resolves and does nothing (CR 701.5).
#[test]
fn issue_706_the_mare_cannot_be_countered() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = state.new_instance(cid(&db, "lightning_mare"));
    state.players[0].hand.push(mare);
    let cancel = state.new_instance(cid(&db, "cancel"));
    state.players[1].hand.push(cancel);

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
    let spell = state.stack.last().expect("the Mare is on the stack").id;
    let mut state = state;
    state.priority = PlayerId(1);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: cancel,
            mode: None,
            x: None,
            targets: vec![sage_engine::Target::Spell(spell)],
            payment: Vec::new(),
        },
        &db,
    );
    let mut state = state;
    for _ in 0..8 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.printed.card() == Some(cid(&db, "lightning_mare"))),
        "the counterspell resolved and the Mare still arrived"
    );
}

/// Declare `attacker` as an attacker and hand back the state with blocks still owed.
fn declared_attack(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let mut state = state.clone();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    state.attackers_declared = false;
    state.blockers_declared = false;
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }
    let mut state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    for _ in 0..6 {
        if state.step == Step::DeclareBlockers {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state.priority = PlayerId(1);
    state
}

/// Tezzeret's Gatebreaker names `a blue or artifact card` as **one** class, so a look at
/// the top five offers both kinds in the same question.
#[test]
fn issue_706_the_gatebreaker_takes_a_blue_or_an_artifact() {
    let db = db();
    let mut state = main_phase(&db);
    // A library whose **top** — the end of the vector, which is where a draw comes from —
    // holds one artifact, one blue card, and one of neither.
    let mut library: Vec<CardInstance> = (0..8)
        .map(|_| state.new_instance(cid(&db, "bogstomper")))
        .collect();
    for slug in ["bogstomper", "frilled_sea_serpent", "gargoyle_sentinel"] {
        let card = state.new_instance(cid(&db, slug));
        library.push(card);
    }
    state.players[0].library = library;
    let gatebreaker = state.new_instance(cid(&db, "tezzeret_s_gatebreaker"));
    state.players[0].hand.push(gatebreaker);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: gatebreaker,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    // Two passes resolve the artifact, two more resolve the enters trigger it put on the
    // stack, and the look is what that trigger asks.
    let mut state = state;
    for _ in 0..6 {
        if sage_engine::pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    let pending = sage_engine::pending_player_choice(&state).expect("the look asks");
    let request = pending.question.cards().expect("a card selection");
    let candidates = sage_engine::choice_candidates(&state, request, &db);
    let names: Vec<&str> = candidates
        .iter()
        .filter_map(|card| db.card(card.card).map(|data| data.name.as_str()))
        .collect();
    assert!(
        names.contains(&"Gargoyle Sentinel") && names.contains(&"Frilled Sea Serpent"),
        "both halves of the class are offered, got {names:?}"
    );
    assert!(
        !names.contains(&"Bogstomper"),
        "and a card that is neither is not"
    );
}
