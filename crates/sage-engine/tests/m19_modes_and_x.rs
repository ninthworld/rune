//! Announcement-time choices (CR 601.2b): the **mode** a modal spell chooses and the
//! **X** it names, and the two M19 cards that need them — Cleansing Nova (#9) and
//! Banefire (#130).
//!
//! Both choices are made before targets and before payment, and both ride the action.
//! What these tests are really about is the ordering, because it is the part that cannot
//! be bolted on afterwards:
//!
//! - **A mode decides which target slots exist**, so `target_requirements` cannot answer
//!   for a modal cast until the mode is chosen — and an announcement that skipped the
//!   question is refused rather than read as a spell with no targets.
//! - **X is announced, then locked**: one number is folded into the cost, charged,
//!   recorded on the stack object, and read by the resolving effect. Nothing re-derives
//!   it, so nothing can disagree about it.
//!
//! Every choice is also re-validated *independently* at apply: a forged mode and an X
//! the pool cannot pay are both refused there, not merely left unoffered.
//!
//! Every test drives the **real** [`apply_action`]. Cards are named by their authored
//! `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, mode_options, target_requirements, valid_actions, x_options, Action,
    CardDatabase, CardId, CardInstance, Color, DamageFilter, FunctionalId, GameState, Permanent,
    PermanentId, PlayerId, StackId, StackObjectKind, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main. Pools are stocked so payability never
/// decides a test that is about something else; the X tests stock their own.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for color in Color::ALL {
        for seat in 0..2 {
            state.players[seat].mana_pool.add(color, 10);
        }
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

/// The requirement form of a cast — what `valid_actions` advertises, and the shape a
/// player fills in.
fn announce(card: CardInstance) -> Action {
    cast(card, None, None, Vec::new())
}

/// A filled-in announcement: the card, the mode and X its controller chose, and the
/// targets that follow from the mode. The payment is left to the engine's own solver —
/// these tests stock a pool and are about the choices, not about which land pays which
/// pip.
fn cast(card: CardInstance, mode: Option<u8>, x: Option<u32>, targets: Vec<Target>) -> Action {
    Action::CastSpell {
        card,
        mode,
        x,
        targets,
        payment: Vec::new(),
    }
}

/// Both players pass, resolving the top of the stack.
fn resolve_top(state: &GameState, db: &CardDatabase) -> GameState {
    let state = apply_action(state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// A synthetic two-mode spell whose modes differ in what they target: the first destroys
/// target creature, the second draws a card and targets nothing.
///
/// Authored here rather than taken from the catalog because M19's two modal cards both
/// have modes with the same (empty) target shape, and the rule under test is precisely
/// that the *slots* follow the mode. A definition assembled in a test goes through the
/// same loader and the same validators a catalog file does.
fn two_shapes() -> CardDatabase {
    CardDatabase::from_json(
        r#"[{"schema_version": 1, "functional_id": "test_two_shapes",
             "name": "Test Two Shapes", "types": ["sorcery"], "mana_cost": "{1}{B}",
             "colors": ["black"],
             "modes": [
               {"effects": [{"kind": "destroy", "target": "any_creature"}]},
               {"effects": [{"kind": "draw_card", "count": 1}]}
             ]}]"#,
    )
    .expect("a well-formed definition")
}

// ----- the mode decides the slots -------------------------------------------

/// **The crux of this issue.** A modal spell's target slots are the *chosen mode's*, so
/// the question "what does this spell target" has no answer until the mode is picked —
/// and the engine says so by answering with no slots at all rather than by guessing.
///
/// The three readings in one test: unchosen names nothing, mode 0 names a creature slot,
/// mode 1 names none. Nothing anywhere returns the union.
#[test]
fn issue_733_target_requirements_follow_the_chosen_mode() {
    let db = two_shapes();
    let mut state = main_phase();
    let spell = to_hand(&mut state, &db, "test_two_shapes", PlayerId(0));

    let unchosen = announce(spell);
    assert!(
        target_requirements(&state, &db, &unchosen).is_empty(),
        "with no mode chosen there is nothing to ask about"
    );

    let mode_one = cast(spell, Some(0), None, Vec::new());
    let slots = target_requirements(&state, &db, &mode_one);
    assert_eq!(slots.len(), 1, "the destroy mode names one creature");
    assert!(!slots[0].optional);

    let mode_two = cast(spell, Some(1), None, Vec::new());
    assert!(
        target_requirements(&state, &db, &mode_two).is_empty(),
        "the draw mode targets nothing"
    );
}

/// The other half of the same rule, on the real card: **an unchosen mode's effects never
/// apply**. Cleansing Nova's first mode destroys all creatures and its second destroys
/// all artifacts and enchantments; choosing one leaves the other class untouched.
#[test]
fn issue_733_an_unchosen_mode_never_applies() {
    let db = db();
    let mut base = main_phase();
    let creature = place(&mut base, &db, "onakke_ogre", PlayerId(1));
    let artifact = place(&mut base, &db, "manalith", PlayerId(1));
    let enchantment = place(&mut base, &db, "ajani_s_welcome", PlayerId(1));
    let nova = to_hand(&mut base, &db, "cleansing_nova", PlayerId(0));

    let sweep_creatures = apply_action(&base, &cast(nova, Some(0), None, Vec::new()), &db);
    let sweep_creatures = resolve_top(&sweep_creatures, &db);
    assert!(
        !on_battlefield(&sweep_creatures, creature),
        "the chosen mode destroyed the creatures"
    );
    assert!(
        on_battlefield(&sweep_creatures, artifact) && on_battlefield(&sweep_creatures, enchantment),
        "the mode that was not chosen never happened"
    );

    let sweep_artifacts = apply_action(&base, &cast(nova, Some(1), None, Vec::new()), &db);
    let sweep_artifacts = resolve_top(&sweep_artifacts, &db);
    assert!(
        !on_battlefield(&sweep_artifacts, artifact)
            && !on_battlefield(&sweep_artifacts, enchantment),
        "the chosen mode destroyed both classes it names"
    );
    assert!(
        on_battlefield(&sweep_artifacts, creature),
        "and left every creature standing"
    );
}

/// A modal cast is advertised **once**, in its requirement form, with the modes
/// enumerated beside it — the shape ADR 0004 gives a targeted action, applied to the
/// choice that comes before targets.
#[test]
fn issue_733_a_modal_cast_is_offered_once_with_its_modes_listed() {
    let db = db();
    let mut state = main_phase();
    let nova = to_hand(&mut state, &db, "cleansing_nova", PlayerId(0));

    let offered = valid_actions(&state, &db);
    let casts: Vec<_> = offered
        .iter()
        .filter(|action| matches!(action, Action::CastSpell { card, .. } if card.id == nova.id))
        .collect();
    assert_eq!(casts.len(), 1, "one offer, not one per mode");
    assert_eq!(casts[0], &announce(nova), "advertised with no mode chosen");

    let modes = mode_options(&state, &db, &announce(nova));
    assert_eq!(modes.len(), 2);
    assert_eq!(modes[0].index, 0);
    assert_eq!(modes[1].index, 1);
    assert!(
        x_options(&state, &db, &announce(nova)).is_empty(),
        "a fixed cost announces no X"
    );
}

/// A mode index the card does not print is refused **at apply**, by a check that
/// re-derives the card's own mode list rather than trusting the action.
#[test]
fn issue_733_a_forged_mode_is_rejected_at_apply() {
    let db = db();
    let mut state = main_phase();
    let creature = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let nova = to_hand(&mut state, &db, "cleansing_nova", PlayerId(0));

    for forged in [2u8, 7, 255] {
        let after = apply_action(&state, &cast(nova, Some(forged), None, Vec::new()), &db);
        assert_eq!(after, state, "mode {forged} is not a mode this card prints");
    }

    // And so is the announcement that skipped the question — the case that would
    // otherwise slip through as "a cast with no target slots".
    let unchosen = apply_action(&state, &announce(nova), &db);
    assert_eq!(unchosen, state, "a modal spell must choose a mode");
    assert!(on_battlefield(&state, creature));
}

// ----- X is announced, then locked ------------------------------------------

/// The values of X are enumerated by the engine **with what each one costs**, so nobody
/// above it ever multiplies a cost out. Banefire is `{X}{R}`, so X = n costs `{n}{R}`,
/// and the list stops where the board stops being able to pay.
#[test]
fn issue_733_x_values_are_enumerated_with_their_costs() {
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.players[0].mana_pool.add(Color::Red, 4);
    let banefire = to_hand(&mut state, &db, "banefire", PlayerId(0));

    let values = x_options(&state, &db, &announce(banefire));
    assert_eq!(
        values.iter().map(|option| option.value).collect::<Vec<_>>(),
        vec![0, 1, 2, 3],
        "four red mana pays {{R}} plus three more"
    );
    assert_eq!(values[0].cost, "{R}");
    assert_eq!(values[3].cost, "{3}{R}");
    for option in &values {
        assert!(
            !option.cost.contains('X'),
            "a cost with an X left in it would make the reader multiply"
        );
    }
    assert!(
        mode_options(&state, &db, &announce(banefire)).is_empty(),
        "a non-modal spell offers no modes"
    );
}

/// **X is announced, then locked.** One number is charged, recorded, and resolved: the
/// pool pays for exactly the value announced, the stack object carries it, and the
/// damage dealt is it.
#[test]
fn issue_733_the_announced_x_drives_payment_and_resolution() {
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.players[0].mana_pool.add(Color::Red, 6);
    let banefire = to_hand(&mut state, &db, "banefire", PlayerId(0));

    let announced = apply_action(
        &state,
        &cast(banefire, None, Some(5), vec![Target::Player(PlayerId(1))]),
        &db,
    );

    assert_eq!(
        announced.players[0].mana_pool.total(),
        0,
        "{{5}}{{R}} took all six"
    );
    match announced
        .stack
        .last()
        .expect("Banefire is on the stack")
        .kind
    {
        StackObjectKind::Spell { x, mode, .. } => {
            assert_eq!(x, Some(5), "the announced value rides the object");
            assert_eq!(mode, None);
        }
        StackObjectKind::Ability { .. } | StackObjectKind::SpellCopy { .. } => {
            panic!("a spell, not an ability or a copy")
        }
    }

    let resolved = resolve_top(&announced, &db);
    assert_eq!(
        resolved.players[1].life, 15,
        "the resolution read the same five"
    );
}

/// An X the pool cannot pay is refused **at apply**, not merely left off the offer.
///
/// The cast itself is on offer the whole time — X = 0 is affordable — so the base
/// legality check passes and this is a second, independent answer about the value the
/// player actually named. That separation is the point: an offer is computed before the
/// player has chosen anything.
#[test]
fn issue_733_an_unpayable_x_is_rejected_at_apply() {
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.players[0].mana_pool.add(Color::Red, 3);
    let banefire = to_hand(&mut state, &db, "banefire", PlayerId(0));

    assert!(
        valid_actions(&state, &db).contains(&announce(banefire)),
        "the cast is on offer — X = 0 is affordable"
    );
    let offered: Vec<u32> = x_options(&state, &db, &announce(banefire))
        .into_iter()
        .map(|option| option.value)
        .collect();
    assert_eq!(offered, vec![0, 1, 2], "and only three values are");

    let after = apply_action(
        &state,
        &cast(banefire, None, Some(9), vec![Target::Player(PlayerId(1))]),
        &db,
    );
    assert_eq!(after, state, "nine is not payable out of three mana");
    assert_eq!(
        after.players[0].mana_pool.total(),
        3,
        "and nothing was spent"
    );

    // The other direction: a spell with no `{X}` may not announce one.
    let mut with_shock = state.clone();
    let shock = to_hand(&mut with_shock, &db, "shock", PlayerId(0));
    let forged = apply_action(
        &with_shock,
        &cast(shock, None, Some(2), vec![Target::Player(PlayerId(1))]),
        &db,
    );
    assert_eq!(forged, with_shock, "Shock's cost prints no X to announce");
}

// ----- the clause above the threshold ---------------------------------------

/// Cast Banefire at `x` targeting player 1, then answer it with Cancel, and let both
/// resolve. Returns the state after everything has settled.
fn banefire_into_a_counterspell(db: &CardDatabase, x: u32) -> GameState {
    let mut state = main_phase();
    let banefire = to_hand(&mut state, db, "banefire", PlayerId(0));
    let cancel = to_hand(&mut state, db, "cancel", PlayerId(1));

    let state = apply_action(
        &state,
        &cast(banefire, None, Some(x), vec![Target::Player(PlayerId(1))]),
        db,
    );
    let aimed_at: StackId = state.stack.last().expect("Banefire on the stack").id;

    // Player 0 passes; player 1 answers with Cancel aimed at the Banefire.
    let state = apply_action(&state, &Action::PassPriority, db);
    assert_eq!(state.priority, PlayerId(1));
    let state = apply_action(
        &state,
        &cast(cancel, None, None, vec![Target::Spell(aimed_at)]),
        db,
    );
    assert_eq!(state.stack.len(), 2, "Cancel went on the stack");

    // Cancel resolves, then whatever is left of the Banefire.
    let state = resolve_top(&state, db);
    if state.stack.is_empty() {
        state
    } else {
        resolve_top(&state, db)
    }
}

/// **Below its threshold Banefire is an ordinary spell.** X = 4 is counterable, so
/// Cancel removes it and nothing is dealt.
#[test]
fn issue_733_banefire_below_the_threshold_can_be_countered() {
    let db = db();
    let after = banefire_into_a_counterspell(&db, 4);
    assert!(after.stack.is_empty());
    assert_eq!(after.players[1].life, 20, "countered, so no damage");
    assert!(
        after.players[0]
            .graveyard
            .iter()
            .any(|card| card.card == cid(&db, "banefire")),
        "a countered spell goes to the graveyard"
    );
}

/// **At its threshold it cannot be countered** (CR 701.5a). Cancel is a perfectly legal
/// aim — "can't be countered" is not hexproof and changes nothing about targeting — it
/// simply resolves and fails to remove the spell, which then resolves for its five.
#[test]
fn issue_733_banefire_at_the_threshold_cannot_be_countered() {
    let db = db();
    let after = banefire_into_a_counterspell(&db, 5);
    assert!(after.stack.is_empty());
    assert_eq!(after.players[1].life, 15, "the five landed anyway");
    assert!(
        after.players[1]
            .graveyard
            .iter()
            .any(|card| card.card == cid(&db, "cancel")),
        "Cancel still resolved; it just did nothing"
    );
}

/// Cast Banefire at `x` into a blanket prevention shield, and let it resolve.
fn banefire_into_a_shield(db: &CardDatabase, x: u32) -> GameState {
    let mut state = main_phase();
    // A shield over every damage event this turn — the blanket form, unfiltered, so
    // there is no question of a burn spell slipping past a combat-only clause.
    state.prevention.push(DamageFilter { combat_only: false });
    let banefire = to_hand(&mut state, db, "banefire", PlayerId(0));
    let announced = apply_action(
        &state,
        &cast(banefire, None, Some(x), vec![Target::Player(PlayerId(1))]),
        db,
    );
    resolve_top(&announced, db)
}

/// Below the threshold the shield holds: X = 4 is prevented in full.
#[test]
fn issue_733_banefire_below_the_threshold_can_be_prevented() {
    let db = db();
    let after = banefire_into_a_shield(&db, 4);
    assert_eq!(after.players[1].life, 20, "the shield prevented all of it");
}

/// **At the threshold the damage can't be prevented** (CR 615.1) — the clause read
/// against the shield the same turn raised. It is not that the shield is gone; it is
/// that this damage is not the kind any shield may apply to.
#[test]
fn issue_733_banefire_at_the_threshold_cannot_be_prevented() {
    let db = db();
    let after = banefire_into_a_shield(&db, 5);
    assert_eq!(after.players[1].life, 15, "five landed through the shield");
    assert!(
        after.prevention.iter().any(|shield| !shield.combat_only),
        "and the shield is still standing for the next thing"
    );
}

/// The threshold is measured against the value **this cast announced**, not against
/// anything re-derived — so one card is two spells depending on the number its
/// controller named, and the boundary is exactly where the card prints it.
#[test]
fn issue_733_the_threshold_reads_the_announced_value() {
    let db = db();
    assert_eq!(banefire_into_a_shield(&db, 4).players[1].life, 20);
    assert_eq!(banefire_into_a_shield(&db, 5).players[1].life, 15);
    assert_eq!(banefire_into_a_shield(&db, 6).players[1].life, 14);
}
