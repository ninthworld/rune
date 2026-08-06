//! Two more places a count of permanents may feed a number (issue #722): the **count of
//! tokens** an effect creates, and an **attachment's** static power/toughness grant.
//!
//! The vocabulary is the same `PermanentCount` a pump, a life gain, and a damage effect
//! already scale with. What differs is *when* each is read, and that difference is the
//! point of this file:
//!
//! - A token count is a one-shot effect, so CR 608.2 takes X **once, on resolution** and
//!   the tokens then simply exist. A creature that arrives while the ability is on the
//!   stack is counted; one that dies is not; and nothing afterwards adds a token or takes
//!   one back.
//! - An attachment's grant is a **static ability** (CR 604.3) whose continuous effect
//!   lasts exactly as long as the attachment is attached, so its value is recalculated on
//!   every read: a Forest played after the Aura resolved makes the Aura bigger.
//!
//! Every test drives the real [`apply_action`] pipeline. The Aura is a bundled card
//! (Blanchwood Armor); the token count has no bundled card to drive it — its M19
//! representative, Lena, Selfless Champion, is still blocked elsewhere — so it is driven
//! from inline definitions (ADR 0009).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, valid_actions, Action, CardDatabase, CardId, CardInstance,
    Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, StackObjectKind, Step,
    Target,
};

// ----- fixtures -------------------------------------------------------------

fn bundled() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability never
/// decides a test that is about an effect.
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

/// Put a permanent of `slug` onto the battlefield under `controller`.
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

/// Cast `slug` from player 0's hand with `targets` and let it resolve. Goes through the
/// ordinary cast gate, so a spell `valid_actions` would not offer fails here rather than
/// silently doing nothing.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let action = Action::CastSpell {
        card: instance,
        mode: None,
        x: None,
        targets,
        payment: Vec::new(),
    };
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
    let state = apply_action(&state, &action, db);
    assert!(
        state.stack.iter().any(|o| matches!(
            o.kind,
            StackObjectKind::Spell { card, .. } if card.id == instance.id
        )),
        "{slug} did not reach the stack — the cast was rejected"
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Resolve everything on the stack.
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

/// The current power/toughness of `perm`, read through the layer system.
fn pt(state: &GameState, perm: PermanentId, db: &CardDatabase) -> (Option<i32>, Option<i32>) {
    let current = characteristics(state, perm, db);
    (current.power, current.toughness)
}

/// How many token permanents are on the battlefield.
fn token_count(state: &GameState) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.is_token())
        .count()
}

// ----- an Aura whose grant is counted ---------------------------------------

/// Blanchwood Armor grants its host `+1/+1` per Forest its controller controls, and the
/// number is **not** frozen when the Aura resolves: a Forest played afterwards makes the
/// grant bigger, and a Forest that leaves makes it smaller.
///
/// This is the whole difference between a static ability's continuous effect (CR 604.3,
/// recalculated on every read) and a one-shot `pump_by_count`, whose X CR 608.2 takes
/// once and never revisits.
#[test]
fn issue_722_blanchwood_armor_is_recounted_on_every_read() {
    let db = bundled();
    let mut state = main_phase();
    let host = place(&mut state, &db, "centaur_courser", PlayerId(0));
    place(&mut state, &db, "forest", PlayerId(0));
    place(&mut state, &db, "forest", PlayerId(0));
    assert_eq!(pt(&state, host, &db), (Some(3), Some(3)), "a 3/3 to start");

    let state = cast(
        &state,
        &db,
        "blanchwood_armor",
        vec![Target::Permanent(host)],
    );
    assert_eq!(
        pt(&state, host, &db),
        (Some(5), Some(5)),
        "+1/+1 for each of two Forests"
    );

    // A third Forest arrives after the Aura resolved. A fixed modifier would ignore it.
    let mut grown = state.clone();
    place(&mut grown, &db, "forest", PlayerId(0));
    assert_eq!(
        pt(&grown, host, &db),
        (Some(6), Some(6)),
        "the grant grows with the count, because it is a static ability"
    );

    // And it shrinks again the moment a Forest leaves.
    let forest = grown
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "forest")))
        .expect("a Forest is on the battlefield")
        .id;
    let mut shrunk = grown.clone();
    shrunk.battlefield.retain(|perm| perm.id != forest);
    assert_eq!(pt(&shrunk, host, &db), (Some(5), Some(5)));
}

/// "Forest **you control**" is the Aura's controller's Forests. An opponent's swamp-free
/// forest does not feed someone else's Aura, and a controller with none grants `+0/+0`
/// rather than failing to attach.
#[test]
fn issue_722_a_counted_grant_counts_only_its_own_controller_s_permanents() {
    let db = bundled();
    let mut state = main_phase();
    let host = place(&mut state, &db, "centaur_courser", PlayerId(0));
    place(&mut state, &db, "forest", PlayerId(1));
    place(&mut state, &db, "forest", PlayerId(1));

    let state = cast(
        &state,
        &db,
        "blanchwood_armor",
        vec![Target::Permanent(host)],
    );
    assert_eq!(
        pt(&state, host, &db),
        (Some(3), Some(3)),
        "a count of zero is a legal Aura granting nothing"
    );

    let mut mine = state.clone();
    place(&mut mine, &db, "forest", PlayerId(0));
    assert_eq!(pt(&mine, host, &db), (Some(4), Some(4)));
}

/// The counted grant ends with the attachment, exactly as a flat one does: destroying the
/// Aura returns the host to its printed size with nothing left to unwind (ADR 0005 — the
/// contribution was derived, never stored).
#[test]
fn issue_722_a_counted_grant_ends_when_the_aura_leaves() {
    let db = bundled();
    let mut state = main_phase();
    let host = place(&mut state, &db, "centaur_courser", PlayerId(0));
    place(&mut state, &db, "forest", PlayerId(0));
    place(&mut state, &db, "forest", PlayerId(0));

    let state = cast(
        &state,
        &db,
        "blanchwood_armor",
        vec![Target::Permanent(host)],
    );
    assert_eq!(pt(&state, host, &db), (Some(5), Some(5)));

    let aura = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "blanchwood_armor")))
        .expect("the Aura is on the battlefield")
        .id;
    let freed = cast(&state, &db, "naturalize", vec![Target::Permanent(aura)]);
    assert!(
        !freed.battlefield.iter().any(|perm| perm.id == aura),
        "the Aura was destroyed"
    );
    assert_eq!(pt(&freed, host, &db), (Some(3), Some(3)));
}

// ----- a token count taken from a count of permanents -----------------------

/// One 1/1 Soldier token per creature its controller controls, and two per creature for
/// the multiplier case. Inline (ADR 0009): the M19 card that prints this shape, Lena,
/// Selfless Champion, needs two families this change does not add — a mass-effect class
/// comparing power against another permanent's, and last-known information for a source
/// sacrificed to pay the cost — so no bundled card can drive it yet.
const COUNTED_TOKEN_MAKERS: &str = r#"[
    {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
     "types":["creature"],"subtypes":["Bear"],"mana_cost":"{1}{G}","colors":["green"],
     "power":2,"toughness":2},
    {"schema_version":1,"functional_id":"test_marshal","name":"Test Marshal",
     "types":["creature"],"subtypes":["Human"],"mana_cost":"{1}{W}","colors":["white"],
     "power":1,"toughness":1,
     "abilities":[{"type":"triggered","event":"self_enters_battlefield","effects":[
       {"kind":"create_token","count_of":{"scope":"you_control","card_type":"creature"},
        "token":{"name":"Soldier","types":["creature"],"subtypes":["Soldier"],
                 "colors":["white"],"power":1,"toughness":1}}]}]},
    {"schema_version":1,"functional_id":"test_captain","name":"Test Captain",
     "types":["creature"],"subtypes":["Human"],"mana_cost":"{1}{W}","colors":["white"],
     "power":1,"toughness":1,
     "abilities":[{"type":"triggered","event":"self_enters_battlefield","effects":[
       {"kind":"create_token","count":2,
        "count_of":{"scope":"you_control","card_type":"creature","subtype":"Bear"},
        "token":{"name":"Soldier","types":["creature"],"subtypes":["Soldier"],
                 "colors":["white"],"power":1,"toughness":1}}]}]}
]"#;

/// X is taken **once, on resolution** (CR 608.2), from the board as it stands then: a
/// creature that arrived while the trigger was on the stack is counted, and the tokens the
/// effect is about to create are not — the count is read before the first one arrives.
#[test]
fn issue_722_a_token_count_is_taken_on_resolution() {
    let db = CardDatabase::from_json(COUNTED_TOKEN_MAKERS).expect("inline definitions");
    let mut state = main_phase();
    place(&mut state, &db, "test_bear", PlayerId(0));
    place(&mut state, &db, "test_bear", PlayerId(0));
    place(&mut state, &db, "test_bear", PlayerId(1));

    // Casting the Marshal resolves the creature spell and leaves its trigger on the stack.
    let mut state = cast(&state, &db, "test_marshal", Vec::new());
    assert_eq!(state.stack.len(), 1, "the entry trigger is waiting");
    assert_eq!(token_count(&state), 0, "nothing has been created yet");

    // A fourth creature arrives while the trigger waits. Announcement-time counting would
    // miss it; resolution-time counting does not.
    place(&mut state, &db, "test_bear", PlayerId(0));

    let state = resolve_stack(&state, &db);
    assert_eq!(
        token_count(&state),
        4,
        "three Bears plus the Marshal itself — the opponent's Bear is not counted, and \
         no Soldier counted a Soldier"
    );
}

/// A creature that dies before the trigger resolves is not counted, and one that dies
/// **afterwards** does not take a token back: the number stopped being a rule the instant
/// it was read.
#[test]
fn issue_722_the_token_count_is_fixed_once_the_effect_has_resolved() {
    let db = CardDatabase::from_json(COUNTED_TOKEN_MAKERS).expect("inline definitions");
    let mut state = main_phase();
    let doomed = place(&mut state, &db, "test_bear", PlayerId(0));
    place(&mut state, &db, "test_bear", PlayerId(0));

    let mut state = cast(&state, &db, "test_marshal", Vec::new());
    assert_eq!(state.stack.len(), 1);
    // One of the two Bears leaves while the trigger is on the stack.
    state.battlefield.retain(|perm| perm.id != doomed);

    let state = resolve_stack(&state, &db);
    assert_eq!(
        token_count(&state),
        2,
        "one Bear plus the Marshal — the Bear that left was not counted"
    );

    // Everything else leaving afterwards changes nothing: the tokens are objects now.
    let mut later = state.clone();
    later.battlefield.retain(|perm| {
        perm.printed.is_token() || perm.printed.card() != Some(cid(&db, "test_bear"))
    });
    assert_eq!(
        token_count(&later),
        2,
        "an amount already fixed does not follow the board"
    );
}

/// An authored `count` beside a `count_of` is the number created **per counted
/// permanent**, the way `amount_per` is on every other counted amount — and the count's
/// own filters still narrow what is counted.
#[test]
fn issue_722_an_authored_count_multiplies_the_counted_one() {
    let db = CardDatabase::from_json(COUNTED_TOKEN_MAKERS).expect("inline definitions");
    let mut state = main_phase();
    place(&mut state, &db, "test_bear", PlayerId(0));
    place(&mut state, &db, "test_bear", PlayerId(0));
    place(&mut state, &db, "test_marshal", PlayerId(0));

    // The Captain counts Bears only, so the Marshal beside it is not counted.
    let state = cast(&state, &db, "test_captain", Vec::new());
    let state = resolve_stack(&state, &db);
    assert_eq!(token_count(&state), 4, "two Soldiers for each of two Bears");
}

/// A count of zero creates nothing at all, rather than the one token the authored `count`
/// would otherwise say.
#[test]
fn issue_722_a_count_of_zero_creates_no_tokens() {
    let db = CardDatabase::from_json(COUNTED_TOKEN_MAKERS).expect("inline definitions");
    let state = main_phase();

    // The Captain counts Bears, and there are none — not even itself, which is a Human.
    let state = cast(&state, &db, "test_captain", Vec::new());
    let state = resolve_stack(&state, &db);
    assert_eq!(token_count(&state), 0);
}
