//! Three cards that watch what a player *does* rather than what the board is
//! (issue #706): an Aura aimed at this creature, a creature spell inside a power band,
//! and an activation of one particular planeswalker.
//!
//! Each is one narrowing on an observation the engine already made, and each narrowing
//! is a phrase a printed card uses:
//!
//! - Druid of Horns — `whenever **you** cast an **Aura** spell that targets this
//!   creature`. Both halves ride the stack-diff of CR 603.6e: it *is* a becoming-the-
//!   target, restricted to a class of spell and to one caster.
//! - Sarkhan's Unsealing — `power 4, 5, or 6` on one ability and `power 7 or greater` on
//!   the next, which is the same class with an upper bound and without one.
//! - Sarkhan's Whelp — `an ability of a **Sarkhan** planeswalker`, which is the source
//!   subtype the activation watcher could not name.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_trigger_target_choice, Action, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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

fn tokens(state: &GameState) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.is_token())
        .count()
}

/// An Aura you cast at the Druid answers with a Beast.
#[test]
fn druid_of_horns_answers_an_aura_you_aimed_at_it() {
    let db = db();
    let mut state = main_phase(&db);
    let druid = place(&mut state, &db, "druid_of_horns", PlayerId(0));
    let aura = to_hand(&mut state, &db, "knight_s_pledge", PlayerId(0));
    let before = tokens(&state);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: aura,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(druid)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(tokens(&state), before + 1, "a 3/3 Beast");
}

/// A **non-Aura** spell aimed at it is not one, however much it targets.
#[test]
fn druid_of_horns_ignores_a_spell_that_is_not_an_aura() {
    let db = db();
    let mut state = main_phase(&db);
    let druid = place(&mut state, &db, "druid_of_horns", PlayerId(0));
    let bolt = to_hand(&mut state, &db, "shock", PlayerId(0));
    let before = tokens(&state);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: bolt,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(druid)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(tokens(&state), before, "a Shock is not an Aura");
}

/// `Whenever **you** cast`: an opponent's Aura on your Druid does nothing.
#[test]
fn druid_of_horns_ignores_an_opponents_aura() {
    let db = db();
    let mut state = main_phase(&db);
    let druid = place(&mut state, &db, "druid_of_horns", PlayerId(0));
    let aura = to_hand(&mut state, &db, "knight_s_pledge", PlayerId(1));
    let before = tokens(&state);
    state.priority = PlayerId(1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: aura,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(druid)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(tokens(&state), before, "the card says *you* cast");
}

/// Cast a creature and see whether the Unsealing's first ability fires.
fn cast_creature(db: &CardDatabase, slug: &str) -> GameState {
    let mut state = main_phase(db);
    place(&mut state, db, "sarkhan_s_unsealing", PlayerId(0));
    let creature = to_hand(&mut state, db, slug, PlayerId(0));
    apply_action(
        &state,
        &Action::CastSpell {
            card: creature,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    )
}

/// A 4/2 is inside `power 4, 5, or 6`, so the first ability triggers and owes a target.
#[test]
fn sarkhans_unsealing_fires_inside_the_power_band() {
    let db = db();
    let state = cast_creature(&db, "onakke_ogre");

    assert!(
        pending_trigger_target_choice(&state).is_some(),
        "a 4/2 is in the band, and the trigger owes a target"
    );
}

/// A 6/5 is inside it too — the band's upper end, and the card that proves the bound is
/// inclusive.
#[test]
fn sarkhans_unsealing_fires_at_the_top_of_the_band() {
    let db = db();
    let state = cast_creature(&db, "bogstomper");

    assert!(
        pending_trigger_target_choice(&state).is_some(),
        "a 6/5 is the top of `4, 5, or 6`"
    );
}

/// A 3/1 is below it and fires nothing: the lower bound is a bound, not a suggestion.
#[test]
fn sarkhans_unsealing_ignores_a_small_creature() {
    let db = db();
    let state = cast_creature(&db, "oreskos_swiftclaw");

    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "a 3/1 is under the band"
    );
    assert!(state.stack.len() == 1, "only the creature spell is there");
}

/// A 10/10 is over the band, so the *second* ability fires instead — and its damage
/// names no target at all, which is how the two are told apart from outside.
#[test]
fn sarkhans_unsealing_sweeps_for_a_creature_over_the_band() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "sarkhan_s_unsealing", PlayerId(0));
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let giant = to_hand(&mut state, &db, "gigantosaurus", PlayerId(0));
    let life = state.players[1].life;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: giant,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "the sweep aims at nothing"
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert_eq!(state.players[1].life, life - 4, "four to the opponent");
    assert!(
        !state.battlefield.iter().any(|perm| perm.id == victim),
        "and four to their creature, which a 4/2 does not survive"
    );
}

/// The Whelp watches one **player** as well as one walker: `whenever **you** activate an
/// ability of a Sarkhan planeswalker`.
///
/// It was authored to watch every player's activation, because the scope vocabulary had
/// only "any" and "opponents" and no way to say "you" (issue #823) — so an opponent's own
/// Sarkhan fired it. The comparison is the same walker on each side of the table, one
/// activation each, which is what separates "the scope is enforced" from "nothing fired
/// for some other reason".
#[test]
fn issue_823_the_whelp_watches_its_own_controllers_sarkhan_and_not_an_opponents() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "sarkhan_s_whelp", PlayerId(0));
    let mine = place(&mut state, &db, "sarkhan_fireblood", PlayerId(0));
    let theirs = place(&mut state, &db, "sarkhan_fireblood", PlayerId(1));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
        perm.counters.insert(sage_engine::CounterKind::Loyalty, 5);
    }

    // Their Sarkhan, activated by them: a Sarkhan planeswalker's ability, and not one
    // this Whelp's controller activated.
    let mut opponents_turn = state.clone();
    opponents_turn.active_player = PlayerId(1);
    opponents_turn.priority = PlayerId(1);
    let after_theirs = apply_action(
        &opponents_turn,
        &Action::ActivateAbility {
            permanent: theirs,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        pending_trigger_target_choice(&after_theirs).is_none(),
        "the card says `whenever you activate`, and that was not you"
    );

    // The same activation, on this side of the table: the Whelp owes a target.
    let after_mine = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: mine,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        pending_trigger_target_choice(&after_mine).is_some(),
        "and this one was"
    );
}

/// The Whelp watches one walker by name: an ability of a **Sarkhan** planeswalker.
#[test]
fn sarkhans_whelp_watches_a_sarkhan_and_not_another_walker() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "sarkhan_s_whelp", PlayerId(0));
    let vivien = place(&mut state, &db, "vivien_reid", PlayerId(0));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
        perm.counters.insert(sage_engine::CounterKind::Loyalty, 5);
    }

    // Vivien is a planeswalker, and not a Sarkhan.
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: vivien,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );

    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "the Whelp is not watching Vivien"
    );
}
