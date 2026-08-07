//! The bundled cards whose behavior no test drove end to end (issue #774).
//!
//! Every effect these cards are built from is covered somewhere. What was not covered is
//! the **composition**: a spell that damages a target and then looks at five cards, an
//! artifact that gains life on the way in and draws on the way out, a creature that taps
//! for a colour it has to be asked about. Those seams — ordering, a mid-resolution
//! question in the middle of a spell, a cost that sacrifices the source that is paying it
//! — are where an individually-correct effect list still adds up to the wrong card.
//!
//! The audit that produced this list is generated (`docs/generated/test-coverage.md`) and
//! is a review aid, not a gate: naming a card in a test proves nothing on its own, which
//! is why every test here drives the real [`apply_action`] pipeline and asserts a state
//! transition.
//!
//! **What none of this can catch** is a card authored wrong from the start — a 3/1 typed
//! as a 3/2 is a definition every test agrees with. Only the printed card answers that,
//! and the printed card is not something this repository ships (see the project's
//! no-Oracle-text rule); each definition below was checked against real set data by hand
//! when this file was written.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, choice_candidates, pending_player_choice, valid_actions, Action,
    CardDatabase, CardId, CardInstance, Color, FunctionalId, GameState, Keyword, Permanent,
    PermanentId, PlayerId, Step, Target,
};

/// A permanent's keywords as the layer system computes them — the same read combat makes.
fn keywords(state: &GameState, db: &CardDatabase, id: PermanentId) -> Vec<Keyword> {
    characteristics(state, id, db).keywords
}

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game in a main phase with mana to spend and a library to draw from.
fn main_phase(db: &CardDatabase) -> GameState {
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
    let forest = cid(db, "forest");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(forest)).collect();
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

/// Pass priority until nothing is left to resolve — or until the game stops to ask a
/// question, which is where a test that is about the question takes over.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..40 {
        if pending_player_choice(&state).is_some() || state.stack.is_empty() {
            return state;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    panic!("the stack never emptied");
}

/// Cast `card` with `targets` and let it — and anything it triggers — resolve.
fn cast(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    targets: Vec<Target>,
) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    settle(&state, db)
}

/// Activate ability `index` of `permanent` and let it resolve.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> GameState {
    let state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    settle(&state, db)
}

fn in_graveyard(state: &GameState, seat: PlayerId, card: CardId) -> bool {
    state.players[seat.0]
        .graveyard
        .iter()
        .any(|instance| instance.card == card)
}

/// Befuddle: `-4/-0` and a draw, in that order and both for real.
#[test]
fn issue_774_befuddle_shrinks_a_creature_and_draws() {
    let db = db();
    let mut state = main_phase(&db);
    // A 6/5, so a shrink of four still leaves a positive power to compare.
    let victim = place(&mut state, &db, "bogstomper", PlayerId(1));
    let spell = to_hand(&mut state, &db, "befuddle", PlayerId(0));
    let before = characteristics(&state, victim, &db);
    let hand = state.players[0].hand.len();

    let state = cast(&state, &db, spell, vec![Target::Permanent(victim)]);

    let after = characteristics(&state, victim, &db);
    assert_eq!(after.power, before.power.map(|p| p - 4), "-4 power");
    assert_eq!(after.toughness, before.toughness, "and -0 toughness");
    assert_eq!(
        state.players[0].hand.len(),
        hand - 1 + 1,
        "the Befuddle left the hand and a card replaced it"
    );
}

/// Diregraf Ghoul enters **tapped** (CR 614.12) — a replacement on the way in, not a tap
/// applied afterwards, which is the difference a summoning-sick creature cannot show.
#[test]
fn issue_774_diregraf_ghoul_arrives_tapped() {
    let db = db();
    let mut state = main_phase(&db);
    let ghoul = to_hand(&mut state, &db, "diregraf_ghoul", PlayerId(0));

    let state = cast(&state, &db, ghoul, Vec::new());

    let perm = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("the Ghoul arrived");
    assert!(perm.tapped, "it enters tapped");
    let stats = characteristics(&state, perm.id, &db);
    assert_eq!((stats.power, stats.toughness), (Some(2), Some(2)));
}

/// Draconic Disciple's mana ability asks which colour, and the answer is what reaches the
/// pool — the smallest card that composes an activation with a mid-resolution question.
#[test]
fn issue_774_draconic_disciple_taps_for_a_colour_it_is_asked_about() {
    let db = db();
    let mut state = main_phase(&db);
    let disciple = place(&mut state, &db, "draconic_disciple", PlayerId(0));
    state.players[0].turn_began = state.turn;
    let before = state.players[0].mana_pool.white;

    // A mana ability does not use the stack (CR 605.3a): activating it is the whole
    // resolution, and the colour question is posed immediately.
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: disciple,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let pending = pending_player_choice(&state).expect("which colour?");
    assert!(pending.question.color().is_some());
    let state = apply_action(
        &state,
        &Action::AnswerColor {
            color: Color::White,
        },
        &db,
    );

    assert_eq!(
        state.players[0].mana_pool.white,
        before + 1,
        "one mana of the colour named, and no other"
    );
}

/// Its second ability pays with itself: `{7}`, `{T}`, sacrifice — and the Dragon it makes
/// is a 5/5 flier, which is a characteristic of the token rather than of the card.
#[test]
fn issue_774_draconic_disciple_sacrifices_itself_for_a_dragon() {
    let db = db();
    let mut state = main_phase(&db);
    let disciple = place(&mut state, &db, "draconic_disciple", PlayerId(0));
    state.players[0].turn_began = state.turn;

    let state = activate(&state, &db, disciple, 1);

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == disciple),
        "the source paid for its own ability"
    );
    assert!(in_graveyard(
        &state,
        PlayerId(0),
        cid(&db, "draconic_disciple")
    ));
    let dragon = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("a Dragon token");
    let stats = characteristics(&state, dragon.id, &db);
    assert_eq!((stats.power, stats.toughness), (Some(5), Some(5)));
    assert!(
        keywords(&state, &db, dragon.id).contains(&Keyword::Flying),
        "with flying"
    );
    assert!(dragon.printed.is_token(), "and it is a token, not a card");
}

/// Horizon Scholar: flying, and an enters trigger that scries — a *look* posed by a
/// creature arriving rather than by a spell resolving.
#[test]
fn issue_774_horizon_scholar_flies_and_scries_two() {
    let db = db();
    let mut state = main_phase(&db);
    let scholar = to_hand(&mut state, &db, "horizon_scholar", PlayerId(0));
    let top_two: Vec<_> = state.players[0]
        .library
        .iter()
        .rev()
        .take(2)
        .map(|card| card.id)
        .collect();

    let state = cast(&state, &db, scholar, Vec::new());

    let perm = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("the Scholar arrived");
    assert!(keywords(&state, &db, perm.id).contains(&Keyword::Flying));

    let pending = pending_player_choice(&state).expect("the scry asks");
    assert_eq!(pending.chooser, PlayerId(0));
    // Bottom the top card, keep the other: a scry's answer is what goes to the bottom.
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![top_two[0]],
        },
        &db,
    );

    let library = &state.players[0].library;
    assert_eq!(
        library.first().map(|card| card.id),
        Some(top_two[0]),
        "the one chosen went to the bottom"
    );
    assert_eq!(
        library.last().map(|card| card.id),
        Some(top_two[1]),
        "and the one kept is still on top"
    );
}

/// Pendulum of Patterns is two halves of one card: three life on the way in, a card on
/// the way out, and the way out eats the artifact.
#[test]
fn issue_774_pendulum_of_patterns_gains_then_draws_and_is_gone() {
    let db = db();
    let mut state = main_phase(&db);
    let pendulum = to_hand(&mut state, &db, "pendulum_of_patterns", PlayerId(0));
    let life = state.players[0].life;

    let state = cast(&state, &db, pendulum, Vec::new());
    assert_eq!(state.players[0].life, life + 3, "three life as it enters");

    let perm = state
        .battlefield
        .iter()
        .find(|perm| perm.controller == PlayerId(0))
        .expect("the Pendulum arrived")
        .id;
    let hand = state.players[0].hand.len();
    let library = state.players[0].library.len();
    // Index into *all* the card's abilities: the enters trigger is ability 0, so the
    // activation is ability 1.
    let state = activate(&state, &db, perm, 1);

    assert_eq!(state.players[0].hand.len(), hand + 1, "a card drawn");
    assert_eq!(state.players[0].library.len(), library - 1, "off the top");
    assert!(
        !state.battlefield.iter().any(|p| p.id == perm),
        "and the Pendulum sacrificed itself to do it"
    );
    assert!(in_graveyard(
        &state,
        PlayerId(0),
        cid(&db, "pendulum_of_patterns")
    ));
}

/// Sarkhan's Dragonfire: damage first, then a look that may only take a **red** card.
/// Two effects, one resolution, and the second one asks a question mid-way.
#[test]
fn issue_774_sarkhans_dragonfire_burns_then_digs_for_red() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "bogstomper", PlayerId(1));
    let spell = to_hand(&mut state, &db, "sarkhan_s_dragonfire", PlayerId(0));
    // A red card fourth from the top, under three Forests: the filter has to pass over
    // what it is not allowed to take.
    let shock = state.new_instance(cid(&db, "shock"));
    let depth = state.players[0].library.len() - 4;
    state.players[0].library.insert(depth, shock);

    let state = cast(&state, &db, spell, vec![Target::Permanent(victim)]);

    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == victim)
            .expect("a 6/5 survives 3 damage")
            .damage,
        3,
        "three damage to the chosen target"
    );

    let pending = pending_player_choice(&state).expect("the look asks");
    let request = pending.question.cards().expect("a card choice");
    let offered = choice_candidates(&state, request, &db);
    assert_eq!(
        offered.len(),
        1,
        "only the red card among the five may be taken"
    );
    assert_eq!(offered[0].id, shock.id);

    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![shock.id],
        },
        &db,
    );
    assert!(
        state.players[0].hand.iter().any(|card| card.id == shock.id),
        "the red card went to hand"
    );
    assert_eq!(
        state.players[0].library.len(),
        12,
        "twelve Forests and a Shock, less the Shock that was taken"
    );
}

/// The spell is advertised **once**, in its empty requirement form (ADR 0004): the target
/// selection rides on the action a player sends back rather than being pre-expanded into
/// one offer per legal aim. Worth pinning on a card that both targets *and* asks a
/// question later — the two are separate mechanisms and neither collapses into the other.
#[test]
fn issue_774_sarkhans_dragonfire_is_offered_once_and_aimed_by_the_answer() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "bogstomper", PlayerId(1));
    let spell = to_hand(&mut state, &db, "sarkhan_s_dragonfire", PlayerId(0));

    let offers: Vec<Action> = valid_actions(&state, &db)
        .into_iter()
        .filter(|action| matches!(action, Action::CastSpell { card, .. } if card.id == spell.id))
        .collect();
    assert_eq!(
        offers.len(),
        1,
        "one offer for the card, not one per target"
    );
    assert!(
        matches!(&offers[0], Action::CastSpell { targets, .. } if targets.is_empty()),
        "and it carries no aim: the answer does"
    );

    // The aim is validated when it arrives, against the freshly derived legal set.
    let aimed = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(victim)],
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(aimed.stack.len(), 1, "the spell is on the stack, aimed");
}

/// Hostile Minotaur is a keyword and nothing else, and the keyword is the one that has a
/// state transition of its own: it can attack the turn it arrives (CR 302.6).
#[test]
fn issue_774_hostile_minotaur_attacks_the_turn_it_arrives() {
    let db = db();
    let mut state = main_phase(&db);
    state.players[0].turn_began = state.turn;
    let hasty = place(&mut state, &db, "hostile_minotaur", PlayerId(0));
    let ordinary = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    // Both arrived this turn: the only difference between them is the keyword.
    for perm in &mut state.battlefield {
        perm.entered_turn = state.turn;
    }
    state.step = Step::DeclareAttackers;

    let candidates = sage_engine::attacker_candidates(&state, &db);
    assert!(candidates.contains(&hasty), "haste attacks immediately");
    assert!(
        !candidates.contains(&ordinary),
        "and a creature without it is summoning-sick"
    );
    assert_eq!(
        keywords(&state, &db, hasty),
        vec![Keyword::Haste],
        "the one keyword it prints, and no other"
    );
}

/// The vanilla cards, as **authored characteristics**: cost, types, power, toughness, and
/// the one keyword each carries.
///
/// The weakest tier of the three the coverage report distinguishes, and deliberately so:
/// a creature with no ability has no state transition of its own to drive, and a test
/// that cast one would be testing the casting pipeline instead. What this does catch is a
/// *silent edit* — a definition changing under a card nobody is watching.
#[test]
fn issue_774_the_vanilla_definitions_are_what_they_are_printed_as() {
    let db = db();
    /// One vanilla definition: identity, cost, power, toughness, subtypes, and the one
    /// keyword it prints (if any).
    struct Printed {
        slug: &'static str,
        cost: &'static str,
        power: i32,
        toughness: i32,
        subtypes: &'static [&'static str],
        keyword: Option<Keyword>,
    }
    let expected = &[
        Printed {
            slug: "havoc_devils",
            cost: "{2}{R}{R}",
            power: 4,
            toughness: 3,
            subtypes: &["Devil"],
            keyword: Some(Keyword::Trample),
        },
        Printed {
            slug: "loxodon_line_breaker",
            cost: "{2}{W}",
            power: 3,
            toughness: 2,
            subtypes: &["Elephant", "Soldier"],
            keyword: None,
        },
        Printed {
            slug: "oreskos_swiftclaw",
            cost: "{1}{W}",
            power: 3,
            toughness: 1,
            subtypes: &["Cat", "Warrior"],
            keyword: None,
        },
        Printed {
            slug: "vigilant_baloth",
            cost: "{3}{G}{G}",
            power: 5,
            toughness: 5,
            subtypes: &["Beast"],
            keyword: Some(Keyword::Vigilance),
        },
    ];
    let mut state = main_phase(&db);
    for card in expected {
        let slug = card.slug;
        let data = db.card(cid(&db, slug)).expect("a bundled card");
        assert_eq!(data.mana_cost, card.cost, "{slug} mana cost");
        assert_eq!(data.power, Some(card.power), "{slug} power");
        assert_eq!(data.toughness, Some(card.toughness), "{slug} toughness");
        assert_eq!(
            data.subtypes,
            card.subtypes
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>(),
            "{slug} subtypes"
        );
        // Read through the same computed path combat reads, so a keyword that is printed
        // but never folded in would still fail here.
        let id = place(&mut state, &db, slug, PlayerId(0));
        for candidate in [Keyword::Trample, Keyword::Vigilance, Keyword::Flying] {
            let has = keywords(&state, &db, id).contains(&candidate);
            assert_eq!(
                has,
                card.keyword == Some(candidate),
                "{slug} and {candidate:?}"
            );
        }
    }
}
