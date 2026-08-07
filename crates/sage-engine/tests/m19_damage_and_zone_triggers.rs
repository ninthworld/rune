//! Four cards that watch an **event** the board keeps no record of (issue #706): damage
//! being dealt, cards leaving a graveyard, and a discard.
//!
//! Each one is only observable because something records it. Damage is the interesting
//! case: by the time it is marked, the recipient knows it was hit and nothing else, so
//! the *dealer* now rides the recorded event. That is engine-internal — the wire's log
//! still carries what was hit and for how much — and it is what lets a trigger tell
//! "this creature dealt damage" from "damage happened".
//!
//! Prevented damage records nothing (CR 615.1), so a shield stops these triggers with no
//! clause about it anywhere.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, Action, Attack, AttackTarget, CardDatabase, CardId,
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

/// Answer a pending card selection with the first candidate — the discard half of a
/// loot, and the discard Fell Specter's first ability asks for.
fn answer_first_card(state: &GameState, db: &CardDatabase) -> GameState {
    let Some(pending) = pending_player_choice(state) else {
        return state.clone();
    };
    let Some(request) = pending.question.cards() else {
        return state.clone();
    };
    let candidates = sage_engine::choice_candidates(state, request, db);
    let chosen = candidates.first().map(|card| card.id).into_iter().collect();
    apply_action(state, &Action::AnswerChoice { chosen }, db)
}

/// Walk an attack by `attacker` through to damage, with no blocks.
fn swing(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    #![allow(clippy::assertions_on_constants)]
    let mut state = state.clone();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    state.attackers_declared = false;
    state.blockers_declared = false;
    // A fresh combat: nothing is still attacking from the last one, and nothing is tapped
    // from having done so (CR 511.3, which the end of combat would have done).
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
        perm.attacking = None;
        perm.blocking.clear();
        perm.tapped = false;
    }
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
    let mut state = state;
    for _ in 0..20 {
        if pending_player_choice(&state).is_some() || state.step == Step::EndCombat {
            return state;
        }
        // The declaration belongs to the defending player, so the walk hands it to
        // whoever owes it rather than pressing on from one seat.
        let action = if state.step == Step::DeclareBlockers && !state.blockers_declared {
            state.priority = PlayerId(1);
            Action::DeclareBlockers { blocks: Vec::new() }
        } else {
            Action::PassPriority
        };
        let next = apply_action(&state, &action, db);
        assert_ne!(next, state, "the walk stalled at {:?}", state.step);
        state = next;
    }
    state
}

/// Surge Mare's damage to an opponent asks whether to loot.
#[test]
fn surge_mare_loots_when_it_connects() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "surge_mare", PlayerId(0));
    // A 0/5 deals nothing, so pump it into something that connects.
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: mare,
            index: 1,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let hand = state.players[0].hand.len();
    let state = swing(&state, &db, mare);

    assert!(
        pending_player_choice(&state).is_some(),
        "the loot is offered"
    );
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    // The discard half is a question of its own: which card.
    let state = answer_first_card(&state, &db);
    assert_eq!(state.players[0].hand.len(), hand, "draw one, discard one");
    assert_eq!(state.players[0].graveyard.len(), 1);
}

/// A 0/5 that deals **no** damage triggers nothing: the event is what is watched, not the
/// attack.
#[test]
fn surge_mare_that_deals_nothing_triggers_nothing() {
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "surge_mare", PlayerId(0));

    let state = swing(&state, &db, mare);

    assert!(
        pending_player_choice(&state).is_none(),
        "a 0/5 deals no damage, so nothing was dealt to watch"
    );
}

/// Rogue's Gloves watches the creature it is **attached to**, not itself.
#[test]
fn rogues_gloves_draw_when_the_equipped_creature_connects() {
    let db = db();
    let mut state = main_phase(&db);
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let gloves = place(&mut state, &db, "rogue_s_gloves", PlayerId(0));
    let bare = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == gloves) {
        perm.attached_to = Some(ogre);
    }
    let hand = state.players[0].hand.len();

    // The equipped creature connects.
    let state = swing(&state, &db, ogre);
    assert!(
        pending_player_choice(&state).is_some(),
        "the draw is offered"
    );
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    assert_eq!(state.players[0].hand.len(), hand + 1);

    // The one that is not equipped connects for the same amount, and nothing is offered.
    let state = swing(&state, &db, bare);
    assert!(
        pending_player_choice(&state).is_none(),
        "the Gloves watch one creature, and it is the one they are on"
    );
}

/// **The word "combat" is load-bearing.** The Gloves say `combat damage`, so damage the
/// equipped creature deals any other way is not what they are watching.
///
/// Nothing about the damage *afterwards* can tell the two apart — marked damage is marked
/// damage, and a player who lost two life lost two life — so the recorded event carries
/// which it was, exactly as it carries who dealt it.
#[test]
fn issue_706_rogues_gloves_ignore_noncombat_damage_from_the_equipped_creature() {
    let db = db();
    let mut state = main_phase(&db);
    let snipe = place(&mut state, &db, "guttersnipe", PlayerId(0));
    let gloves = place(&mut state, &db, "rogue_s_gloves", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == gloves) {
        perm.attached_to = Some(snipe);
    }
    let shock = to_hand(&mut state, &db, "shock", PlayerId(0));
    let life = state.players[1].life;

    // Casting an instant makes the equipped creature deal two damage to the opponent —
    // its own damage, dealt to a player, and not in combat.
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
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

    assert_eq!(
        state.players[1].life,
        life - 4,
        "two from the Guttersnipe and two from the spell"
    );
    assert!(
        pending_player_choice(&state).is_none(),
        "the Gloves watch combat damage, and none was dealt"
    );
}

/// A creature card leaving your graveyard makes a Bat — once, however many left.
#[test]
fn desecrated_tomb_makes_one_bat_however_many_cards_left() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "desecrated_tomb", PlayerId(0));
    // Two creature cards in the graveyard, and a Gravedigger to fetch one of them.
    for _ in 0..2 {
        let card = state.new_instance(cid(&db, "onakke_ogre"));
        state.players[0].graveyard.push(card);
    }
    let digger = to_hand(&mut state, &db, "gravedigger", PlayerId(0));
    let tokens = |state: &GameState| {
        state
            .battlefield
            .iter()
            .filter(|perm| perm.printed.is_token())
            .count()
    };
    let before = tokens(&state);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: digger,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    // Its enters trigger returns a creature card from the graveyard — that is the card
    // leaving, and what the Tomb is watching.
    let ability = sage_engine::pending_trigger_target_choice(&state).expect("owed a target");
    let card = state.players[0].graveyard[0];
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Card(card.id)],
        },
        &db,
    );
    // Gravedigger's return is a `you may`: accepting is what takes the card out of the
    // graveyard, which is the event the Tomb watches.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(tokens(&state), before + 1, "one Bat");
}

/// Fell Specter's second ability acts on **that player** — the one the event named, with
/// nobody asked to choose.
#[test]
fn fell_specter_drains_the_player_who_discarded() {
    let db = db();
    let mut state = main_phase(&db);
    let specter = to_hand(&mut state, &db, "fell_specter", PlayerId(0));
    let card = state.new_instance(cid(&db, "onakke_ogre"));
    state.players[1].hand.push(card);
    let life = state.players[1].life;
    let mine = state.players[0].life;

    // The Specter's own first ability makes the discard its second one watches, which is
    // the card working against itself and the cleanest way to produce a real discard.
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: specter,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let ability = sage_engine::pending_trigger_target_choice(&state).expect("owed an opponent");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Player(PlayerId(1))],
        },
        &db,
    );
    let mut state = apply_action(&state, &Action::PassPriority, &db);
    // The discard resolves — asking seat 1 which card — and then the drain it triggers.
    for _ in 0..10 {
        if state.stack.is_empty() && state.players[1].life != life {
            break;
        }
        state = if pending_player_choice(&state).is_some() {
            answer_first_card(&state, &db)
        } else {
            apply_action(&state, &Action::PassPriority, &db)
        };
    }

    assert_eq!(state.players[1].life, life - 2, "that player lost 2 life");
    assert_eq!(state.players[0].life, mine, "and nobody else did");
    assert_eq!(state.players[1].hand.len(), 0, "and they really discarded");
}
