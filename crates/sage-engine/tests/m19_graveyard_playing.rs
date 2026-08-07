//! Playing a card from a graveyard, and the two M19 cards that ask for it.
//!
//! A land is **played**, never cast (CR 116.2a), so the permission that reaches one is
//! not the permission that lets a spell be cast from a graveyard: Crucible of Worlds
//! (M19 #229) is a continuous ability *about its controller*, read where the land play is
//! offered and lasting exactly as long as the artifact is on the battlefield. Everything
//! else about the play is unchanged — one per turn, sorcery speed, the active player's —
//! because those gates are asked of the play rather than of the zone it came from.
//!
//! Talons of Wildwood (M19 #202) is the other half: an Aura whose activated ability
//! functions from the graveyard it is in and returns its own card to hand, which the
//! graveyard-activation seam already carries. It is here because it is the card that
//! proves an ability of a *non-creature* card works from a graveyard, and that an
//! attachment and a graveyard ability coexist on one definition.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, plays_lands_from_graveyard, valid_actions, Action, CardDatabase, CardId,
    CardInstance, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, each seat floating `mana` of every
/// colour.
fn main_phase(db: &CardDatabase, mana: u8) -> GameState {
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
            state.players[seat].mana_pool.add(color, mana);
        }
        state.players[seat].mana_pool.add_colorless(mana);
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `seat`, and return its id.
fn place(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller: seat,
        ..Default::default()
    });
    id
}

/// Put a card of `slug` into `seat`'s graveyard and return the instance.
fn bury(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// Pass priority until the top of the stack has resolved.
fn resolve(state: &GameState, db: &CardDatabase) -> GameState {
    let after = apply_action(state, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

// ----- the permission -------------------------------------------------------

#[test]
fn issue_723_a_land_in_a_graveyard_is_playable_only_under_the_permission() {
    // The whole card, in one comparison: the same land, in the same graveyard, with and
    // without the artifact that says it may be played. Nothing about the land differs.
    let db = db();
    let mut state = main_phase(&db, 0);
    let land = bury(&mut state, &db, "forest", PlayerId(0));

    assert!(
        !plays_lands_from_graveyard(&state, PlayerId(0), &db),
        "no permanent grants the permission"
    );
    assert!(
        !valid_actions(&state, &db).contains(&Action::PlayLand { card: land }),
        "a graveyard land is not playable on its own"
    );

    place(&mut state, &db, "crucible_of_worlds", PlayerId(0));
    assert!(plays_lands_from_graveyard(&state, PlayerId(0), &db));
    assert!(
        valid_actions(&state, &db).contains(&Action::PlayLand { card: land }),
        "the Crucible offers it"
    );
}

#[test]
fn issue_723_the_permission_ends_with_the_permanent_that_grants_it() {
    // Derived on every read, never stored (ADR 0005 §1): the answer is a fact about the
    // board as it stands, so removing the source removes the permission with no pruning
    // step and nothing left behind to expire.
    let db = db();
    let mut state = main_phase(&db, 0);
    let land = bury(&mut state, &db, "forest", PlayerId(0));
    let crucible = place(&mut state, &db, "crucible_of_worlds", PlayerId(0));
    assert!(valid_actions(&state, &db).contains(&Action::PlayLand { card: land }));

    state.battlefield.retain(|perm| perm.id != crucible);
    assert!(
        !plays_lands_from_graveyard(&state, PlayerId(0), &db),
        "the permission is gone the instant its source is"
    );
    assert!(!valid_actions(&state, &db).contains(&Action::PlayLand { card: land }));
}

#[test]
fn issue_723_the_permission_reaches_its_controller_and_nobody_else() {
    // A graveyard is a per-player zone (CR 404.1) and the ability says "you". An
    // opponent's Crucible unlocks their own graveyard, never this seat's, and this
    // seat's Crucible does not reach into theirs.
    let db = db();
    let mut state = main_phase(&db, 0);
    let mine = bury(&mut state, &db, "forest", PlayerId(0));
    let theirs = bury(&mut state, &db, "forest", PlayerId(1));
    place(&mut state, &db, "crucible_of_worlds", PlayerId(1));

    assert!(
        !plays_lands_from_graveyard(&state, PlayerId(0), &db),
        "the opponent's artifact grants the opponent's permission"
    );
    assert!(!valid_actions(&state, &db).contains(&Action::PlayLand { card: mine }));
    assert!(
        !valid_actions(&state, &db).contains(&Action::PlayLand { card: theirs }),
        "and it never offers another seat's graveyard to the priority holder"
    );
}

// ----- the play itself ------------------------------------------------------

#[test]
fn issue_723_playing_a_land_from_a_graveyard_moves_it_and_spends_the_land_drop() {
    // CR 305.2: the permission changes which zone a land may come from and nothing else.
    // It is still the one land this turn, so a graveyard play locks out a hand play, and
    // the card leaves the graveyard for the battlefield exactly as a hand play leaves the
    // hand.
    let db = db();
    let mut state = main_phase(&db, 0);
    let buried = bury(&mut state, &db, "forest", PlayerId(0));
    let in_hand = state.new_instance(cid(&db, "mountain"));
    state.players[0].hand.push(in_hand);
    place(&mut state, &db, "crucible_of_worlds", PlayerId(0));

    let after = apply_action(&state, &Action::PlayLand { card: buried }, &db);
    assert!(
        after.players[0].graveyard.is_empty(),
        "the card left the graveyard"
    );
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.instance == buried.id),
        "and is on the battlefield as a fresh permanent"
    );
    assert!(after.land_played, "it was this turn's land");
    let offers = valid_actions(&after, &db);
    assert!(
        !offers.contains(&Action::PlayLand { card: in_hand }),
        "so the land in hand is no longer playable"
    );
}

#[test]
fn issue_723_a_graveyard_land_play_obeys_the_ordinary_timing() {
    // Sorcery speed and the active player, both asked of the *play* rather than of the
    // zone: an opponent with a Crucible and a graveyard full of lands may not play one
    // during this turn, and nobody may play one with something on the stack.
    let db = db();
    let mut state = main_phase(&db, 0);
    let theirs = bury(&mut state, &db, "forest", PlayerId(1));
    place(&mut state, &db, "crucible_of_worlds", PlayerId(1));

    state.priority = PlayerId(1);
    assert!(
        !valid_actions(&state, &db).contains(&Action::PlayLand { card: theirs }),
        "the non-active player never plays a land"
    );

    let mut mine = main_phase(&db, 0);
    let land = bury(&mut mine, &db, "forest", PlayerId(0));
    place(&mut mine, &db, "crucible_of_worlds", PlayerId(0));
    mine.step = Step::Upkeep;
    assert!(
        !valid_actions(&mine, &db).contains(&Action::PlayLand { card: land }),
        "and not outside a main phase"
    );
}

// ----- Talons of Wildwood ---------------------------------------------------

/// The requirement form of the Talons activation. Index 0 because the Aura prints no
/// other ability — its enchant grant is an attachment, not an ability in the list.
fn talons_offer(card: CardInstance) -> Action {
    Action::ActivateAbilityFromGraveyard {
        card,
        index: 0,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

#[test]
fn issue_723_talons_of_wildwood_buys_itself_back_out_of_the_graveyard() {
    // An Aura is not a creature and never was a permanent this game — the card went to
    // the graveyard from wherever it was, and the ability functions from there (CR 113.6)
    // exactly as a creature card's does.
    let db = db();
    let mut state = main_phase(&db, 0);
    let card = bury(&mut state, &db, "talons_of_wildwood", PlayerId(0));

    assert!(
        !valid_actions(&state, &db).contains(&talons_offer(card)),
        "an empty pool withholds the offer"
    );
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.add_colorless(2);
    assert!(valid_actions(&state, &db).contains(&talons_offer(card)));

    let activated = apply_action(&state, &talons_offer(card), &db);
    assert!(
        activated.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == card.id),
        "the card stays in the graveyard until the ability resolves"
    );

    let resolved = resolve(&activated, &db);
    assert!(resolved.players[0].graveyard.is_empty());
    assert!(
        resolved.players[0].hand.iter().any(|c| c.id == card.id),
        "and lands in its owner's hand"
    );
}

#[test]
fn issue_723_talons_of_wildwood_offers_nothing_from_the_battlefield() {
    // The mirror gate every graveyard ability shares: on the battlefield the source is a
    // permanent and there is no card in a graveyard for it to return, so the activation
    // is withheld rather than offered and then found to do nothing.
    let db = db();
    let mut state = main_phase(&db, 5);
    let id = place(&mut state, &db, "talons_of_wildwood", PlayerId(0));

    assert!(
        !valid_actions(&state, &db).iter().any(|action| matches!(
            action,
            Action::ActivateAbility { permanent, .. } if *permanent == id
        )),
        "the enchantment on the battlefield offers no activation"
    );
}
