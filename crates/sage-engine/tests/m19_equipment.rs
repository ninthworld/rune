//! Equipment, the equip action, and the one thing an attachment that outlives its host
//! does that an Aura does not (CR 301.5, CR 702.6, CR 704.5m/n).
//!
//! The Aura model half-covers Equipment, and every test here is about the other half. An
//! Aura arrives attached, stays on one object for as long as it exists, and dies with it;
//! an Equipment arrives attached to nothing, is moved onto a creature by an activation
//! that can be repeated, and survives whatever happens to whatever it was on. Three of
//! those four differences are behaviour the Aura path had no way to express at all.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, valid_actions, Action, CardDatabase, CardId, CardInstance,
    Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with pools stocked so payability never
/// decides a test that is about timing or attachment.
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

/// Put `slug` into player 0's hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    instance
}

/// The requirement form of `equipment`'s equip activation — the shape `valid_actions`
/// advertises, with no target filled in.
///
/// Index 0 because Marauder's Axe prints no other ability, and the derived equip ability
/// is appended after the authored ones: an Equipment that also printed a static ability
/// would put equip second, which is the ordering
/// [`abilities_of`](sage_engine::abilities_of) fixes for every reader at once.
fn equip_offer(equipment: PermanentId) -> Action {
    Action::ActivateAbility {
        permanent: equipment,
        index: 0,
        targets: Vec::new(),
    }
}

/// Activate `equipment`'s equip ability at `host`, and let it resolve.
fn equip(
    state: &GameState,
    db: &CardDatabase,
    equipment: PermanentId,
    host: PermanentId,
) -> GameState {
    let action = Action::ActivateAbility {
        permanent: equipment,
        index: 0,
        targets: vec![Target::Permanent(host)],
    };
    let state = apply_action(state, &action, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// What `id` is attached to, or `None` — including for a permanent that has left, which
/// is why this answers an `Option` rather than panicking.
fn attached_to(state: &GameState, id: PermanentId) -> Option<PermanentId> {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .and_then(|perm| perm.attached_to)
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// `id`'s current power, through the layer system — which is the only reading that counts.
fn power(state: &GameState, db: &CardDatabase, id: PermanentId) -> Option<i32> {
    characteristics(state, id, db).power
}

// ----- the equip action -----------------------------------------------------

#[test]
fn issue_728_an_equipment_enters_attached_to_nothing_and_equips_a_creature() {
    // CR 301.5c / 702.6b: the two halves of what an Equipment is. It is cast like any
    // other artifact — no target chosen, nothing enchanted on arrival — and only its
    // equip ability puts it on a creature. Cast for real rather than placed, because
    // "enters attached to nothing" is a fact about the cast path an Aura shares.
    let db = db();
    let mut state = main_phase(&db);
    let bear = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let axe = to_hand(&mut state, &db, "marauder_s_axe");

    let cast = Action::CastSpell {
        card: axe,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(
        valid_actions(&state, &db).contains(&cast),
        "an Equipment is cast with no target chosen"
    );
    let state = apply_action(&state, &cast, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let equipment = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == axe.id)
        .expect("the Axe resolved onto the battlefield")
        .id;
    assert_eq!(
        attached_to(&state, equipment),
        None,
        "an Equipment enters attached to nothing"
    );
    assert_eq!(
        power(&state, &db, bear),
        Some(3),
        "the Centaur is untouched"
    );

    // The equip ability is on offer and moves the Axe onto the creature.
    assert!(valid_actions(&state, &db).contains(&equip_offer(equipment)));
    let state = equip(&state, &db, equipment, bear);
    assert_eq!(attached_to(&state, equipment), Some(bear));
    assert_eq!(
        (
            power(&state, &db, bear),
            characteristics(&state, bear, &db).toughness
        ),
        (Some(5), Some(4)),
        "a 3/3 holding a +2/+1 Axe is a 5/4 (CR 613 layers 7c and 6)"
    );
}

#[test]
fn issue_728_equipping_a_second_creature_moves_the_grant_with_the_axe() {
    // CR 701.3c: attaching an already-attached Equipment unattaches it first, in the same
    // step. The grant follows because it is *derived* from the attachment (ADR 0005) —
    // nothing is migrated, and the old host is not left holding a stale modifier. This is
    // the case the Aura model explicitly excludes, so there was no code path for it.
    let db = db();
    let mut state = main_phase(&db);
    let first = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let second = place(&mut state, &db, "field_creeper", PlayerId(0));
    let axe = place(&mut state, &db, "marauder_s_axe", PlayerId(0));

    let state = equip(&state, &db, axe, first);
    assert_eq!(
        (power(&state, &db, first), power(&state, &db, second)),
        (Some(5), Some(2))
    );

    let state = equip(&state, &db, axe, second);
    assert_eq!(attached_to(&state, axe), Some(second), "the Axe moved");
    assert_eq!(
        (power(&state, &db, first), power(&state, &db, second)),
        (Some(3), Some(4)),
        "the first creature is a plain 3/3 again and the second carries the grant"
    );
}

// ----- what happens when the host leaves ------------------------------------

#[test]
fn issue_728_the_axe_survives_its_host_and_an_aura_does_not() {
    // CR 704.5m vs CR 704.5n, side by side on the same creature and in the same
    // state-based-action pass, because the whole feature is the difference between them.
    // One creature dies; the Aura on it goes to a graveyard, and the Equipment on it stays
    // on the battlefield unattached, ready to be equipped again.
    let db = db();
    let mut state = main_phase(&db);
    let host = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let axe = place(&mut state, &db, "marauder_s_axe", PlayerId(0));
    let mut state = equip(&state, &db, axe, host);

    // Knightly Valor is an M19 Aura granting +2/+2; attached directly, since how it got
    // there is not what this test is about.
    let aura = place(&mut state, &db, "knightly_valor", PlayerId(0));
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == aura)
        .expect("the Aura is on the battlefield")
        .attached_to = Some(host);
    let aura_card = state
        .battlefield
        .iter()
        .find(|perm| perm.id == aura)
        .expect("the Aura is on the battlefield")
        .instance;

    // Kill the host. Take Vengeance destroys a tapped creature, so the state-based actions
    // that follow are the real ones rather than a hand-edited battlefield.
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == host)
        .expect("the host is on the battlefield")
        .tapped = true;
    let spell = to_hand(&mut state, &db, "take_vengeance");
    let cast = Action::CastSpell {
        card: spell,
        targets: vec![Target::Permanent(host)],
        payment: Vec::new(),
    };
    let state = apply_action(&state, &cast, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!on_battlefield(&state, host), "the host died");

    // CR 704.5n: the Equipment is still there, and attached to nothing.
    assert!(
        on_battlefield(&state, axe),
        "an Equipment outlives the creature it was on"
    );
    assert_eq!(
        attached_to(&state, axe),
        None,
        "and becomes unattached rather than following it"
    );
    // CR 704.5m: the Aura did not.
    assert!(
        !on_battlefield(&state, aura),
        "an Aura still follows its host off the battlefield"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == aura_card),
        "into its owner's graveyard"
    );

    // And the Axe is immediately usable again, which is the point of surviving.
    let survivor = place_and_equip_again(&state, &db, axe);
    assert!(survivor, "the freed Equipment is offered on a new creature");
}

/// Put a fresh creature down under player 0 and report whether `axe`'s equip ability is
/// offered for it — the "ready to be used again" half of surviving a host.
fn place_and_equip_again(state: &GameState, db: &CardDatabase, axe: PermanentId) -> bool {
    let mut state = state.clone();
    let fresh = place(&mut state, db, "field_creeper", PlayerId(0));
    if !valid_actions(&state, db).contains(&equip_offer(axe)) {
        return false;
    }
    let state = equip(&state, db, axe, fresh);
    attached_to(&state, axe) == Some(fresh) && power(&state, db, fresh) == Some(4)
}

#[test]
fn issue_728_an_equipment_whose_host_stops_being_a_legal_permanent_falls_off() {
    // CR 704.5n reaches further than a death: any host that stops being a legal permanent
    // unattaches the Equipment. Bounced rather than killed here, so the check is the
    // state-based action reading the host and not the death seam doing cleanup.
    let db = db();
    let mut state = main_phase(&db);
    let host = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let axe = place(&mut state, &db, "marauder_s_axe", PlayerId(0));
    let state = equip(&state, &db, axe, host);

    let spell = {
        let mut state = state.clone();
        let card = to_hand(&mut state, &db, "disperse");
        (state, card)
    };
    let (state, bounce) = spell;
    let cast = Action::CastSpell {
        card: bounce,
        targets: vec![Target::Permanent(host)],
        payment: Vec::new(),
    };
    let state = apply_action(&state, &cast, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(!on_battlefield(&state, host), "the host was bounced");
    assert!(on_battlefield(&state, axe), "the Axe stayed");
    assert_eq!(attached_to(&state, axe), None);
}

// ----- the target is a rule, not an offer -----------------------------------

#[test]
fn issue_728_equip_can_only_be_aimed_at_a_creature_its_controller_controls() {
    // CR 702.6b. Refused at *apply*, not merely unoffered: an opponent's creature, a
    // noncreature permanent, and a stale id are each handed straight to `apply_action`,
    // which is a no-op — the Axe does not move and no mana is spent.
    let db = db();
    let mut state = main_phase(&db);
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let axe = place(&mut state, &db, "marauder_s_axe", PlayerId(0));

    for bad in [theirs, land, PermanentId(9999)] {
        let action = Action::ActivateAbility {
            permanent: axe,
            index: 0,
            targets: vec![Target::Permanent(bad)],
        };
        let after = apply_action(&state, &action, &db);
        assert_eq!(after, state, "an illegal equip target changes nothing");
    }

    // And with nothing legal to equip at all, the ability is not offered either — a
    // required slot with no candidate withholds the whole activation (CR 601.2c).
    assert!(!valid_actions(&state, &db).contains(&equip_offer(axe)));

    // An equip carrying no target is not a legal action either: the slot is required, so
    // "attach to nothing" is unrepresentable rather than a quietly ignored no-op.
    let mine = place(&mut state, &db, "field_creeper", PlayerId(0));
    assert!(valid_actions(&state, &db).contains(&equip_offer(axe)));
    let after = apply_action(&state, &equip_offer(axe), &db);
    assert_eq!(
        attached_to(&after, axe),
        None,
        "the requirement form is an advertisement, not an activation"
    );
    // The filled-in form is the one that works.
    let after = equip(&state, &db, axe, mine);
    assert_eq!(attached_to(&after, axe), Some(mine));
}

#[test]
fn issue_728_an_equip_whose_target_died_first_leaves_the_axe_where_it_was() {
    // CR 608.2b: the target is re-checked on resolution. The Equipment is not moved, not
    // unattached, and not destroyed — the activation simply does nothing, which is the
    // right answer precisely because attaching is a move rather than a creation.
    let db = db();
    let mut state = main_phase(&db);
    let keeper = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let doomed = place(&mut state, &db, "field_creeper", PlayerId(0));
    let axe = place(&mut state, &db, "marauder_s_axe", PlayerId(0));
    let state = equip(&state, &db, axe, keeper);

    // Activate the equip at the second creature, then kill it before the ability resolves.
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: axe,
            index: 0,
            targets: vec![Target::Permanent(doomed)],
        },
        &db,
    );
    let mut state = state;
    state.battlefield.retain(|perm| perm.id != doomed);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(on_battlefield(&state, axe), "the Axe is still there");
    assert_eq!(
        attached_to(&state, axe),
        Some(keeper),
        "and still on the creature it was already equipping"
    );
    assert_eq!(power(&state, &db, keeper), Some(5));
}

// ----- timing ---------------------------------------------------------------

#[test]
fn issue_728_equip_is_sorcery_speed_at_the_offer_and_at_the_apply() {
    // CR 702.6b. Both gates, because they are independent: the offer is what a client
    // sees, and the apply-time re-derivation is what stops a stale or forged action id
    // moving a sword mid-combat.
    let db = db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let axe = place(&mut state, &db, "marauder_s_axe", PlayerId(0));
    let action = Action::ActivateAbility {
        permanent: axe,
        index: 0,
        targets: vec![Target::Permanent(mine)],
    };
    assert!(valid_actions(&state, &db).contains(&equip_offer(axe)));

    // Outside a main phase: neither offered nor applied.
    let mut combat = state.clone();
    combat.step = Step::DeclareBlockers;
    assert!(!valid_actions(&combat, &db).contains(&equip_offer(axe)));
    assert_eq!(
        apply_action(&combat, &action, &db),
        combat,
        "a forged equip in combat is a no-op"
    );

    // On the opponent's turn, with its controller holding priority at instant speed.
    let mut theirs = state.clone();
    theirs.active_player = PlayerId(1);
    assert!(!valid_actions(&theirs, &db).contains(&equip_offer(axe)));
    assert_eq!(apply_action(&theirs, &action, &db), theirs);

    // With something on the stack, which is the window a response would use.
    let mut responding = state.clone();
    let bolt = to_hand(&mut responding, &db, "shock");
    let responding = apply_action(
        &responding,
        &Action::CastSpell {
            card: bolt,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    assert!(!responding.stack.is_empty());
    assert!(!valid_actions(&responding, &db).contains(&equip_offer(axe)));
    assert_eq!(apply_action(&responding, &action, &db), responding);
}

#[test]
fn issue_728_a_freshly_cast_equipment_equips_the_turn_it_arrives() {
    // Summoning sickness restricts `{T}` costs on creatures (CR 302.6), and equip is
    // neither: an Equipment cast this turn moves onto a creature immediately. Worth
    // asserting because "activated ability of a permanent that just arrived" is exactly
    // the shape the sickness gate is written against.
    let db = db();
    let mut state = main_phase(&db);
    state.turn = 3;
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let card = cid(&db, "marauder_s_axe");
    let instance = state.new_instance(card).id;
    let axe = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: axe,
        instance,
        printed: card.into(),
        controller: PlayerId(0),
        entered_turn: 3,
        ..Default::default()
    });

    assert!(valid_actions(&state, &db).contains(&equip_offer(axe)));
    let state = equip(&state, &db, axe, mine);
    assert_eq!(attached_to(&state, axe), Some(mine));
}
