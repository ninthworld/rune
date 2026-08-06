//! Behaviour of triggers that fire at the beginning of a step (issue #607, CR 603.6a).
//!
//! Every test here drives the **real** [`apply_action`] pipeline across whole turns
//! rather than crafting a before/after pair, because the properties that matter are
//! properties of the walk: a step trigger must fire exactly once per crossing however
//! many steps a single pass of priority moves through, and must survive the room's
//! auto-pass settle rather than being skipped past (ADR 0010).
//!
//! No M19 card carries a bare step trigger with nothing attached to it — every printed
//! one also wants an intervening-if clause, a sacrifice cost, or a counted amount — so
//! these definitions are authored inline (ADR 0009). The life amounts are distinct
//! powers of two so a single life total says exactly which abilities fired and how
//! often.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, forced_declaration_without_choice, priority_has_no_meaningful_action,
    valid_actions, Action, CardDatabase, CardId, FunctionalId, GameState, Permanent, PermanentId,
    PlayerId, Step,
};

/// Enough steps to walk several whole turns; a settle that has not reached its goal by
/// then is a hang, and failing beats spinning.
const STEP_CAP: usize = 400;

/// An inline catalog of permanents whose only ability is a step trigger, one per
/// scope/step pairing the vocabulary can express.
fn step_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_your_upkeep","name":"Test Your Upkeep",
         "types":["enchantment"],"mana_cost":"{1}{W}","colors":["white"],
         "abilities":[{"type":"triggered",
            "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"yours"}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
        {"schema_version":1,"functional_id":"test_each_upkeep","name":"Test Each Upkeep",
         "types":["artifact"],"mana_cost":"{1}",
         "abilities":[{"type":"triggered",
            "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"each"}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]}]},
        {"schema_version":1,"functional_id":"test_each_end_step","name":"Test Each End Step",
         "types":["enchantment"],"mana_cost":"{2}{B}","colors":["black"],
         "abilities":[{"type":"triggered",
            "event":{"beginning_of_step":{"step":"end_step","whose_turn":"each"}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]},
        {"schema_version":1,"functional_id":"test_your_combat","name":"Test Your Combat",
         "types":["enchantment"],"mana_cost":"{3}{W}","colors":["white"],
         "abilities":[{"type":"triggered",
            "event":{"beginning_of_step":{"step":"begin_combat","whose_turn":"yours"}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":8}]}]},
        {"schema_version":1,"functional_id":"test_filler","name":"Test Filler",
         "types":["creature"],"subtypes":["Bird"],"mana_cost":"{1}{U}","colors":["blue"],
         "power":1,"toughness":1}
    ]"#;
    CardDatabase::from_json(json).expect("an inline catalog")
}

/// The interned handle for an authored identity.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a catalog card")
}

/// A two-player game at the very start of turn 1, with both libraries stocked so the
/// walk never trips the CR 704.5c decking loss and both hands empty so cleanup never
/// asks for a discard.
fn fresh_game(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let library: Vec<_> = (0..6)
            .map(|_| state.new_instance(cid(db, "test_filler")))
            .collect();
        state.players[seat].library = library;
    }
    state
}

/// Put a permanent of `slug` on the battlefield under `controller` and return its id.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card);
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: instance.id,
        printed: card.into(),
        controller,
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
    });
    id
}

/// The room's settle policy, replayed exactly (`room::input::auto_action_for`): a seat
/// with no meaningful action passes, and a seat owing a combat declaration with no
/// legal non-empty answer submits the empty one. Returns `None` the moment a real
/// decision is owed — which is what makes this a *test* of the automation and not just
/// a way to move the game along.
fn auto_action(state: &GameState, db: &CardDatabase) -> Option<Action> {
    if priority_has_no_meaningful_action(state, db) {
        return Some(Action::PassPriority);
    }
    forced_declaration_without_choice(state, db)
}

/// Settle the game forward under [`auto_action`] until `done` holds, and panic if it
/// stalls on a decision or runs past [`STEP_CAP`].
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..STEP_CAP {
        if done(&state) {
            return state;
        }
        let action = auto_action(&state, db).unwrap_or_else(|| {
            panic!("the settle stalled at turn {} {:?}", state.turn, state.step)
        });
        let next = apply_action(&state, &action, db);
        assert_ne!(next, state, "the settle made no progress");
        state = next;
    }
    panic!("the settle ran past its cap without reaching the goal");
}

#[test]
fn issue_607_a_step_trigger_fires_once_per_crossing_across_a_full_game_walk() {
    // The headline property, over the real turn structure: "your upkeep" fires on its
    // controller's turns only, "each upkeep" fires on every turn, and each fires
    // exactly once per crossing. Life is the ledger — +1 and +2 on seat 0's upkeeps,
    // +2 alone on seat 1's.
    let db = step_db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "test_your_upkeep", PlayerId(0));
    place(&mut state, &db, "test_each_upkeep", PlayerId(0));
    let start = state.players[0].life;

    // Turn 1 is seat 0's: both abilities trigger and resolve. Reaching the precombat
    // main means the upkeep is behind us and the stack has emptied.
    let state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life,
        start + 3,
        "seat 0's own upkeep fires both abilities, once each"
    );

    // Turn 2 is seat 1's: only the each-upkeep ability fires, and seat 0 still gains
    // the life because it controls the ability.
    let state = settle_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life,
        start + 5,
        "an opponent's upkeep fires the each-scope ability alone"
    );
    assert_eq!(
        state.players[1].life, start,
        "the opponent gains nothing; the ability's controller does"
    );

    // Turn 3 is seat 0's again: both fire once more. Three upkeeps, five firings, no
    // double-count from the several priority windows each step contains.
    let state = settle_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    assert_eq!(state.players[0].life, start + 8);
}

#[test]
fn issue_607_end_step_and_begin_combat_triggers_fire_at_their_own_boundaries() {
    // The other two steps in the vocabulary, and the property that a trigger fires at
    // its boundary and not before it. Begin-combat is "yours"-scoped and end-step is
    // "each"-scoped, so one turn of seat 0 produces one of each.
    let db = step_db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "test_your_combat", PlayerId(0));
    place(&mut state, &db, "test_each_end_step", PlayerId(0));
    let start = state.players[0].life;

    // Nothing has fired by the precombat main: combat has not begun and the turn has
    // not ended.
    let state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life, start,
        "neither boundary is crossed yet"
    );

    // The declare-attackers step is past begin-combat, so the combat trigger has fired
    // and resolved by the time the walk gets there.
    let state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    assert_eq!(
        state.players[0].life,
        start + 8,
        "the begin-combat trigger fired once, at the beginning of combat"
    );

    // Through the end step and into seat 1's turn: the end-step trigger fires once.
    let state = settle_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life,
        start + 8 + 4,
        "seat 0's end step fired the end-step ability once"
    );

    // Seat 1's own combat crosses the same begin-combat boundary, but the ability is
    // "yours"-scoped and this is not its controller's turn: it stays silent.
    let state = settle_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::DeclareAttackers
    });
    assert_eq!(
        state.players[0].life,
        start + 8 + 4,
        "a begin-combat trigger scoped to your turn does not fire on an opponent's"
    );

    // Seat 1's end step, though, is an "each" boundary and fires again — and seat 0's
    // own combat comes round on turn 3.
    let state = settle_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life,
        start + 8 + 4 + 4,
        "seat 1's end step fired the each-scope ability; turn 3's combat is still ahead"
    );
    let state = settle_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::DeclareAttackers
    });
    assert_eq!(
        state.players[0].life,
        start + 8 + 4 + 4 + 8,
        "the begin-combat trigger fires again on its controller's next turn"
    );
}

#[test]
fn issue_607_auto_pass_does_not_skip_a_step_at_which_a_trigger_is_owed() {
    // ADR 0010's safety property for this vocabulary. The dangerous shape is a seat
    // with literally nothing to do at its upkeep: the idle predicate says "pass", the
    // room passes for it, and if a pass could walk the turn structure onward with the
    // trigger still on the stack, the ability would silently never happen — the same
    // class of silent-nothing bug #602 fixed for targets.
    let db = step_db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "test_your_upkeep", PlayerId(0));

    // Walk to the upkeep. The trigger is on the stack and the step has not moved on.
    let state = settle_until(&state, &db, |s| !s.stack.is_empty());
    assert_eq!(state.step, Step::Upkeep);
    assert_eq!(
        state.stack.len(),
        1,
        "the upkeep trigger is waiting to resolve"
    );

    // Both seats are idle here — the room *will* auto-pass them.
    assert!(
        priority_has_no_meaningful_action(&state, &db),
        "with nothing to respond with, the seat is idle and will be auto-passed"
    );

    // A full round of those auto-passes resolves the trigger instead of advancing the
    // step: the stack empties, the life is gained, and the game is still in upkeep.
    let one = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(one.step, Step::Upkeep);
    let two = apply_action(&one, &Action::PassPriority, &db);
    assert!(two.stack.is_empty(), "the trigger resolved");
    assert_eq!(
        two.step,
        Step::Upkeep,
        "the pass resolved the stack rather than leaving the step behind"
    );
    assert_eq!(two.players[0].life, state.players[0].life + 1);

    // And the whole settle, run end to end, reaches the next step with the life gained
    // — the room never skipped past the ability.
    let settled = settle_until(&state, &db, |s| s.step == Step::Draw);
    assert_eq!(settled.players[0].life, state.players[0].life + 1);
}

#[test]
fn issue_607_a_step_trigger_is_offered_as_an_ordinary_stack_object() {
    // A step trigger is not a special case anywhere downstream: it reaches the stack
    // as an ordinary triggered ability, so the seats holding priority over it are
    // offered the ordinary pass and nothing else is owed.
    let db = step_db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "test_your_upkeep", PlayerId(0));
    let state = settle_until(&state, &db, |s| !s.stack.is_empty());
    let actions = valid_actions(&state, &db);
    assert!(actions.contains(&Action::PassPriority));
    assert!(
        sage_engine::pending_trigger_target_choice(&state).is_none(),
        "a life-gain trigger declares no target slots, so nothing is owed"
    );
}
