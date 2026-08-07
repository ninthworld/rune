//! What a **copy of a spell** inherits from the original (CR 707.10): not only the
//! copiable values of the card, but the *choices made for it* — the mode it announced and
//! the X it named.
//!
//! This is the seam between two things that were built apart — the spell copy of issue
//! #734 and the announcement-time choices of issue #733 — and it is a seam with no
//! forgiving failure mode. A copy that carried no mode is a Cleansing Nova that destroys
//! nothing; a copy that carried no X is a Banefire that deals zero and is answerable by a
//! counterspell the original was immune to. Neither shows up as an error; both show up as
//! a spell that quietly did less than it says.
//!
//! Every test therefore watches the **copy on its own**. The delayed ability Doublecast
//! leaves behind resolves above the spell it watched, so the copy resolves while the
//! original is still on the stack — which is the one vantage point from which "the copy
//! resolved mode 0" and "the copy resolved nothing" are different observations.
//! `copy_effects.rs` covers what a copy *is*; this covers what it was told.
//!
//! Everything drives the real [`apply_action`]. Cards are named by their authored
//! `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_trigger_target_choice, Action, CardDatabase, CardId, CardInstance, Color,
    DamageFilter, FunctionalId, GameEvent, GameState, Permanent, PermanentId, PlayerId, StackId,
    StackObjectKind, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability never
/// decides a test that is about a choice.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 20);
        }
        player.mana_pool.add_colorless(20);
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

/// Both players pass, resolving the top of the stack.
fn resolve_top(state: &GameState, db: &CardDatabase) -> GameState {
    let state = apply_action(state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Announce `slug` from `seat` with the mode, X, and targets its controller chose, and
/// leave it on the stack.
fn announce(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
    mode: Option<u8>,
    x: Option<u32>,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let card = to_hand(&mut state, db, slug, seat);
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card,
            mode,
            x,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    assert_ne!(after, state, "{slug} was refused at apply");
    after
}

/// Resolve a Doublecast so its delayed ability is armed (CR 603.7), leaving the board and
/// the pools as they were.
///
/// The whole stack is emptied rather than one object popped: Doublecast is itself an
/// instant-or-sorcery cast, so a watcher on the battlefield triggers off it, and anything
/// the test below counts has to be counted after that has settled.
fn armed(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = announce(state, db, "doublecast", PlayerId(0), None, None, Vec::new());
    for _ in 0..8 {
        if state.stack.is_empty() {
            break;
        }
        state = resolve_top(&state, db);
    }
    assert!(state.stack.is_empty(), "the Doublecast settled");
    assert_eq!(state.delayed_triggers.len(), 1, "one ability is waiting");
    state
}

/// Announce `slug` under an armed Doublecast, then resolve the delayed ability that
/// watched it — so what comes back has the **copy** on the stack, above the original.
fn copy_of(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    mode: Option<u8>,
    x: Option<u32>,
    targets: Vec<Target>,
) -> GameState {
    let state = announce(&armed(state, db), db, slug, PlayerId(0), mode, x, targets);
    assert_eq!(
        state.stack.len(),
        2,
        "the spell, and the delayed ability that watched it being cast"
    );
    let state = resolve_top(&state, db);
    assert!(
        state
            .stack
            .iter()
            .any(|o| matches!(o.kind, StackObjectKind::SpellCopy { .. })),
        "the ability resolved into a copy"
    );
    state
}

/// The copy on the stack — its id and the choices it carries.
fn the_copy(state: &GameState) -> (StackId, Option<u8>, Option<u32>) {
    state
        .stack
        .iter()
        .find_map(|o| match o.kind {
            StackObjectKind::SpellCopy { mode, x, .. } => Some((o.id, mode, x)),
            _ => None,
        })
        .expect("a copy is on the stack")
}

/// Aim the copy at `target`, answering the CR 707.10c question with the same action a
/// triggered ability's targets ride.
fn aim_copy(state: &GameState, db: &CardDatabase, target: Target) -> GameState {
    let ability = pending_trigger_target_choice(state).expect("the copy owes targets");
    apply_action(
        state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![target],
        },
        db,
    )
}

fn life(state: &GameState, seat: PlayerId) -> i32 {
    state.players[seat.0].life
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

// ----- the mode the original chose (CR 707.10) ------------------------------

/// **The crux for a modal spell.** Cleansing Nova's two modes sweep two disjoint classes,
/// so a copy that resolved the chosen mode, a copy that resolved the *other* one, and a
/// copy that resolved no mode at all are three visibly different boards.
///
/// The copy is watched alone — the original is still on the stack underneath it — because
/// that is the only vantage from which "the copy did the same thing" is distinguishable
/// from "the copy did nothing". Both modes are driven, so neither answer can be the one
/// the code happens to default to.
fn nova_copy_resolves(
    db: &CardDatabase,
    mode: u8,
) -> (GameState, PermanentId, PermanentId, PermanentId) {
    let mut base = main_phase();
    let creature = place(&mut base, db, "onakke_ogre", PlayerId(1));
    let artifact = place(&mut base, db, "manalith", PlayerId(1));
    let enchantment = place(&mut base, db, "ajani_s_welcome", PlayerId(1));

    let state = copy_of(&base, db, "cleansing_nova", Some(mode), None, Vec::new());
    let (_, carried, x) = the_copy(&state);
    assert_eq!(carried, Some(mode), "the copy carries the announced mode");
    assert_eq!(x, None, "a fixed cost announced no X to carry");

    // Resolve the copy only. The original Cleansing Nova is still on the stack, so
    // everything below is the copy's own doing.
    let state = resolve_top(&state, db);
    assert!(
        state
            .stack
            .iter()
            .any(|o| matches!(o.kind, StackObjectKind::Spell { .. })),
        "the original has not resolved yet"
    );
    (state, creature, artifact, enchantment)
}

#[test]
fn cr_707_10_a_copy_resolves_the_first_mode_the_original_chose() {
    let db = db();
    let (state, creature, artifact, enchantment) = nova_copy_resolves(&db, 0);

    assert!(
        !on_battlefield(&state, creature),
        "the copy swept the creatures, which is the mode the original announced"
    );
    assert!(
        on_battlefield(&state, artifact) && on_battlefield(&state, enchantment),
        "and the mode nobody chose never applied to the copy either"
    );
}

#[test]
fn cr_707_10_a_copy_resolves_the_second_mode_the_original_chose() {
    let db = db();
    let (state, creature, artifact, enchantment) = nova_copy_resolves(&db, 1);

    assert!(
        !on_battlefield(&state, artifact) && !on_battlefield(&state, enchantment),
        "the copy swept both classes its mode names"
    );
    assert!(
        on_battlefield(&state, creature),
        "and left every creature standing, as the announced mode says"
    );
}

// ----- the X the original announced -----------------------------------------

/// **The crux for an X spell.** A copy that lost the announced value would deal zero and
/// look for all the world like a spell that resolved. The copy is again watched alone:
/// three life off before the original has resolved at all.
#[test]
fn cr_707_10_a_copy_deals_the_x_the_original_announced() {
    let db = db();
    let state = copy_of(
        &main_phase(),
        &db,
        "banefire",
        None,
        Some(3),
        vec![Target::Player(PlayerId(1))],
    );
    let (_, mode, x) = the_copy(&state);
    assert_eq!(x, Some(3), "the announced value rides the copy");
    assert_eq!(mode, None, "a non-modal spell announced no mode to carry");

    let state = aim_copy(&state, &db, Target::Player(PlayerId(1)));
    let after_copy = resolve_top(&state, &db);
    assert_eq!(
        life(&after_copy, PlayerId(1)),
        17,
        "the copy dealt three on its own, before the original resolved"
    );

    let after_both = resolve_top(&after_copy, &db);
    assert!(after_both.stack.is_empty());
    assert_eq!(
        life(&after_both, PlayerId(1)),
        14,
        "and the original's three"
    );
}

/// CR 707.10c: the copy may be aimed somewhere new, and choosing a new **target** is not
/// choosing a new **X** — the number was settled at announcement and travels untouched.
#[test]
fn cr_707_10c_a_re_aimed_copy_keeps_the_announced_x() {
    let db = db();
    let state = copy_of(
        &main_phase(),
        &db,
        "banefire",
        None,
        Some(3),
        vec![Target::Player(PlayerId(1))],
    );
    let state = aim_copy(&state, &db, Target::Player(PlayerId(0)));
    let (_, _, x) = the_copy(&state);
    assert_eq!(x, Some(3), "answering the target question changed no X");

    let after = resolve_top(&resolve_top(&state, &db), &db);
    assert!(after.stack.is_empty());
    assert_eq!(
        life(&after, PlayerId(0)),
        17,
        "the copy hit its new target for the announced three"
    );
    assert_eq!(
        life(&after, PlayerId(1)),
        17,
        "and the original hit its own"
    );
}

// ----- what the announced X makes true of the copy (CR 707.10, CR 701.5a) ---

/// Seat 1 answers the **copy** with a Cancel, and everything resolves. The counterspell
/// is aimed at the copy rather than at the original, which is a legal aim either way: a
/// copy of a spell is a spell on the stack (CR 707.10).
fn copy_into_a_counterspell(db: &CardDatabase, x: u32) -> GameState {
    let state = copy_of(
        &main_phase(),
        db,
        "banefire",
        None,
        Some(x),
        vec![Target::Player(PlayerId(1))],
    );
    let state = aim_copy(&state, db, Target::Player(PlayerId(1)));
    let (copy, _, _) = the_copy(&state);

    let state = apply_action(&state, &Action::PassPriority, db);
    assert_eq!(state.priority, PlayerId(1));
    let state = announce(
        &state,
        db,
        "cancel",
        PlayerId(1),
        None,
        None,
        vec![Target::Spell(copy)],
    );
    assert_eq!(state.stack.len(), 3, "Cancel, the copy, and the original");

    let mut state = resolve_top(&state, db);
    for _ in 0..4 {
        if state.stack.is_empty() {
            break;
        }
        state = resolve_top(&state, db);
    }
    assert!(state.stack.is_empty(), "everything settled");
    state
}

/// **Below the threshold the copy is an ordinary spell.** Cancel removes it, and only the
/// original's four lands — which is what makes the test above it able to fail.
#[test]
fn cr_707_10_a_copy_below_the_threshold_can_be_countered() {
    let db = db();
    let after = copy_into_a_counterspell(&db, 4);
    assert_eq!(
        life(&after, PlayerId(1)),
        16,
        "the copy was countered; the original's four is all that landed"
    );
}

/// **At the threshold the copy can't be countered either** (CR 701.5a). The clause is
/// part of what the copy inherited — the card's text, measured against the X the
/// *original* announced — so Cancel resolves against it and fails, exactly as it fails
/// against the original.
#[test]
fn cr_707_10_a_copy_at_the_threshold_cannot_be_countered() {
    let db = db();
    let after = copy_into_a_counterspell(&db, 5);
    assert_eq!(
        life(&after, PlayerId(1)),
        10,
        "both the copy's five and the original's five landed"
    );
    assert!(
        after.players[1]
            .graveyard
            .iter()
            .any(|card| card.card == cid(&db, "cancel")),
        "Cancel still resolved; it just did nothing"
    );
}

/// The copy resolves under a blanket prevention shield. The shield is raised as the value
/// Root Snare's resolution produces minus the combat filter, because Root Snare's own
/// filter would let a sorcery's damage through whatever X it named — the wrong question
/// (`prevented_damage_is_not_dealt.rs` asks the right one of it).
fn copy_into_a_shield(db: &CardDatabase, x: u32) -> GameState {
    let mut base = main_phase();
    base.prevention.push(DamageFilter { combat_only: false });
    let state = copy_of(
        &base,
        db,
        "banefire",
        None,
        Some(x),
        vec![Target::Player(PlayerId(1))],
    );
    let state = aim_copy(&state, db, Target::Player(PlayerId(1)));
    resolve_top(&resolve_top(&state, db), db)
}

/// Below the threshold the shield holds against the copy as it holds against the
/// original: neither point of either lands.
#[test]
fn cr_615_1_a_copy_below_the_threshold_has_its_damage_prevented() {
    let db = db();
    let after = copy_into_a_shield(&db, 4);
    assert_eq!(
        life(&after, PlayerId(1)),
        20,
        "the shield prevented all of it"
    );
}

/// **At the threshold the copy's damage can't be prevented either.** The declaration is
/// read off the copied card against the announced X (CR 615.1), so the copy is as
/// unstoppable as the spell it came from — the whole point of carrying the number.
#[test]
fn cr_615_1_a_copy_at_the_threshold_cannot_have_its_damage_prevented() {
    let db = db();
    let after = copy_into_a_shield(&db, 5);
    assert_eq!(
        life(&after, PlayerId(1)),
        10,
        "five from the copy and five from the original, through the shield"
    );
    assert!(
        after.prevention.iter().any(|shield| !shield.combat_only),
        "and the shield is still standing for the next thing"
    );
}

/// A **combat-only** shield was never going to stop either of them: Root Snare's filter
/// is read at the same seam for the copy as for the original, and a sorcery's damage is
/// not combat damage whatever X it named.
#[test]
fn cr_615_1_a_combat_only_shield_stops_neither_the_copy_nor_the_original() {
    let db = db();
    let mut base = main_phase();
    base.prevention.push(DamageFilter { combat_only: true });
    let state = copy_of(
        &base,
        &db,
        "banefire",
        None,
        Some(2),
        vec![Target::Player(PlayerId(1))],
    );
    let state = aim_copy(&state, &db, Target::Player(PlayerId(1)));
    let after = resolve_top(&resolve_top(&state, &db), &db);
    assert_eq!(life(&after, PlayerId(1)), 16, "two and two, unprevented");
}

// ----- and it was still never cast (CR 707.10) ------------------------------

/// Carrying the original's choices does not make the copy a *cast*. Guttersnipe is the
/// witness `copy_effects.rs` uses, and it is the right one here too: it fires for the
/// Doublecast and for the Banefire, and not at all for the copy that dealt the same X.
///
/// The arithmetic is the assertion. Guttersnipe's two for the Banefire, the Banefire's
/// three, and the copy's three is eight; a copy that counted as a cast would be ten, and
/// a copy that lost its X would be five.
#[test]
fn cr_707_10_a_copy_carrying_a_mode_and_an_x_was_still_never_cast() {
    let db = db();
    let mut base = main_phase();
    place(&mut base, &db, "guttersnipe", PlayerId(0));

    // Arm the Doublecast first and let its own Guttersnipe trigger resolve, so the count
    // below measures only the Banefire and its copy.
    let state = armed(&base, &db);
    let opening = life(&state, PlayerId(1));

    let state = announce(
        &state,
        &db,
        "banefire",
        PlayerId(0),
        None,
        Some(3),
        vec![Target::Player(PlayerId(1))],
    );
    let mut state = state;
    for _ in 0..8 {
        if state.stack.is_empty() {
            break;
        }
        state = if pending_trigger_target_choice(&state).is_some() {
            aim_copy(&state, &db, Target::Player(PlayerId(1)))
        } else {
            resolve_top(&state, &db)
        };
    }
    assert!(state.stack.is_empty(), "everything settled");

    assert_eq!(
        opening - life(&state, PlayerId(1)),
        8,
        "Guttersnipe's two, the Banefire's three, and the copy's three"
    );

    let casts = state
        .log
        .iter()
        .filter(|entry| {
            matches!(&entry.event, GameEvent::SpellCast { card, .. }
                if card.card == cid(&db, "banefire"))
        })
        .count();
    assert_eq!(
        casts, 1,
        "one cast, however much the copy inherited from it"
    );

    let copies = state.players[0]
        .graveyard
        .iter()
        .filter(|card| card.card == cid(&db, "banefire"))
        .count();
    assert_eq!(copies, 1, "and one card to reach a graveyard (CR 707.10a)");
}
