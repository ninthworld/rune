//! Alpine Moon (M19 #128): a **card named as the permanent enters** (CR 614.12), and a
//! static ability that reaches lands its controller does not control, takes their
//! abilities away, and hands them one back (CR 613 layer 6).
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! What has to be true is not that the definition parses: it is that the question is
//! asked before the permanent exists, that the answer is a *card identity* the catalog
//! holds, that the class the ability reaches is re-derived on every read rather than
//! settled when the source arrived, and that CR 613.7 timestamps decide the rest — across
//! two sources under two different controllers. Cards are named by their authored
//! `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    abilities_of_permanent, apply_action, named_card_candidates, pending_player_choice,
    valid_actions, Ability, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId,
    GameState, NamedCardClass, Permanent, PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game parked at player 0's precombat main with both pools stocked, so
/// payability never decides a test that is about a continuous effect.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, optionally attached
/// to `host`. Minting the id here is what gives a test control of CR 613.7 order: an
/// object placed earlier has the smaller id and therefore the earlier timestamp.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
    host: Option<PermanentId>,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        attached_to: host,
        ..Default::default()
    });
    id
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast `slug` from `seat`'s hand and pass it down to resolution. For a card that names
/// something as it enters, the state comes back *before* the permanent exists.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let instance = to_hand(&mut state, db, slug, seat);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Cast Alpine Moon under `seat`, name `named`, and hand back the resulting state with
/// the enchantment that arrived.
fn moon(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    named: &str,
) -> (GameState, PermanentId) {
    let before = cast(state, db, "alpine_moon", seat);
    let after = apply_action(
        &before,
        &Action::AnswerCardName {
            card: cid(db, named),
        },
        db,
    );
    let id = after
        .battlefield
        .iter()
        .find(|perm| !before.battlefield.iter().any(|old| old.id == perm.id))
        .expect("the Moon arrived once a name was chosen")
        .id;
    (after, id)
}

/// The abilities the permanent `id` currently has, after CR 613 layer 6.
fn abilities(state: &GameState, db: &CardDatabase, id: PermanentId) -> Vec<Ability> {
    let perm = state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .expect("the permanent is on the battlefield");
    abilities_of_permanent(state, db, perm)
}

/// The one activated ability `{T}: Add one mana of any color` Alpine Moon hands out, as a
/// value to compare against — read off the catalog rather than written out here, so a
/// test cannot quietly disagree with the card.
fn granted_by_the_moon(db: &CardDatabase) -> Ability {
    let data = db.card(cid(db, "alpine_moon")).expect("a bundled card");
    data.abilities
        .iter()
        .find_map(|ability| match ability {
            Ability::Static {
                modification: sage_engine::StaticModification::GrantAbility { ability },
                ..
            } => Some((**ability).clone()),
            _ => None,
        })
        .expect("Alpine Moon grants an ability")
}

// ----- the name is part of entering -----------------------------------------

/// CR 614.12: the card is named *as* Alpine Moon enters, so there is no instant at which
/// it is on the battlefield without one. The spell has left the stack and the card is in
/// no zone at all, and the only thing anyone may do is answer.
#[test]
fn cr_614_12_the_name_is_chosen_before_the_permanent_exists() {
    let db = db();
    let state = cast(&main_phase(), &db, "alpine_moon", PlayerId(0));

    assert!(
        state.battlefield.is_empty(),
        "the permanent must not arrive before a name is chosen"
    );
    assert!(state.stack.is_empty(), "the spell has finished resolving");
    assert!(
        state.players.iter().all(|player| {
            player.hand.is_empty() && player.graveyard.is_empty() && player.exile.is_empty()
        }),
        "the card waits in no zone, as a suspended spell's card does"
    );

    let pending = pending_player_choice(&state).expect("a name is owed");
    assert_eq!(pending.chooser, PlayerId(0), "its controller answers");
    assert_eq!(state.priority, PlayerId(0));
    assert!(
        pending.resume.is_none(),
        "nothing was suspended: the entry is the last step of the resolution"
    );

    let offered = valid_actions(&state, &db);
    assert!(
        offered
            .iter()
            .all(|action| matches!(action, Action::AnswerCardName { .. } | Action::Concede)),
        "nothing else is legal while the question is owed: {offered:?}"
    );
    assert!(
        valid_actions(
            &GameState {
                priority: PlayerId(1),
                ..state.clone()
            },
            &db
        )
        .is_empty(),
        "no other seat may act meanwhile"
    );
}

/// The answer is a **functional identity**, and it is one the catalog holds: the candidate
/// list is derived from the catalog on every read, and it is exactly the nonbasic lands.
#[test]
fn the_candidates_are_the_catalog_s_nonbasic_lands_and_nothing_else() {
    let db = db();
    let candidates = named_card_candidates(&db, NamedCardClass::NonbasicLand);

    assert!(candidates.contains(&cid(&db, "highland_lake")));
    assert!(candidates.contains(&cid(&db, "detection_tower")));
    assert!(
        !candidates.contains(&cid(&db, "plains")),
        "a basic land is not a nonbasic one (CR 205.4a)"
    );
    assert!(
        !candidates.contains(&cid(&db, "shock")),
        "only land cards may be named"
    );
    assert!(
        candidates.windows(2).all(|pair| pair[0] < pair[1]),
        "the list is deterministic, so two runs of one game offer it in one order"
    );
}

/// The recorded answer sticks on the permanent, and two copies name independently.
#[test]
fn the_named_card_is_recorded_on_the_permanent_that_arrives() {
    let db = db();
    let (state, first) = moon(&main_phase(), &db, PlayerId(0), "highland_lake");
    let (state, second) = moon(&state, &db, PlayerId(0), "detection_tower");

    let named = |id: PermanentId, state: &GameState| {
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == id)
            .expect("on the battlefield")
            .named_card
    };
    assert_eq!(named(first, &state), Some(cid(&db, "highland_lake")));
    assert_eq!(named(second, &state), Some(cid(&db, "detection_tower")));
}

/// The legal-posture rule, enforced: an answer the engine did not derive is refused, so
/// nothing a client sends can put an undefined name into the game state. A handle no card
/// interns, and a card that is real but outside the class the ability named, are both
/// rejected — and rejecting leaves the question owed rather than letting the game move on.
#[test]
fn an_answer_the_catalog_does_not_contain_is_rejected() {
    let db = db();
    let state = cast(&main_phase(), &db, "alpine_moon", PlayerId(0));

    for refused in [
        // No card interns this handle — the shape of a forged or stale answer.
        CardId(u64::MAX),
        // A real card, but a basic land: outside the class Alpine Moon named.
        cid(&db, "plains"),
        // A real card that is not a land at all.
        cid(&db, "shock"),
    ] {
        let after = apply_action(&state, &Action::AnswerCardName { card: refused }, &db);
        assert_eq!(
            after, state,
            "{refused:?} is not a legal answer and must change nothing"
        );
        assert!(pending_player_choice(&after).is_some(), "still owed");
    }
}

// ----- reaching past the source's own controller ----------------------------

/// The whole of issue #743: the modification applies to permanents the source's
/// controller does **not** control. The opponent's named land loses everything it printed
/// and has exactly the one ability Alpine Moon grants — offered to its own controller by
/// the same [`valid_actions`] a printed ability goes through.
#[test]
fn issue_743_the_static_reaches_a_land_its_controller_does_not_control() {
    let db = db();
    let mut state = main_phase();
    let theirs = place(&mut state, &db, "highland_lake", PlayerId(1), None);
    let (state, _) = moon(&state, &db, PlayerId(0), "highland_lake");

    assert_eq!(
        abilities(&state, &db, theirs),
        vec![granted_by_the_moon(&db)],
        "the printed blue and red mana abilities are gone and the grant is there"
    );

    // And it is a real activation for the seat that controls the land, not a fact only
    // the layer system knows.
    let mut theirs_turn = state.clone();
    theirs_turn.priority = PlayerId(1);
    theirs_turn.consecutive_passes = 0;
    let offered = valid_actions(&theirs_turn, &db);
    assert!(
        offered.contains(&Action::ActivateAbility {
            permanent: theirs,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        }),
        "the granted mana ability is offered to the land's controller: {offered:?}"
    );
}

/// The class is narrowed three ways, and each of the three is re-asked on every read: the
/// card must match the name that was chosen, must be a land, and must be an opponent's.
#[test]
fn a_different_name_a_different_type_and_the_controller_s_own_are_all_untouched() {
    let db = db();
    let mut state = main_phase();
    let named = place(&mut state, &db, "highland_lake", PlayerId(1), None);
    let other = place(&mut state, &db, "detection_tower", PlayerId(1), None);
    let mine = place(&mut state, &db, "highland_lake", PlayerId(0), None);
    let creature = place(&mut state, &db, "onakke_ogre", PlayerId(1), None);

    let printed_of = |state: &GameState, id| abilities(state, &db, id);
    let before_other = printed_of(&state, other);
    let before_mine = printed_of(&state, mine);
    let before_creature = printed_of(&state, creature);

    let (state, _) = moon(&state, &db, PlayerId(0), "highland_lake");

    assert_eq!(
        abilities(&state, &db, named),
        vec![granted_by_the_moon(&db)]
    );
    assert_eq!(
        printed_of(&state, other),
        before_other,
        "a land with a different name is not in the class"
    );
    assert_eq!(
        printed_of(&state, mine),
        before_mine,
        "the source's controller's own lands are not in the class"
    );
    assert_eq!(
        printed_of(&state, creature),
        before_creature,
        "an opponent's creature is not a land"
    );
}

/// A static ability is derived from its source's presence and nothing else (ADR 0005 §1),
/// so it starts and stops with the source — including for a land that arrived *after* it,
/// which is what separates a re-derived class from a resolution-time one.
#[test]
fn it_covers_a_land_that_arrives_later_and_stops_the_instant_the_source_leaves() {
    let db = db();
    let (mut state, moon_id) = moon(&main_phase(), &db, PlayerId(0), "highland_lake");

    // Nothing was decided when the Moon arrived: a land that turns up afterwards is in
    // the class the moment it is there.
    let late = place(&mut state, &db, "highland_lake", PlayerId(1), None);
    assert_eq!(abilities(&state, &db, late), vec![granted_by_the_moon(&db)]);
    let printed = sage_engine::abilities_of(&db, cid(&db, "highland_lake"));
    assert_ne!(
        abilities(&state, &db, late),
        printed,
        "while the Moon is there the land is not what it printed"
    );

    // And the effect ends with the source, with nothing to prune.
    state.battlefield.retain(|perm| perm.id != moon_id);
    assert_eq!(
        abilities(&state, &db, late),
        printed,
        "the land has its own abilities back the instant the Moon is gone"
    );
}

// ----- CR 613.7, across two controllers -------------------------------------

/// CR 613.7: an older source's modification applies before a newer one's, whoever
/// controls them. Alpine Moon belongs to player 0 and the Aura on the land belongs to
/// player 1, and only the order they arrived in decides the answer.
///
/// This is also the interaction with the fold that grants a whole ability: the Aura's
/// grant and the Moon's loses-all are two entries in one ordered list, and neither knows
/// the other exists.
#[test]
fn cr_613_7_two_sources_under_different_controllers_apply_in_timestamp_order() {
    let db = db();
    let granted = granted_by_the_moon(&db);
    let aura_grant = {
        let data = db
            .card(cid(&db, "gift_of_paradise"))
            .expect("a bundled card");
        data.attachment
            .as_ref()
            .expect("an Aura")
            .abilities
            .first()
            .expect("it grants an ability")
            .clone()
    };

    // The Aura is older than the Moon: its grant applies first and the Moon's loses-all
    // takes it away again, leaving only what the Moon then grants.
    let aura_first = {
        let mut state = main_phase();
        let land = place(&mut state, &db, "highland_lake", PlayerId(1), None);
        place(&mut state, &db, "gift_of_paradise", PlayerId(1), Some(land));
        let (state, _) = moon(&state, &db, PlayerId(0), "highland_lake");
        abilities(&state, &db, land)
    };
    assert_eq!(
        aura_first,
        vec![granted.clone()],
        "a grant before a loses-all is wiped by it"
    );

    // The Moon is older: its loses-all applies first, its own grant next, and the Aura's
    // grant lands on top of both.
    let moon_first = {
        let mut state = main_phase();
        let land = place(&mut state, &db, "highland_lake", PlayerId(1), None);
        let (mut state, _) = moon(&state, &db, PlayerId(0), "highland_lake");
        place(&mut state, &db, "gift_of_paradise", PlayerId(1), Some(land));
        abilities(&state, &db, land)
    };
    assert_eq!(
        moon_first,
        vec![granted, aura_grant],
        "a grant after a loses-all still grants, and joins the one the loses-all's own \
         source made"
    );
}

// ----- the seam is unchanged for every other card ---------------------------

/// A card that names nothing still enters in one action, with no question posed and no
/// name recorded — the deferral is opt-in, declared by the card.
#[test]
fn a_card_that_names_no_card_enters_as_it_always_did() {
    let db = db();
    let state = cast(&main_phase(), &db, "manalith", PlayerId(0));

    assert!(pending_player_choice(&state).is_none());
    assert_eq!(state.battlefield.len(), 1, "it arrived straight away");
    assert_eq!(state.battlefield[0].named_card, None);
}
