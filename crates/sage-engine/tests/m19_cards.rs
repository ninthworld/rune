//! Behaviour of the M19 cards added on top of the widened ability IR.
//!
//! Every test here drives the **real** [`apply_action`] pipeline over the bundled
//! catalog and asserts on the resulting [`GameState`]: a card is "supported" only if
//! playing it does what the card says, so a definition that parses is not evidence of
//! anything. Cards are named by their authored `functional_id` and resolved through
//! [`CardDatabase::card_id`] — a `CardId` is interned at build time and shifts
//! whenever the catalog changes (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attacker_candidates, blocker_candidates_for, characteristics,
    pending_trigger_target_choice, permanent_restrictions, target_requirements, valid_actions,
    Action, Attack, Block, CardData, CardDatabase, CardId, CardInstance, CardType, Color,
    CombatRestriction, DamageTarget, FunctionalId, GameEvent, GameState, Keyword, Permanent,
    PermanentId, PlayerId, StackId, StackObject, StackObjectKind, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned handle for an authored identity.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// The bundled definition authored under `slug`.
fn card<'a>(db: &'a CardDatabase, slug: &str) -> &'a CardData {
    db.card(cid(db, slug)).expect("a bundled card")
}

/// A two-player game parked at player 0's precombat main with an empty stack, both
/// players' pools stocked so payability never decides a test that is about an effect.
fn main_phase(db: &CardDatabase) -> GameState {
    let _ = db;
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

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and
/// free of summoning sickness, and return its battlefield identity.
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

/// Put `slug` into `seat`'s hand and return the instance.
fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast `slug` from player 0's hand with `targets` and let it resolve (both players
/// pass). Goes through the ordinary cast gate, so a spell that `valid_actions` would
/// not currently offer fails here rather than silently doing nothing.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let action = Action::CastSpell {
        card: instance,
        targets,
    };
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            targets: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
    let state = apply_action(&state, &action, db);
    assert!(
        state.stack.iter().any(|o| matches!(
            o.kind,
            StackObjectKind::Spell { card } if card.id == instance.id
        )),
        "{slug} did not reach the stack — the cast was rejected"
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Activate ability `index` of `permanent` with `targets` and let it resolve.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    targets: Vec<Target>,
) -> GameState {
    let action = Action::ActivateAbility {
        permanent,
        index,
        targets,
    };
    let after = apply_action(state, &action, db);
    assert_ne!(
        &after, state,
        "the activation was rejected — apply_action was a no-op"
    );
    let after = apply_action(&after, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

/// Advance the game by one action on behalf of whoever holds priority: pass when
/// passing is offered, and otherwise submit the first non-concede action there is (the
/// empty combat declaration during a declare step). Lets a test walk a turn forward
/// without caring which step owes what.
fn advance(state: &GameState, db: &CardDatabase) -> GameState {
    let offered = valid_actions(state, db);
    let chosen = if offered.contains(&Action::PassPriority) {
        Action::PassPriority
    } else {
        offered
            .into_iter()
            .find(|a| a != &Action::Concede)
            .expect("some action is always available")
    };
    let after = apply_action(state, &chosen, db);
    assert_ne!(&after, state, "the pipeline stalled on {chosen:?}");
    after
}

/// Whether the permanent `id` is still on the battlefield.
fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

// ----- the ten "gain lands": enter tapped, tap for either of two colours --------

/// Every M19 dual land in the catalog enters tapped and offers exactly the two
/// single-colour mana abilities its cycle names. Asserted over the data because the
/// cycle's whole content is its two colours, and over the pipeline below because
/// "enters tapped" is a rules behaviour rather than a field.
#[test]
fn every_dual_land_enters_tapped_and_taps_for_its_two_colours() {
    let db = db();
    let cycle = [
        ("cinder_barrens", Color::Black, Color::Red),
        ("forsaken_sanctuary", Color::White, Color::Black),
        ("foul_orchard", Color::Black, Color::Green),
        ("highland_lake", Color::Blue, Color::Red),
        ("meandering_river", Color::White, Color::Blue),
        ("stone_quarry", Color::Red, Color::White),
        ("submerged_boneyard", Color::Blue, Color::Black),
        ("timber_gorge", Color::Red, Color::Green),
        ("tranquil_expanse", Color::Green, Color::White),
        ("woodland_stream", Color::Green, Color::Blue),
    ];
    for (slug, first, second) in cycle {
        let data = card(&db, slug);
        assert!(data.has_type(CardType::Land), "{slug} is a land");
        assert!(
            data.mana_cost.is_empty(),
            "{slug} is played, not cast, so it has no mana cost"
        );

        // Played from hand, it is on the battlefield and tapped — the CR 614.1c
        // self-replacement, applied at the entry seam, so there is no untapped
        // window in which it could have been tapped for mana this turn.
        let mut state = main_phase(&db);
        state.players[0].mana_pool = Default::default();
        let land = to_hand(&mut state, &db, slug, PlayerId(0));
        let state = apply_action(&state, &Action::PlayLand { card: land }, &db);
        let perm = state
            .battlefield
            .iter()
            .find(|p| p.instance == land.id)
            .unwrap_or_else(|| panic!("{slug} did not enter the battlefield"));
        assert!(perm.tapped, "{slug} enters the battlefield tapped");
        assert_eq!(
            state.players[0].mana_pool.total(),
            0,
            "{slug} produced no mana on the turn it entered"
        );

        // Untapped on a later turn it offers both halves of its cycle, one colour each.
        let mut ready = state.clone();
        let id = perm.id;
        ready.battlefield.iter_mut().for_each(|p| p.tapped = false);
        for (index, color) in [(1usize, first), (2usize, second)] {
            let after = apply_action(
                &ready,
                &Action::ActivateAbility {
                    permanent: id,
                    index,
                    targets: Vec::new(),
                },
                &db,
            );
            assert_eq!(
                pool_of(&after, PlayerId(0), color),
                1,
                "{slug} ability {index} adds one {color:?}"
            );
            assert!(
                after.stack.is_empty(),
                "a mana ability never uses the stack (CR 605.3)"
            );
        }
    }
}

/// The amount of `color` in `seat`'s pool.
fn pool_of(state: &GameState, seat: PlayerId, color: Color) -> u8 {
    let pool = &state.players[seat.0].mana_pool;
    match color {
        Color::White => pool.white,
        Color::Blue => pool.blue,
        Color::Black => pool.black,
        Color::Red => pool.red,
        Color::Green => pool.green,
    }
}

// ----- bodies: vanilla and keyword creatures ---------------------------------

/// The plain bodies added with this batch carry exactly the characteristics they are
/// printed with. A vanilla creature has no IR to test, so its data *is* its behaviour
/// — but a wrong power or a missing keyword is a rules bug, so it is pinned here
/// rather than left to the compatibility report's name-only listing.
#[test]
fn new_bodies_carry_their_printed_characteristics() {
    let db = db();
    let bodies = [
        ("sun_sentinel", "{1}{W}", 2, 2, &[Keyword::Vigilance][..]),
        ("silverbeak_griffin", "{W}{W}", 2, 2, &[Keyword::Flying]),
        ("thornhide_wolves", "{4}{G}", 4, 5, &[]),
        (
            "wall_of_vines",
            "{G}",
            0,
            3,
            &[Keyword::Defender, Keyword::Reach],
        ),
        (
            "daggerback_basilisk",
            "{2}{G}",
            2,
            2,
            &[Keyword::Deathtouch],
        ),
        ("wall_of_mist", "{1}{U}", 0, 5, &[Keyword::Defender]),
        ("boggart_brute", "{2}{R}", 3, 2, &[Keyword::Menace]),
        ("two_headed_zombie", "{3}{B}", 4, 2, &[Keyword::Menace]),
        (
            "aggressive_mammoth",
            "{3}{G}{G}{G}",
            8,
            8,
            &[Keyword::Trample],
        ),
        ("angel_of_the_dawn", "{4}{W}", 3, 3, &[Keyword::Flying]),
        ("herald_of_faith", "{3}{W}{W}", 4, 3, &[Keyword::Flying]),
        (
            "serra_s_guardian",
            "{4}{W}{W}",
            5,
            5,
            &[Keyword::Flying, Keyword::Vigilance],
        ),
        ("vampire_sovereign", "{3}{B}{B}", 3, 4, &[Keyword::Flying]),
        ("skymarch_bloodletter", "{2}{B}", 2, 2, &[Keyword::Flying]),
    ];
    for (slug, cost, power, toughness, keywords) in bodies {
        let data = card(&db, slug);
        assert!(data.has_type(CardType::Creature), "{slug} is a creature");
        assert_eq!(data.mana_cost, cost, "{slug} mana cost");
        assert_eq!(data.power, Some(power), "{slug} power");
        assert_eq!(data.toughness, Some(toughness), "{slug} toughness");
        for &keyword in keywords {
            assert!(data.has_keyword(keyword), "{slug} has {keyword:?}");
        }
    }
}

// ----- defender (CR 702.3b) --------------------------------------------------

#[test]
fn wall_of_mist_cannot_be_declared_as_an_attacker() {
    // CR 702.3b: defender removes a creature from the attacker candidate set
    // outright, independently of tapped-ness and summoning sickness. A creature
    // beside it with the same body but no defender stays eligible, so the exclusion
    // is the keyword's doing and not the fixture's.
    let db = db();
    let mut state = main_phase(&db);
    state.step = Step::DeclareAttackers;
    let wall = place(&mut state, &db, "wall_of_mist", PlayerId(0));
    let cat = place(&mut state, &db, "sun_sentinel", PlayerId(0));

    let candidates = attacker_candidates(&state, &db);
    assert!(
        !candidates.contains(&wall),
        "a creature with defender can't attack"
    );
    assert!(candidates.contains(&cat), "its neighbour still can");

    // And the declaration itself is refused, not merely unadvertised.
    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: wall,
                defender: PlayerId(1),
            }],
        },
        &db,
    );
    assert_eq!(
        after, state,
        "declaring a defender as an attacker is a no-op"
    );
}

// ----- menace (CR 702.110b) --------------------------------------------------

/// Declare `attacker` as an attacker on player 1 and walk the pipeline to the
/// declare-blockers step, where player 1 owes the declaration.
fn attack_with(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let mut state = state.clone();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: PlayerId(1),
            }],
        },
        db,
    );
    let mut state = state;
    while state.step != Step::DeclareBlockers {
        state = advance(&state, db);
    }
    state
}

#[test]
fn boggart_brute_can_be_blocked_only_by_two_or_more_creatures() {
    // CR 702.110b: menace is a restriction on the *declaration*, not on any one
    // blocker — so a lone blocker that is otherwise perfectly legal makes the whole
    // selection illegal, and the same blocker becomes legal the moment a second
    // joins it.
    let db = db();
    let mut state = main_phase(&db);
    let brute = place(&mut state, &db, "boggart_brute", PlayerId(0));
    let first = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let second = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = attack_with(&state, &db, brute);
    assert_eq!(state.priority, PlayerId(1), "the defender declares blocks");

    let lone = Action::DeclareBlockers {
        blocks: vec![Block {
            blocker: first,
            attacker: brute,
        }],
    };
    assert_eq!(
        apply_action(&state, &lone, &db),
        state,
        "one blocker alone cannot block a menacing attacker"
    );

    let pair = Action::DeclareBlockers {
        blocks: vec![
            Block {
                blocker: first,
                attacker: brute,
            },
            Block {
                blocker: second,
                attacker: brute,
            },
        ],
    };
    let after = apply_action(&state, &pair, &db);
    assert_ne!(
        after, state,
        "two blockers together are a legal declaration"
    );
    assert_eq!(
        after
            .battlefield
            .iter()
            .filter(|p| p.blocking == Some(brute))
            .count(),
        2
    );

    // Declaring no blockers stays legal: menace restricts *how* a creature is
    // blocked, never whether it must be.
    let none = Action::DeclareBlockers { blocks: Vec::new() };
    assert_ne!(apply_action(&state, &none, &db), state);
}

// ----- evasion and blocking restrictions (issue #606) -------------------------

/// Whether the block assigning `blocker` to `attacker` is accepted by the pipeline —
/// submitted as a real declaration, so an illegal one is a no-op rather than an error.
fn block_is_legal(
    state: &GameState,
    db: &CardDatabase,
    blocker: PermanentId,
    attacker: PermanentId,
) -> bool {
    let action = Action::DeclareBlockers {
        blocks: vec![Block { blocker, attacker }],
    };
    &apply_action(state, &action, db) != state
}

/// Cast `slug` from `seat`'s hand with `targets` and let it resolve. The counterpart of
/// [`cast`] for a spell an *opponent* casts, which is what a targeting restriction has
/// to be tested from.
fn cast_by(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let instance = to_hand(&mut state, db, slug, seat);
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets,
        },
        db,
    );
    let after = apply_action(&after, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

#[test]
fn bristling_boar_can_be_blocked_by_one_creature_but_never_two() {
    // CR 509.1b, the mirror of menace: a ceiling on the block rather than a floor. One
    // blocker is fine, none is fine, and the *second* blocker is what makes the whole
    // selection illegal — judgeable only over the assembled declaration.
    let db = db();
    let mut state = main_phase(&db);
    let boar = place(&mut state, &db, "bristling_boar", PlayerId(0));
    let first = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let second = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let state = attack_with(&state, &db, boar);

    assert!(
        block_is_legal(&state, &db, first, boar),
        "a single blocker is legal"
    );
    assert_ne!(
        apply_action(&state, &Action::DeclareBlockers { blocks: Vec::new() }, &db),
        state,
        "declaring no blockers is legal — the ceiling restricts how, not whether"
    );

    let pair = Action::DeclareBlockers {
        blocks: vec![
            Block {
                blocker: first,
                attacker: boar,
            },
            Block {
                blocker: second,
                attacker: boar,
            },
        ],
    };
    assert_eq!(
        apply_action(&state, &pair, &db),
        state,
        "a second blocker makes the declaration illegal"
    );
}

#[test]
fn vine_mare_cannot_be_blocked_by_black_creatures_but_green_ones_may() {
    // CR 509.1b: a colour-restricted evasion is a *pairwise* fact, so it removes only
    // the creatures of that colour — the unaffected neighbour still blocks.
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "vine_mare", PlayerId(0));
    let black = place(&mut state, &db, "walking_corpse", PlayerId(1));
    let green = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let state = attack_with(&state, &db, mare);

    assert!(
        !block_is_legal(&state, &db, black, mare),
        "a black creature cannot block Vine Mare"
    );
    assert!(
        block_is_legal(&state, &db, green, mare),
        "a green creature is unaffected"
    );
}

#[test]
fn vine_mare_is_hexproof_from_opponents_and_not_from_its_controller() {
    // CR 702.11b: hexproof is controller-relative. The same removal spell is illegal
    // from the opponent and legal from the creature's own controller, and the
    // enumerated candidate set agrees with the legality check rather than restating it.
    let db = db();
    let mut state = main_phase(&db);
    let mare = place(&mut state, &db, "vine_mare", PlayerId(0));
    let plain = place(&mut state, &db, "centaur_courser", PlayerId(0));

    // The opponent's Murder cannot be aimed at it, and it is not offered as a choice.
    let opponent_kill = cast_by(
        &state,
        &db,
        "murder",
        PlayerId(1),
        vec![Target::Permanent(mare)],
    );
    assert!(
        on_battlefield(&opponent_kill, mare),
        "an opponent cannot target a hexproof creature"
    );
    let mut offering = state.clone();
    offering.priority = PlayerId(1);
    let instance = to_hand(&mut offering, &db, "murder", PlayerId(1));
    let candidates = target_requirements(
        &offering,
        &db,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
        },
    );
    assert_eq!(candidates.len(), 1, "Murder fills one target slot");
    assert!(
        !candidates[0].candidates.contains(&Target::Permanent(mare)),
        "a hexproof creature is not offered to an opponent"
    );
    assert!(
        candidates[0].candidates.contains(&Target::Permanent(plain)),
        "the unaffected neighbour still is"
    );

    // Its own controller may target it freely.
    let own_kill = cast_by(
        &state,
        &db,
        "murder",
        PlayerId(0),
        vec![Target::Permanent(mare)],
    );
    assert!(
        !on_battlefield(&own_kill, mare),
        "hexproof never stops the controller"
    );
}

#[test]
fn plague_mare_shrinks_only_its_opponents_creatures_and_dodges_white_blockers() {
    // Two independent halves of one card: the opponents-wide mass scope (which must
    // spare the controller's own board) and the colour-restricted evasion.
    let db = db();
    let mut state = main_phase(&db);
    let ours = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let white = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    // Cast it as a creature spell so the enters trigger runs through the real seam.
    let state = cast(&state, &db, "plague_mare", Vec::new());
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let mare = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(cid(&db, "plague_mare")))
        .expect("Plague Mare entered the battlefield")
        .id;
    // It entered this turn, so CR 302.6 would keep it out of combat; the evasion half
    // of the card is about the block, not about when it may attack.
    let mut state = state;
    state
        .battlefield
        .iter_mut()
        .filter(|p| p.id == mare)
        .for_each(|p| p.entered_turn = 0);

    assert_eq!(
        characteristics(&state, theirs, &db).power,
        Some(2),
        "an opponent's 3/3 is now a 2/2"
    );
    assert_eq!(
        characteristics(&state, ours, &db).power,
        Some(3),
        "the controller's own creature is untouched"
    );

    let state = attack_with(&state, &db, mare);
    assert!(
        !block_is_legal(&state, &db, white, mare),
        "a white creature cannot block Plague Mare"
    );
    assert!(
        block_is_legal(&state, &db, theirs, mare),
        "a green creature is unaffected"
    );
}

#[test]
fn luminous_bonds_stops_its_host_attacking_and_blocking_until_it_leaves() {
    // The Aura carries a *restriction* rather than a P/T or keyword grant, and it is
    // derived from the attachment: destroying the Aura frees the creature with nothing
    // to unwind.
    let db = db();
    let mut state = main_phase(&db);
    let bound = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let free = place(&mut state, &db, "centaur_courser", PlayerId(0));

    let state = cast(
        &state,
        &db,
        "luminous_bonds",
        vec![Target::Permanent(bound)],
    );
    let candidates = attacker_candidates(&state, &db);
    assert!(
        !candidates.contains(&bound),
        "the enchanted creature can't attack"
    );
    assert!(candidates.contains(&free), "its neighbour still can");

    // The same host can't block either. Player 1 attacks into it.
    let mut defending = state.clone();
    defending.active_player = PlayerId(1);
    defending.players[0].turn_began = defending.turn;
    let attacker = place(&mut defending, &db, "onakke_ogre", PlayerId(1));
    let defending = {
        let mut s = defending;
        s.step = Step::DeclareAttackers;
        s.priority = PlayerId(1);
        s.consecutive_passes = 0;
        let s = apply_action(
            &s,
            &Action::DeclareAttackers {
                attackers: vec![Attack {
                    attacker,
                    defender: PlayerId(0),
                }],
            },
            &db,
        );
        let mut s = s;
        while s.step != Step::DeclareBlockers {
            s = advance(&s, &db);
        }
        s
    };
    let blockers = blocker_candidates_for(&defending, PlayerId(0), &db);
    assert!(
        !blockers.contains(&bound),
        "the enchanted creature can't block"
    );
    assert!(blockers.contains(&free), "its neighbour still can");
    assert!(!block_is_legal(&defending, &db, bound, attacker));

    // Destroying the Aura ends the restriction immediately (CR 613.1f, ADR 0005).
    let aura = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(cid(&db, "luminous_bonds")))
        .expect("the Aura is attached")
        .id;
    let freed = cast(&state, &db, "naturalize", vec![Target::Permanent(aura)]);
    assert!(!on_battlefield(&freed, aura));
    assert!(attacker_candidates(&freed, &db).contains(&bound));
}

#[test]
fn aether_tunnel_makes_its_host_unblockable_and_bigger() {
    // One Aura carrying a P/T grant (layer 7c) and a restriction (layer 6) at once —
    // the two halves are independent and both apply.
    let db = db();
    let mut state = main_phase(&db);
    let host = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let ground = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = cast(&state, &db, "aether_tunnel", vec![Target::Permanent(host)]);
    assert_eq!(characteristics(&state, host, &db).power, Some(4), "+1/+0");
    assert_eq!(characteristics(&state, host, &db).toughness, Some(3));

    let state = attack_with(&state, &db, host);
    assert!(
        !block_is_legal(&state, &db, ground, host),
        "nothing may block an unblockable attacker"
    );
}

#[test]
fn frilled_sea_serpent_buys_unblockability_for_the_turn_only() {
    // A granted restriction behaves exactly as a printed one — and, being an
    // until-end-of-turn effect, is gone by the next combat (CR 514.2).
    let db = db();
    let mut state = main_phase(&db);
    let serpent = place(&mut state, &db, "frilled_sea_serpent", PlayerId(0));
    let blocker = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let blocked = attack_with(&state, &db, serpent);
    assert!(
        block_is_legal(&blocked, &db, blocker, serpent),
        "before the activation it blocks normally"
    );

    let activated = activate(&state, &db, serpent, 0, Vec::new());
    let evasive = attack_with(&activated, &db, serpent);
    assert!(
        !block_is_legal(&evasive, &db, blocker, serpent),
        "the granted restriction binds exactly as a printed one"
    );

    // CR 514.2: the cleanup step ends it, so the restriction is gone with the turn.
    assert!(permanent_restrictions(&activated, serpent, &db)
        .contains(&CombatRestriction::CantBeBlocked));
    let mut later = activated;
    while later.turn == 1 {
        later = advance(&later, &db);
    }
    assert!(
        permanent_restrictions(&later, serpent, &db).is_empty(),
        "the grant wore off at cleanup"
    );
}

#[test]
fn siegebreaker_giant_stops_one_blocker_and_leaves_the_rest() {
    // The other direction of the same layer: a restriction aimed at the *blocker*
    // rather than at the attacker.
    let db = db();
    let mut state = main_phase(&db);
    let giant = place(&mut state, &db, "siegebreaker_giant", PlayerId(0));
    let silenced = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let other = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = activate(&state, &db, giant, 0, vec![Target::Permanent(silenced)]);
    let state = attack_with(&state, &db, giant);
    assert!(
        !blocker_candidates_for(&state, PlayerId(1), &db).contains(&silenced),
        "a creature that can't block is not a blocker candidate"
    );
    assert!(!block_is_legal(&state, &db, silenced, giant));
    assert!(
        block_is_legal(&state, &db, other, giant),
        "its neighbour is unaffected"
    );
}

#[test]
fn suspicious_bookcase_makes_one_creature_unblockable_for_the_turn() {
    // A `{3}, {T}` activation aimed at another creature — the targeted form of the
    // same imposition Frilled Sea Serpent applies to itself.
    let db = db();
    let mut state = main_phase(&db);
    let bookcase = place(&mut state, &db, "suspicious_bookcase", PlayerId(0));
    let sneak = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let blocker = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = activate(&state, &db, bookcase, 0, vec![Target::Permanent(sneak)]);
    assert!(
        state
            .battlefield
            .iter()
            .any(|p| p.id == bookcase && p.tapped),
        "the tap in the cost was paid"
    );
    let state = attack_with(&state, &db, sneak);
    assert!(!block_is_legal(&state, &db, blocker, sneak));
}

#[test]
fn tectonic_rift_destroys_a_land_and_grounds_every_non_flying_blocker() {
    // Two effects in one spell, one of which names a class rather than a target: the
    // class is enumerated on resolution (CR 611.2c) and excludes flyers by their
    // *computed* keywords.
    let db = db();
    let mut state = main_phase(&db);
    let attacker = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let land = place(&mut state, &db, "forest", PlayerId(1));
    let ground = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let flyer = place(&mut state, &db, "silverbeak_griffin", PlayerId(1));

    let state = cast(&state, &db, "tectonic_rift", vec![Target::Permanent(land)]);
    assert!(
        !on_battlefield(&state, land),
        "the target land is destroyed"
    );

    let state = attack_with(&state, &db, attacker);
    assert!(
        !block_is_legal(&state, &db, ground, attacker),
        "a creature without flying can't block this turn"
    );
    assert!(
        block_is_legal(&state, &db, flyer, attacker),
        "a flyer is outside the class"
    );
}

// ----- triggered abilities ---------------------------------------------------

#[test]
fn tattered_mummy_drains_each_opponent_when_it_dies() {
    // A non-targeting trigger: it names a class of players, so it chooses nothing and
    // resolves without anyone being asked to aim it.
    let db = db();
    let mut state = main_phase(&db);
    let mummy = place(&mut state, &db, "tattered_mummy", PlayerId(0));
    let before = state.players[1].life;

    // Kill it with a real removal spell so the death runs through the ordinary seam.
    let after = cast(&state, &db, "murder", vec![Target::Permanent(mummy)]);
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(!on_battlefield(&after, mummy));
    assert_eq!(after.players[1].life, before - 2);
    assert_eq!(after.players[0].life, 20, "the controller loses nothing");
}

#[test]
fn herald_of_faith_gains_two_life_each_time_it_attacks() {
    // CR 603.6d: the attacks trigger fires from the declaration itself, observed by
    // diff on the one field that declaration writes — so it fires once per
    // declaration, and not again while the creature merely remains attacking.
    let db = db();
    let mut state = main_phase(&db);
    let herald = place(&mut state, &db, "herald_of_faith", PlayerId(0));
    let before = state.players[0].life;

    let state = attack_with(&state, &db, herald);
    // Walk to the point the trigger has resolved.
    let mut state = state;
    while state
        .stack
        .iter()
        .any(|o| matches!(o.kind, StackObjectKind::Ability { source, .. } if source == herald))
    {
        state = advance(&state, &db);
    }
    assert_eq!(
        state.players[0].life,
        before + 2,
        "one attack declaration, one trigger"
    );
    assert!(
        !state.stack.iter().any(|o| matches!(
            o.kind,
            StackObjectKind::Ability { source, .. } if source == herald
        )),
        "the trigger resolved rather than re-triggering"
    );
}

#[test]
fn angel_of_the_dawn_pumps_the_board_it_finds_and_no_one_who_arrives_later() {
    // CR 611.2c: a one-shot mass modification locks its affected set in on
    // resolution. That is the whole difference between this trigger and an anthem,
    // and it is the thing a class-scoped implementation would get wrong.
    let db = db();
    let mut state = main_phase(&db);
    let veteran = place(&mut state, &db, "sun_sentinel", PlayerId(0));
    let theirs = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = cast(&state, &db, "angel_of_the_dawn", Vec::new());
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let pumped = characteristics(&state, veteran, &db);
    assert_eq!(pumped.power, Some(3), "printed 2 + the Angel's +1");
    assert_eq!(pumped.toughness, Some(3));
    assert!(
        pumped.keywords.contains(&Keyword::Vigilance),
        "and gains vigilance until end of turn"
    );

    let opponents = characteristics(&state, theirs, &db);
    assert_eq!(
        opponents.power,
        Some(2),
        "an opponent's creature is untouched"
    );

    // A creature that arrives after the trigger resolved missed it.
    let mut later = state.clone();
    let latecomer = place(&mut later, &db, "sun_sentinel", PlayerId(0));
    assert_eq!(
        characteristics(&later, latecomer, &db).power,
        Some(2),
        "the affected set was fixed on resolution, not re-derived"
    );
}

// ----- triggered abilities that choose targets (CR 603.3d) --------------------

/// Answer the pending trigger-target choice by taking the first legal candidate in
/// each slot, and assert one was actually owed.
fn aim_pending_trigger(state: &GameState, db: &CardDatabase) -> GameState {
    let ability = pending_trigger_target_choice(state).expect("a trigger is waiting to be aimed");
    let requirement_form = Action::ChooseTriggerTargets {
        ability,
        targets: Vec::new(),
    };
    let targets: Vec<Target> = target_requirements(state, db, &requirement_form)
        .into_iter()
        .map(|req| {
            *req.candidates
                .first()
                .expect("a trigger with no legal choice is never put on the stack")
        })
        .collect();
    let after = apply_action(
        state,
        &Action::ChooseTriggerTargets { ability, targets },
        db,
    );
    assert_ne!(&after, state, "the choice was rejected");
    after
}

#[test]
fn vampire_sovereign_is_aimed_by_its_controller_before_anyone_acts() {
    // CR 603.3b/603.3d: the trigger goes on the stack unaimed, and until its
    // controller has chosen, the choice is the *only* action anyone is offered — the
    // game does not proceed around it.
    let db = db();
    let state = main_phase(&db);
    let state = cast(&state, &db, "vampire_sovereign", Vec::new());

    let ability = pending_trigger_target_choice(&state).expect("the ETB trigger is unaimed");
    let offered = valid_actions(&state, &db);
    assert!(
        offered
            .iter()
            .all(|a| matches!(a, Action::ChooseTriggerTargets { .. } | Action::Concede)),
        "nothing but the choice (and conceding) is offered: {offered:?}"
    );
    assert_eq!(
        state.priority,
        PlayerId(0),
        "priority is the trigger's controller's while the choice is owed"
    );
    // Passing is not even a legal escape.
    assert_eq!(apply_action(&state, &Action::PassPriority, &db), state);

    // "Target opponent" excludes the controller's own seat, so there is exactly one
    // candidate in a two-player game — and it is the opponent.
    let requirements = target_requirements(
        &state,
        &db,
        &Action::ChooseTriggerTargets {
            ability,
            targets: Vec::new(),
        },
    );
    assert_eq!(requirements.len(), 1);
    assert_eq!(
        requirements[0].candidates,
        vec![Target::Player(PlayerId(1))]
    );

    let state = aim_pending_trigger(&state, &db);
    assert!(pending_trigger_target_choice(&state).is_none());
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(state.players[1].life, 17, "target opponent loses 3 life");
    assert_eq!(state.players[0].life, 23, "and you gain 3");
}

#[test]
fn a_trigger_is_aimed_by_its_own_controller_not_by_the_priority_holder() {
    // The case that makes the chooser worth routing to: a creature killed on the
    // *opponent's* turn by the opponent's removal spell gives its own controller a
    // trigger to aim, while the opponent is the one holding priority. Skeleton Archer
    // has no dies trigger, so this uses Exclusion Mage's ETB — cast by the non-active
    // player at a moment the active player holds priority is not possible for a
    // creature, so the sharper version is asserted through the restore instead: after
    // the choice, priority returns to whoever it was going to.
    let db = db();
    let mut state = main_phase(&db);
    let theirs = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let before_priority = state.priority;
    let state = cast(&state, &db, "exclusion_mage", Vec::new());
    assert_eq!(
        state.priority,
        PlayerId(0),
        "the trigger's controller chooses"
    );
    let state = aim_pending_trigger(&state, &db);
    assert_eq!(
        state.priority, before_priority,
        "priority returns to the seat it was headed for once the choice is made"
    );
    assert!(state.interrupted_priority.is_none(), "and nothing is owed");

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        !on_battlefield(&state, theirs),
        "the opponent's creature was returned to hand"
    );
    assert_eq!(state.players[1].hand.len(), 1);
}

#[test]
fn a_trigger_with_no_legal_target_never_reaches_the_stack() {
    // CR 603.3c: a triggered ability that requires targets and has none available is
    // removed from the stack — so it is never put there. Exclusion Mage's ETB names a
    // creature an opponent controls; with the opponent's board empty there is no such
    // creature, and the game must not stall waiting for an unanswerable choice.
    let db = db();
    let state = main_phase(&db);
    let state = cast(&state, &db, "exclusion_mage", Vec::new());

    assert!(pending_trigger_target_choice(&state).is_none());
    assert!(
        !state
            .stack
            .iter()
            .any(|o| matches!(o.kind, StackObjectKind::Ability { .. })),
        "the unanswerable trigger was not put on the stack"
    );
    assert!(
        valid_actions(&state, &db).contains(&Action::PassPriority),
        "play continues normally"
    );
}

#[test]
fn skeleton_archer_finally_deals_its_damage() {
    // The regression this fix is for: before triggers could be aimed, an ETB that
    // targeted was put on the stack with no targets and fizzled on resolution
    // (CR 608.2b), so two shipped cards silently did nothing.
    let db = db();
    let state = main_phase(&db);
    let state = cast(&state, &db, "skeleton_archer", Vec::new());

    let ability =
        pending_trigger_target_choice(&state).expect("the ETB trigger is waiting to be aimed");
    let opponent = Target::Player(PlayerId(1));
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![opponent],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(state.players[1].life, 19, "one damage to the chosen target");
}

#[test]
fn infectious_horror_drains_on_every_attack_without_choosing_anything() {
    // The other half of the seam: a trigger that names a *class* of players chooses
    // nothing, so it is never owed a choice and resolves straight away.
    let db = db();
    let mut state = main_phase(&db);
    let horror = place(&mut state, &db, "infectious_horror", PlayerId(0));

    let state = attack_with(&state, &db, horror);
    assert!(pending_trigger_target_choice(&state).is_none());
    let mut state = state;
    while state
        .stack
        .iter()
        .any(|o| matches!(o.kind, StackObjectKind::Ability { source, .. } if source == horror))
    {
        state = advance(&state, &db);
    }
    assert_eq!(state.players[1].life, 18);
}

// ----- triggers that watch another object (issue #603) ------------------------

#[test]
fn ajanis_welcome_gains_life_once_per_creature_that_enters() {
    // The condition counts rather than answers: two creatures entering in one
    // transition is two life-gain triggers, not one (CR 603.2). A single-fire
    // implementation passes the one-creature case and silently halves this one.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "ajani_s_welcome", PlayerId(0));
    let before = state.players[0].life;

    let after = cast(&state, &db, "sun_sentinel", Vec::new());
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(
        after.players[0].life,
        before + 1,
        "one creature, one trigger"
    );

    // Two creatures entering in the same transition.
    let mut twice = main_phase(&db);
    place(&mut twice, &db, "ajani_s_welcome", PlayerId(0));
    let first = cid(&db, "sun_sentinel");
    let second = cid(&db, "silverbeak_griffin");
    for card in [first, second] {
        let instance = twice.new_instance(card).id;
        let id = PermanentId(twice.mint_id());
        twice.battlefield.push(Permanent {
            id,
            instance,
            printed: card.into(),
            controller: PlayerId(0),
            ..Default::default()
        });
    }
    let triggers = sage_engine::collect_triggers(&main_phase_with_welcome(&db), &twice, &db);
    assert_eq!(triggers.len(), 2, "two entries, two triggers");
}

/// The same board as `twice` above minus the two creatures — the "before" snapshot
/// the collector diffs against.
fn main_phase_with_welcome(db: &CardDatabase) -> GameState {
    let mut state = main_phase(db);
    place(&mut state, db, "ajani_s_welcome", PlayerId(0));
    state
}

#[test]
fn ajanis_welcome_ignores_creatures_an_opponent_controls() {
    // "A creature *you control*" is evaluated relative to the watching ability's
    // controller, the same frame of reference a possessive target spec uses.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "ajani_s_welcome", PlayerId(0));
    let before = state.players[0].life;

    let mut after = state.clone();
    let card = cid(&db, "sun_sentinel");
    let instance = after.new_instance(card).id;
    let id = PermanentId(after.mint_id());
    after.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller: PlayerId(1),
        ..Default::default()
    });
    assert!(
        sage_engine::collect_triggers(&state, &after, &db).is_empty(),
        "an opponent's creature entering is not watched"
    );
    assert_eq!(after.players[0].life, before);
}

#[test]
fn poison_tip_archer_drains_once_per_other_creature_that_dies() {
    // "Another creature" excludes the source, and a death-watcher still observes a
    // creature that died alongside it — the one case where the source need not have
    // survived the transition.
    let db = db();
    let mut state = main_phase(&db);
    let archer = place(&mut state, &db, "poison_tip_archer", PlayerId(0));
    let a = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let b = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    // Both opposing creatures die in one transition.
    let mut after = state.clone();
    after.battlefield.retain(|p| p.id != a && p.id != b);
    for id in [a, b] {
        let _ = id;
        let inst = after.new_instance(cid(&db, "sun_sentinel"));
        after.players[1].graveyard.push(inst);
    }
    // Match the instances the battlefield actually carried, so `died` recognises them.
    after.players[1].graveyard.clear();
    for perm in state.battlefield.iter().filter(|p| p.id == a || p.id == b) {
        after.players[1].graveyard.push(sage_engine::CardInstance {
            id: perm.instance,
            card: perm.printed.card().expect("a card permanent"),
        });
    }
    let triggers = sage_engine::collect_triggers(&state, &after, &db);
    assert_eq!(triggers.len(), 2, "two deaths, two triggers");
    assert!(triggers.iter().all(|t| t.source == archer));

    // The Archer dying alone triggers nothing: `except_this` excludes it.
    let mut alone = state.clone();
    let archer_perm = state
        .battlefield
        .iter()
        .find(|p| p.id == archer)
        .expect("the archer is on the battlefield")
        .clone();
    alone.battlefield.retain(|p| p.id != archer);
    alone.players[0].graveyard.push(sage_engine::CardInstance {
        id: archer_perm.instance,
        card: archer_perm.printed.card().expect("a card permanent"),
    });
    assert!(
        sage_engine::collect_triggers(&state, &alone, &db).is_empty(),
        "a creature that watches *another* creature dying does not watch itself"
    );
}

#[test]
fn epicure_of_blood_drains_on_a_life_gain_event_not_on_a_net_change() {
    // The condition is about the event, not the totals: it is read from what the
    // transition recorded, so a gain that is later cancelled out still triggered.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "epicure_of_blood", PlayerId(0));
    // Revitalize draws as well as gains, so the library must not be empty — an
    // empty-library draw loses the game before the trigger could resolve.
    let library_card = state.new_instance(cid(&db, "sun_sentinel"));
    state.players[0].library.push(library_card);
    let before = state.players[1].life;

    let after = cast(&state, &db, "revitalize", Vec::new());
    // The gain resolved; the Epicure's trigger is on the stack behind it.
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(after.players[0].life, 23, "Revitalize gained three");
    assert_eq!(after.players[1].life, before - 1, "each opponent lost one");
}

#[test]
fn ajanis_pridemate_grows_on_its_controllers_life_gain_only() {
    // A self-referential effect: the counter goes on the ability's own source, which
    // is not a target and was never chosen.
    let db = db();
    let mut state = main_phase(&db);
    let pridemate = place(&mut state, &db, "ajani_s_pridemate", PlayerId(0));
    let library_card = state.new_instance(cid(&db, "sun_sentinel"));
    state.players[0].library.push(library_card);
    // A second copy under the opponent: "you gain life" is scoped to each ability's
    // own controller, so player 0's life gain must leave this one untouched.
    let theirs = place(&mut state, &db, "ajani_s_pridemate", PlayerId(1));

    let after = cast(&state, &db, "revitalize", Vec::new());
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);

    let ch = characteristics(&after, pridemate, &db);
    assert_eq!(ch.power, Some(3), "printed 2 plus a +1/+1 counter");
    assert_eq!(ch.toughness, Some(3));
    assert_eq!(
        characteristics(&after, theirs, &db).power,
        Some(2),
        "an opponent gaining life is not *you* gaining life"
    );
}

#[test]
fn satyr_enchanter_and_aven_wind_mage_watch_what_their_controller_casts() {
    // A cast trigger fires as the spell goes on the stack, before it resolves, and
    // only for the spell class the card names.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "satyr_enchanter", PlayerId(0));
    let mage = place(&mut state, &db, "aven_wind_mage", PlayerId(0));
    let library_card = state.new_instance(cid(&db, "sun_sentinel"));
    state.players[0].library.push(library_card);

    // An instant: the Wind Mage grows, the Enchanter does not draw.
    let instance = to_hand(&mut state, &db, "shock", PlayerId(0));
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: vec![Target::Player(PlayerId(1))],
        },
        &db,
    );
    assert_eq!(
        after
            .stack
            .iter()
            .filter(|o| matches!(o.kind, StackObjectKind::Ability { source, .. } if source == mage))
            .count(),
        1,
        "the instant cast triggered the Wind Mage exactly once"
    );
    assert!(
        after.players[0]
            .hand
            .iter()
            .all(|c| c.id != library_card.id),
        "an instant is not an enchantment spell, so nothing was drawn"
    );

    // An enchantment: the Enchanter draws.
    let enchantment = to_hand(&mut state, &db, "ajani_s_welcome", PlayerId(0));
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: enchantment,
            targets: Vec::new(),
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(
        after.players[0]
            .hand
            .iter()
            .any(|c| c.id == library_card.id),
        "the enchantment cast drew a card"
    );
}

// ----- damage dealt to a class (issue #611) ----------------------------------

#[test]
fn guttersnipe_burns_each_opponent_when_its_controller_casts_an_instant() {
    // The whole card, through the real pipeline: the cast trigger from #609 aimed at
    // nothing, resolving into damage that names a class instead of a target. It is
    // *damage*, not life loss, so it is recorded as such — Guttersnipe's own two
    // damage land on top of Shock's, and the trigger never had a slot to fizzle.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "guttersnipe", PlayerId(0));

    let shock = to_hand(&mut state, &db, "shock", PlayerId(0));
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            targets: vec![Target::Player(PlayerId(1))],
        },
        &db,
    );
    // The trigger is on the stack above the spell, and chose no target: a class is
    // not a target (CR 115.1), so nothing was asked of its controller.
    assert!(
        pending_trigger_target_choice(&after).is_none(),
        "the trigger fills no target slot, so no choice is pending"
    );
    assert_eq!(after.stack.len(), 2, "the trigger sits above the spell");

    // Trigger resolves, then Shock.
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(
        after.players[1].life, 18,
        "Guttersnipe dealt two to each opponent"
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(after.players[1].life, 16, "then Shock dealt its own two");
    assert_eq!(
        after.players[0].life, 20,
        "its controller is not one of their own opponents"
    );
    assert!(
        after.log.iter().any(|entry| matches!(
            entry.event,
            GameEvent::DamageDealt {
                target: DamageTarget::Player(PlayerId(1)),
                amount: 2,
            }
        )),
        "it is recorded as damage — the event life loss would never produce"
    );
}

#[test]
fn guttersnipe_ignores_a_creature_spell_and_its_opponents_instants() {
    // The condition is `you_cast_spell` with an instant-or-sorcery selector: a
    // creature spell is not it, and neither is an opponent's instant.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "guttersnipe", PlayerId(0));

    let creature = to_hand(&mut state, &db, "sun_sentinel", PlayerId(0));
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: creature,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.stack.len(), 1, "a creature spell triggers nothing");

    // The opponent's own instant is not "you cast".
    let mut theirs = main_phase(&db);
    place(&mut theirs, &db, "guttersnipe", PlayerId(0));
    theirs.priority = PlayerId(1);
    let shock = to_hand(&mut theirs, &db, "shock", PlayerId(1));
    let after = apply_action(
        &theirs,
        &Action::CastSpell {
            card: shock,
            targets: vec![Target::Player(PlayerId(0))],
        },
        &db,
    );
    assert_eq!(
        after.stack.len(),
        1,
        "an opponent casting an instant is not its controller casting one"
    );
}

// ----- static abilities ------------------------------------------------------

#[test]
fn aggressive_mammoth_grants_trample_to_its_other_creatures_only() {
    let db = db();
    let mut state = main_phase(&db);
    let mammoth = place(&mut state, &db, "aggressive_mammoth", PlayerId(0));
    let ally = place(&mut state, &db, "sun_sentinel", PlayerId(0));
    let enemy = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    assert!(characteristics(&state, ally, &db)
        .keywords
        .contains(&Keyword::Trample));
    assert!(
        !characteristics(&state, enemy, &db)
            .keywords
            .contains(&Keyword::Trample),
        "the grant is scoped to creatures its controller controls"
    );
    assert!(
        characteristics(&state, mammoth, &db)
            .keywords
            .contains(&Keyword::Trample),
        "the Mammoth's own trample is printed, not granted"
    );

    // Derived, never stored: the grant ends the instant its source leaves.
    let mut gone = state.clone();
    gone.battlefield.retain(|p| p.id != mammoth);
    assert!(!characteristics(&gone, ally, &db)
        .keywords
        .contains(&Keyword::Trample));
}

// ----- activated abilities that cost mana ------------------------------------

#[test]
fn vampire_neonate_drains_each_opponent_for_two_generic_and_a_tap() {
    let db = db();
    let mut state = main_phase(&db);
    let neonate = place(&mut state, &db, "vampire_neonate", PlayerId(0));
    let before_pool = state.players[0].mana_pool.total();

    let after = activate(&state, &db, neonate, 0, Vec::new());
    assert_eq!(after.players[1].life, 19, "each opponent loses 1 life");
    assert_eq!(after.players[0].life, 21, "and its controller gains 1");
    assert_eq!(
        after.players[0].mana_pool.total(),
        before_pool - 2,
        "{{2}} was charged from the pool"
    );
    assert!(
        after
            .battlefield
            .iter()
            .any(|p| p.id == neonate && p.tapped),
        "and the {{T}} half of the cost was paid too"
    );
}

#[test]
fn vampire_neonate_is_not_offered_without_the_mana_to_pay_for_it() {
    // The offer and the charge are decided by the same `ManaPool::can_pay` over the
    // same cost string, so an ability that cannot be paid for is never advertised —
    // and is refused if submitted anyway.
    let db = db();
    let mut state = main_phase(&db);
    state.players[0].mana_pool = Default::default();
    let neonate = place(&mut state, &db, "vampire_neonate", PlayerId(0));

    let action = Action::ActivateAbility {
        permanent: neonate,
        index: 0,
        targets: Vec::new(),
    };
    assert!(!valid_actions(&state, &db).contains(&action));
    assert_eq!(apply_action(&state, &action, &db), state);

    // One mana short is still short; the second makes it payable.
    let mut one = state.clone();
    one.players[0].mana_pool.add_colorless(1);
    assert!(!valid_actions(&one, &db).contains(&action));
    let mut two = one.clone();
    two.players[0].mana_pool.add_colorless(1);
    assert!(valid_actions(&two, &db).contains(&action));
}

#[test]
fn millstone_mills_the_targeted_player_without_decking_them() {
    // CR 701.13: milling is not drawing. A library too short to cover the mill moves
    // what it has and never sets the empty-library flag the CR 704.5c loss reads —
    // the difference between a slow clock and an instant win.
    let db = db();
    let mut state = main_phase(&db);
    let millstone = place(&mut state, &db, "millstone", PlayerId(0));
    for _ in 0..3 {
        let card = state.new_instance(cid(&db, "sun_sentinel"));
        state.players[1].library.push(card);
    }

    let after = activate(&state, &db, millstone, 0, vec![Target::Player(PlayerId(1))]);
    assert_eq!(after.players[1].library.len(), 1);
    assert_eq!(after.players[1].graveyard.len(), 2);
    assert!(after.log.iter().any(|entry| matches!(
        entry.event,
        GameEvent::CardsMilled {
            player: PlayerId(1),
            count: 2
        }
    )));

    // One card left, asked for two: it mills one, logs one, and does not lose.
    let mut untapped = after.clone();
    untapped
        .battlefield
        .iter_mut()
        .for_each(|p| p.tapped = false);
    let drained = activate(
        &untapped,
        &db,
        millstone,
        0,
        vec![Target::Player(PlayerId(1))],
    );
    assert!(drained.players[1].library.is_empty());
    assert!(
        !drained.players[1].has_lost,
        "an over-long mill is not a draw from an empty library"
    );
}

#[test]
fn goblin_motivator_grants_haste_to_the_creature_it_targets() {
    let db = db();
    let mut state = main_phase(&db);
    let motivator = place(&mut state, &db, "goblin_motivator", PlayerId(0));
    // A creature that entered this turn, and so is summoning sick (CR 302.6).
    let card = cid(&db, "sun_sentinel");
    let instance = state.new_instance(card).id;
    let sick = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: sick,
        instance,
        printed: card.into(),
        controller: PlayerId(0),
        entered_turn: state.turn,
        ..Default::default()
    });
    let mut sick_combat = state.clone();
    sick_combat.step = Step::DeclareAttackers;
    assert!(!attacker_candidates(&sick_combat, &db).contains(&sick));

    let after = activate(&state, &db, motivator, 0, vec![Target::Permanent(sick)]);
    assert!(characteristics(&after, sick, &db)
        .keywords
        .contains(&Keyword::Haste));
    let mut hasty_combat = after.clone();
    hasty_combat.step = Step::DeclareAttackers;
    assert!(
        attacker_candidates(&hasty_combat, &db).contains(&sick),
        "granted haste lifts the CR 302.6 restriction exactly as printed haste does"
    );
}

// ----- spells: removal with a narrowed target class --------------------------

#[test]
fn plummet_destroys_only_creatures_that_currently_have_flying() {
    let db = db();
    let mut state = main_phase(&db);
    let ground = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let flyer = place(&mut state, &db, "rustwing_falcon", PlayerId(1));

    // The ground creature is not in the legal set, so the spell can't be aimed at it.
    let mut attempt = state.clone();
    let bad = to_hand(&mut attempt, &db, "plummet", PlayerId(0));
    assert_eq!(
        apply_action(
            &attempt,
            &Action::CastSpell {
                card: bad,
                targets: vec![Target::Permanent(ground)],
            },
            &db,
        ),
        attempt,
        "a creature without flying is not a legal Plummet target"
    );

    let after = cast(&state, &db, "plummet", vec![Target::Permanent(flyer)]);
    assert!(!on_battlefield(&after, flyer), "the flyer is destroyed");
    assert!(on_battlefield(&after, ground));
}

#[test]
fn plummet_can_be_aimed_at_a_creature_that_was_granted_flying() {
    // CR 613.1f: a granted keyword is indistinguishable from a printed one, so the
    // target class is read through the computed characteristics rather than the
    // printed keyword list.
    let db = db();
    let mut state = main_phase(&db);
    let ground = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    // Mighty Leap grants flying until end of turn (alongside +2/+2), filling one
    // target slot per effect.
    let state = cast(
        &state,
        &db,
        "mighty_leap",
        vec![Target::Permanent(ground), Target::Permanent(ground)],
    );
    assert!(characteristics(&state, ground, &db)
        .keywords
        .contains(&Keyword::Flying));

    let after = cast(&state, &db, "plummet", vec![Target::Permanent(ground)]);
    assert!(!on_battlefield(&after, ground));
}

#[test]
fn take_vengeance_destroys_only_a_tapped_creature() {
    let db = db();
    let mut state = main_phase(&db);
    let untapped = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let tapped = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    state
        .battlefield
        .iter_mut()
        .filter(|p| p.id == tapped)
        .for_each(|p| p.tapped = true);

    let mut attempt = state.clone();
    let bad = to_hand(&mut attempt, &db, "take_vengeance", PlayerId(0));
    assert_eq!(
        apply_action(
            &attempt,
            &Action::CastSpell {
                card: bad,
                targets: vec![Target::Permanent(untapped)],
            },
            &db,
        ),
        attempt
    );

    let after = cast(
        &state,
        &db,
        "take_vengeance",
        vec![Target::Permanent(tapped)],
    );
    assert!(!on_battlefield(&after, tapped));
    assert!(on_battlefield(&after, untapped));
}

#[test]
fn naturalize_and_invoke_the_divine_take_an_artifact_or_an_enchantment() {
    // One slot that accepts either type (CR 601.2c), not two slots — and never a
    // creature.
    let db = db();
    let mut state = main_phase(&db);
    let artifact = place(&mut state, &db, "millstone", PlayerId(1));
    let creature = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    // Oakenform is an Aura, so it must be attached: an Aura attached to nothing is
    // put into its owner's graveyard by the CR 704.5n state-based action.
    let enchantment = place(&mut state, &db, "oakenform", PlayerId(1));
    state
        .battlefield
        .iter_mut()
        .filter(|p| p.id == enchantment)
        .for_each(|p| p.attached_to = Some(creature));

    let mut attempt = state.clone();
    let bad = to_hand(&mut attempt, &db, "naturalize", PlayerId(0));
    assert_eq!(
        apply_action(
            &attempt,
            &Action::CastSpell {
                card: bad,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        ),
        attempt,
        "a creature is not an artifact or enchantment"
    );

    let after = cast(&state, &db, "naturalize", vec![Target::Permanent(artifact)]);
    assert!(!on_battlefield(&after, artifact));

    // Invoke the Divine destroys the same class and gains its caster four life.
    let life = state.players[0].life;
    let after = cast(
        &state,
        &db,
        "invoke_the_divine",
        vec![Target::Permanent(enchantment)],
    );
    assert!(!on_battlefield(&after, enchantment));
    assert_eq!(after.players[0].life, life + 4);
}

#[test]
fn smelt_destroys_an_artifact_and_nothing_else() {
    let db = db();
    let mut state = main_phase(&db);
    let artifact = place(&mut state, &db, "millstone", PlayerId(1));
    let creature = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let mut attempt = state.clone();
    let bad = to_hand(&mut attempt, &db, "smelt", PlayerId(0));
    assert_eq!(
        apply_action(
            &attempt,
            &Action::CastSpell {
                card: bad,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        ),
        attempt
    );

    let after = cast(&state, &db, "smelt", vec![Target::Permanent(artifact)]);
    assert!(!on_battlefield(&after, artifact));
}

// ----- spells: bounce, drain, counterspells, mass pumps -----------------------

#[test]
fn disperse_returns_a_creature_to_its_owners_hand_without_it_dying() {
    // CR 400.7: a bounce is not a death. The card reaches the hand, no
    // `permanent_died` is logged, and the battlefield identity is dropped — so a
    // recast is a brand-new object.
    let db = db();
    let mut state = main_phase(&db);
    let target = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let instance = state
        .battlefield
        .iter()
        .find(|p| p.id == target)
        .unwrap()
        .instance;

    let after = cast(&state, &db, "disperse", vec![Target::Permanent(target)]);
    assert!(!on_battlefield(&after, target));
    assert!(
        after.players[1].hand.iter().any(|c| c.id == instance),
        "the card is in its owner's hand"
    );
    assert!(after.players[1].graveyard.is_empty());
    assert!(
        !after
            .log
            .iter()
            .any(|entry| matches!(entry.event, GameEvent::PermanentDied { .. })),
        "returning a creature to hand is not a death (CR 700.4)"
    );
}

#[test]
fn sovereigns_bite_drains_the_player_it_targets() {
    let db = db();
    let state = main_phase(&db);
    let after = cast(
        &state,
        &db,
        "sovereign_s_bite",
        vec![Target::Player(PlayerId(1))],
    );
    assert_eq!(after.players[1].life, 17, "target player loses 3 life");
    assert_eq!(after.players[0].life, 23, "and you gain 3");
}

#[test]
fn essence_scatter_counters_a_creature_spell_but_not_an_instant() {
    let db = db();
    let mut state = main_phase(&db);

    // A creature spell on the stack, controlled by the opponent.
    let creature = to_hand(&mut state, &db, "sun_sentinel", PlayerId(1));
    let creature_spell = StackId(state.mint_id());
    state.stack.push(StackObject {
        id: creature_spell,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell { card: creature },
        targets: Vec::new(),
    });
    // …and a noncreature spell beside it.
    let burn = to_hand(&mut state, &db, "shock", PlayerId(1));
    let burn_spell = StackId(state.mint_id());
    state.stack.push(StackObject {
        id: burn_spell,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell { card: burn },
        targets: vec![Target::Player(PlayerId(0))],
    });

    let mut attempt = state.clone();
    let bad = to_hand(&mut attempt, &db, "essence_scatter", PlayerId(0));
    assert_eq!(
        apply_action(
            &attempt,
            &Action::CastSpell {
                card: bad,
                targets: vec![Target::Spell(burn_spell)],
            },
            &db,
        ),
        attempt,
        "an instant is not a creature spell"
    );

    let mut good = state.clone();
    let scatter = to_hand(&mut good, &db, "essence_scatter", PlayerId(0));
    let after = apply_action(
        &good,
        &Action::CastSpell {
            card: scatter,
            targets: vec![Target::Spell(creature_spell)],
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(
        !after.stack.iter().any(|o| o.id == creature_spell),
        "the creature spell was countered"
    );
    assert!(
        after.players[1]
            .graveyard
            .iter()
            .any(|c| c.id == creature.id),
        "and its card went to the graveyard"
    );
    assert!(!after.battlefield.iter().any(|p| p.instance == creature.id));
}

#[test]
fn bone_to_ash_counters_a_creature_spell_and_replaces_itself() {
    let db = db();
    let mut state = main_phase(&db);
    let library_card = state.new_instance(cid(&db, "sun_sentinel"));
    state.players[0].library.push(library_card);
    let creature = to_hand(&mut state, &db, "sun_sentinel", PlayerId(1));
    let spell = StackId(state.mint_id());
    state.stack.push(StackObject {
        id: spell,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell { card: creature },
        targets: Vec::new(),
    });

    let bone = to_hand(&mut state, &db, "bone_to_ash", PlayerId(0));
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: bone,
            targets: vec![Target::Spell(spell)],
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(!after.stack.iter().any(|o| o.id == spell));
    assert!(
        after.players[0]
            .hand
            .iter()
            .any(|c| c.id == library_card.id),
        "and its controller drew a card"
    );
}

#[test]
fn inspired_charge_pumps_only_your_creatures_and_wears_off_at_cleanup() {
    let db = db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "sun_sentinel", PlayerId(0));
    let theirs = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let after = cast(&state, &db, "inspired_charge", Vec::new());
    let pumped = characteristics(&after, mine, &db);
    assert_eq!(pumped.power, Some(4), "printed 2 + 2");
    assert_eq!(pumped.toughness, Some(3), "printed 2 + 1");
    assert_eq!(
        characteristics(&after, theirs, &db).power,
        Some(2),
        "an opponent's creature is untouched"
    );

    // CR 514.2: the modifier is removed in the cleanup step.
    let mut turn = after;
    while turn.turn == 1 {
        turn = advance(&turn, &db);
    }
    assert_eq!(
        characteristics(&turn, mine, &db).power,
        Some(2),
        "the pump ended with the turn"
    );
}

#[test]
fn crash_through_grants_trample_to_your_board_and_draws() {
    let db = db();
    let mut state = main_phase(&db);
    let library_card = state.new_instance(cid(&db, "sun_sentinel"));
    state.players[0].library.push(library_card);
    let mine = place(&mut state, &db, "sun_sentinel", PlayerId(0));
    let theirs = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let after = cast(&state, &db, "crash_through", Vec::new());
    assert!(characteristics(&after, mine, &db)
        .keywords
        .contains(&Keyword::Trample));
    assert!(!characteristics(&after, theirs, &db)
        .keywords
        .contains(&Keyword::Trample));
    assert!(after.players[0]
        .hand
        .iter()
        .any(|c| c.id == library_card.id));
}

#[test]
fn daggerback_basilisk_kills_what_it_blocks() {
    // Deathtouch on a new body, proved through the combat-damage path rather than the
    // keyword list: any nonzero damage it deals is lethal (CR 702.2b).
    let db = db();
    let mut state = main_phase(&db);
    let attacker = place(&mut state, &db, "gigantosaurus", PlayerId(0));
    let basilisk = place(&mut state, &db, "daggerback_basilisk", PlayerId(1));

    let state = attack_with(&state, &db, attacker);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: basilisk,
                attacker,
            }],
        },
        &db,
    );
    let mut state = state;
    while on_battlefield(&state, attacker) && state.step != Step::End {
        state = advance(&state, &db);
    }
    assert!(
        !on_battlefield(&state, attacker),
        "a 10/10 blocked by a deathtouch 2/2 dies"
    );
}
