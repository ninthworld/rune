//! Tokens: a battlefield object with no card (issue #605, CR 111).
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! A token that parses is not evidence of anything: what has to be true is that a
//! created token is an ordinary permanent everywhere the rules touch a permanent, and
//! that it ceases to exist the instant it would go anywhere but the battlefield
//! (CR 111.7) — including the half of that rule which is easy to miss, that it does
//! not arrive in a graveyard even though its death is real.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attacker_candidates, blocker_candidates_for, characteristics, valid_actions,
    Action, Attack, Block, CardDatabase, CardId, CardInstance, CardType, Color, FunctionalId,
    GameEvent, GameState, Keyword, LoggedIdentity, Permanent, PermanentId, PlayerId, Printed, Step,
    Target, TokenData,
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
/// payability never decides a test that is about an effect.
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

/// The 2/2 black Zombie token Doomed Dissenter leaves behind — the hand-built stand-in
/// for "some effect created this", used where a test is about the object rather than
/// about the card that made it.
fn zombie() -> TokenData {
    TokenData {
        name: "Zombie".to_string(),
        types: vec![CardType::Creature],
        subtypes: vec!["Zombie".to_string()],
        colors: vec![Color::Black],
        power: Some(2),
        toughness: Some(2),
        ..TokenData::default()
    }
}

/// Put a token onto the battlefield under `controller`, free of summoning sickness,
/// and return its battlefield identity.
fn place_token(state: &mut GameState, token: TokenData, controller: PlayerId) -> PermanentId {
    let id = PermanentId(state.mint_id());
    let instance = state.mint_id();
    state.battlefield.push(Permanent {
        id,
        instance: sage_engine::CardInstanceId(instance),
        printed: Printed::Token(Box::new(token)),
        controller,
        ..Default::default()
    });
    id
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

/// Cast `slug` from player 0's hand with `targets` and let it (and anything it puts on
/// the stack) resolve.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            targets: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets,
        },
        db,
    );
    while !state.stack.is_empty() {
        state = apply_action(&state, &Action::PassPriority, db);
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

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

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

/// Every card in every graveyard, hand, and exile pile, across both seats — the zones
/// CR 111.7 says a token may never reach.
fn cards_outside_the_battlefield(state: &GameState) -> usize {
    state
        .players
        .iter()
        .map(|p| p.graveyard.len() + p.hand.len() + p.exile.len())
        .sum()
}

// ----- the object model -----------------------------------------------------

/// A created token is a permanent in every sense the rules use the word: it is on the
/// battlefield with its own computed characteristics, it can attack and be blocked, it
/// takes damage, and it dies to lethal damage — all through the ordinary pipeline,
/// with no arm anywhere that says "unless it is a token".
#[test]
fn issue_605_a_token_attacks_blocks_takes_damage_and_dies() {
    let db = db();
    let mut state = main_phase();
    let token = place_token(&mut state, zombie(), PlayerId(0));

    // Its characteristics are computed exactly as a card permanent's are.
    let current = characteristics(&state, token, &db);
    assert_eq!(current.power, Some(2));
    assert_eq!(current.toughness, Some(2));
    assert_eq!(current.types, vec![CardType::Creature]);
    assert_eq!(current.subtypes, vec!["Zombie".to_string()]);

    // It can be declared as an attacker.
    state.step = Step::DeclareAttackers;
    assert!(
        attacker_candidates(&state, &db).contains(&token),
        "a token creature attacks like any other"
    );
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: token,
                defender: PlayerId(1),
            }],
        },
        &db,
    );
    let mut state = state;
    while state.step != Step::DeclareBlockers {
        state = advance(&state, &db);
    }

    // And it can be blocked — here by a 2/4, which kills it and survives.
    let blocker = place(&mut state, &db, "giant_spider", PlayerId(1));
    state.players[1].turn_began = state.turn;
    assert!(
        blocker_candidates_for(&state, PlayerId(1), &db).contains(&blocker),
        "a token attacker can be blocked like any other"
    );
    let mut state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker,
                attacker: token,
            }],
        },
        &db,
    );
    while on_battlefield(&state, token) && state.step != Step::End {
        state = advance(&state, &db);
    }

    assert!(
        !on_battlefield(&state, token),
        "a 2/2 token blocked by a 2/4 takes lethal damage and dies"
    );
    assert_eq!(
        state.players[1].life, 20,
        "a blocked attacker deals no damage to the defending player"
    );
    // The blocker took the token's 2 damage and lived — the token dealt combat damage
    // like any other creature.
    let ogre = state
        .battlefield
        .iter()
        .find(|p| p.id == blocker)
        .expect("the blocker survived");
    assert_eq!(ogre.damage, 2, "the token dealt its power in combat damage");
}

/// CR 111.7: a token that dies goes **nowhere**. The death is real — the log records
/// it, and a dies trigger watching the board fires — but no graveyard receives it,
/// which is the half of the rule that is easy to leave half-implemented.
#[test]
fn issue_605_a_token_that_dies_reaches_no_graveyard() {
    let db = db();
    let mut state = main_phase();
    let token = place_token(&mut state, zombie(), PlayerId(0));
    let before = cards_outside_the_battlefield(&state);

    // Kill it with a burn spell through the real pipeline.
    let state = cast(&state, &db, "electrify", vec![Target::Permanent(token)]);

    assert!(!on_battlefield(&state, token), "4 damage kills a 2/2 token");
    assert_eq!(
        cards_outside_the_battlefield(&state),
        before + 1,
        "only the sorcery itself moved to a graveyard; the token went nowhere"
    );
    assert!(
        state.players.iter().all(|p| p
            .graveyard
            .iter()
            .all(|c| c.card != CardId::default() || c.card == cid(&db, "electrify"))),
        "no token card appears in a graveyard"
    );

    // The death itself is recorded, named by what the token was — the one thing that
    // outlives it (CR 111.3).
    let died = state
        .log
        .iter()
        .find_map(|entry| match &entry.event {
            GameEvent::PermanentDied { permanent } if permanent.permanent == token => {
                Some(permanent.identity.clone())
            }
            _ => None,
        })
        .expect("the token's death is logged");
    assert_eq!(died, LoggedIdentity::Token("Zombie".to_string()));
}

/// CR 603.6c: a dies trigger fires on the way out, before the object ceases to exist.
/// A token dying is observed by a death-watcher exactly as a card dying is — which the
/// diff-based collector cannot answer from the graveyard, because the token never
/// reaches one.
#[test]
fn issue_605_a_dies_trigger_sees_a_token_die() {
    let db = db();
    let mut state = main_phase();
    // Poison-Tip Archer: whenever another creature dies, each opponent loses 1 life.
    let _archer = place(&mut state, &db, "poison_tip_archer", PlayerId(0));
    let token = place_token(&mut state, zombie(), PlayerId(0));
    let opponent_life = state.players[1].life;

    let state = cast(&state, &db, "electrify", vec![Target::Permanent(token)]);

    assert!(!on_battlefield(&state, token));
    assert_eq!(
        state.players[1].life,
        opponent_life - 1,
        "the death-watcher triggered on the token's death (CR 603.6c)"
    );
}

/// CR 111.7 again, from the other two exits: a token returned to a hand or exiled
/// ceases to exist instead of arriving there. Neither is a death, so neither fires a
/// dies trigger — the token simply stops being.
///
/// The bounce is a bundled card; the exile is an inline definition (ADR 0009), because
/// no card in the shipped catalog exiles a permanent.
#[test]
fn issue_605_a_bounced_or_exiled_token_ceases_to_exist() {
    let bundled = db();
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_banish","name":"Test Banish",
             "types":["sorcery"],"mana_cost":"{1}","colors":[],
             "spell_effects":[{"kind":"exile","target":"any_creature"}]}]"#,
    )
    .expect("an inline definition");

    for (db, slug, exit) in [
        (&bundled, "disperse", "hand"),
        (&inline, "test_banish", "exile"),
    ] {
        let mut state = main_phase();
        let token = place_token(&mut state, zombie(), PlayerId(0));
        let before = cards_outside_the_battlefield(&state);

        let state = cast(&state, db, slug, vec![Target::Permanent(token)]);

        assert!(
            !on_battlefield(&state, token),
            "{slug} removed the token from the battlefield"
        );
        assert_eq!(
            cards_outside_the_battlefield(&state),
            before + 1,
            "only {slug} itself moved zones; the token did not arrive in the {exit}"
        );
        assert!(
            state
                .players
                .iter()
                .all(|p| p.hand.is_empty() && p.exile.is_empty()),
            "neither the hand nor exile received the token"
        );
        // Ceasing to exist is not dying: nothing is logged as a death.
        assert!(
            !state
                .log
                .iter()
                .any(|entry| matches!(&entry.event, GameEvent::PermanentDied { .. })),
            "a token that leaves for a hand or exile has not died (CR 700.4)"
        );
    }
}

/// A token created under another player's control is theirs: the creator is a
/// [`PlayerRef`](sage_engine::PlayerRef), not an assumption about the ability's
/// controller. Exercised inline (ADR 0009) because no bundled M19 card says it.
#[test]
fn issue_605_a_token_can_be_created_under_another_players_control() {
    let json = r#"[{"schema_version":1,"functional_id":"test_gift","name":"Test Gift",
        "types":["sorcery"],"mana_cost":"{1}","colors":[],
        "spell_effects":[{"kind":"create_token","player_ref":"each_opponent","tapped":true,
          "token":{"name":"Ox","types":["creature"],"subtypes":["Ox"],"colors":["white"],
                   "power":2,"toughness":4}}]}]"#;
    let db = CardDatabase::from_json(json).expect("an inline definition");
    let state = main_phase();
    let state = cast(&state, &db, "test_gift", Vec::new());

    let ox = state
        .battlefield
        .iter()
        .find(|p| p.printed.is_token())
        .expect("the token was created");
    assert_eq!(
        ox.controller,
        PlayerId(1),
        "the opponent creates it, so the opponent controls it"
    );
    assert!(ox.tapped, "the effect said tapped, so it entered tapped");
    assert_eq!(characteristics(&state, ox.id, &db).power, Some(2));
}

// ----- the cards ------------------------------------------------------------

/// Goblin Instigator's enters-the-battlefield trigger creates its Goblin, through the
/// ordinary cast → resolve → trigger pipeline.
#[test]
fn issue_605_goblin_instigator_brings_a_goblin_with_it() {
    let db = db();
    let state = main_phase();
    let state = cast(&state, &db, "goblin_instigator", Vec::new());

    let tokens: Vec<&Permanent> = state
        .battlefield
        .iter()
        .filter(|p| p.printed.is_token())
        .collect();
    assert_eq!(tokens.len(), 1, "one Goblin token");
    let goblin = tokens[0];
    assert_eq!(goblin.controller, PlayerId(0));
    let current = characteristics(&state, goblin.id, &db);
    assert_eq!((current.power, current.toughness), (Some(1), Some(1)));
    assert_eq!(current.subtypes, vec!["Goblin".to_string()]);
    assert!(!goblin.tapped, "it enters untapped");
}

/// Aviation Pioneer's Thopter is a colourless **artifact** creature with flying — a
/// token whose types and keywords are not the ones its creator has.
#[test]
fn issue_605_aviation_pioneer_makes_a_flying_artifact_thopter() {
    let db = db();
    let state = main_phase();
    let state = cast(&state, &db, "aviation_pioneer", Vec::new());

    let thopter = state
        .battlefield
        .iter()
        .find(|p| p.printed.is_token())
        .expect("the Thopter was created");
    let current = characteristics(&state, thopter.id, &db);
    assert!(current.types.contains(&CardType::Artifact));
    assert!(current.types.contains(&CardType::Creature));
    assert!(current.keywords.contains(&Keyword::Flying));
    assert_eq!((current.power, current.toughness), (Some(1), Some(1)));
}

/// Doomed Dissenter's Zombie arrives when the Dissenter dies — the death seam creating
/// a token, rather than the entry seam.
#[test]
fn issue_605_doomed_dissenter_leaves_a_zombie_behind() {
    let db = db();
    let mut state = main_phase();
    let dissenter = place(&mut state, &db, "doomed_dissenter", PlayerId(0));

    let state = cast(&state, &db, "electrify", vec![Target::Permanent(dissenter)]);

    assert!(!on_battlefield(&state, dissenter), "the 1/1 died");
    let zombie = state
        .battlefield
        .iter()
        .find(|p| p.printed.is_token())
        .expect("the Zombie was created");
    let current = characteristics(&state, zombie.id, &db);
    assert_eq!((current.power, current.toughness), (Some(2), Some(2)));
    assert_eq!(current.subtypes, vec!["Zombie".to_string()]);
    // The Dissenter itself is a card, so it *does* reach a graveyard.
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|c| c.card == cid(&db, "doomed_dissenter")),
        "the card that died is in its graveyard; only tokens go nowhere"
    );
}

/// Heroic Reinforcements creates two tokens and then pumps the team — the two tokens
/// are separate objects, and both are included in the mass effect that follows them in
/// the same resolution.
#[test]
fn issue_605_heroic_reinforcements_makes_two_soldiers_and_pumps_them() {
    let db = db();
    let state = main_phase();
    let state = cast(&state, &db, "heroic_reinforcements", Vec::new());

    let soldiers: Vec<&Permanent> = state
        .battlefield
        .iter()
        .filter(|p| p.printed.is_token())
        .collect();
    assert_eq!(soldiers.len(), 2, "two Soldier tokens, two objects");
    assert_ne!(
        soldiers[0].id, soldiers[1].id,
        "each token is its own permanent with its own id"
    );
    for soldier in soldiers {
        let current = characteristics(&state, soldier.id, &db);
        assert_eq!(
            (current.power, current.toughness),
            (Some(2), Some(2)),
            "1/1 plus the spell's own +1/+1 until end of turn"
        );
        assert!(
            current.keywords.contains(&Keyword::Haste),
            "the tokens gained haste with the rest of the team"
        );
    }
}

/// Gallant Cavalry and Knightly Valor both make the same 2/2 vigilant Knight, from a
/// creature's entry and from an Aura's.
#[test]
fn issue_605_gallant_cavalry_and_knightly_valor_both_make_a_vigilant_knight() {
    let db = db();

    let state = cast(&main_phase(), &db, "gallant_cavalry", Vec::new());
    let knight = state
        .battlefield
        .iter()
        .find(|p| p.printed.is_token())
        .expect("Gallant Cavalry's Knight");
    let current = characteristics(&state, knight.id, &db);
    assert_eq!((current.power, current.toughness), (Some(2), Some(2)));
    assert!(current.keywords.contains(&Keyword::Vigilance));

    // The Aura needs a host, and grants its own +2/+2 and vigilance on top.
    let mut state = main_phase();
    let host = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let state = cast(&state, &db, "knightly_valor", vec![Target::Permanent(host)]);
    let knight = state
        .battlefield
        .iter()
        .find(|p| p.printed.is_token())
        .expect("Knightly Valor's Knight");
    assert_eq!(
        characteristics(&state, knight.id, &db).power,
        Some(2),
        "the token is the Aura's, not the host's"
    );
    let host_now = characteristics(&state, host, &db);
    assert_eq!(
        (host_now.power, host_now.toughness),
        (Some(6), Some(4)),
        "the enchanted 4/2 is a 6/4"
    );
    assert!(host_now.keywords.contains(&Keyword::Vigilance));
}

// ----- the honesty artifact -------------------------------------------------

/// A token is not a card, so it must never reach the compatibility report — the
/// project's central honesty artifact, which would otherwise claim support for a card
/// that does not exist.
///
/// The structural guarantee is that [`TokenData`] has no `functional_id` to put there;
/// this asserts the consequence, with tokens actually on the battlefield.
#[test]
fn issue_605_tokens_never_reach_the_compatibility_report() {
    let db = db();
    let mut state = main_phase();
    place_token(&mut state, zombie(), PlayerId(0));
    let state = cast(&state, &db, "goblin_instigator", Vec::new());
    assert!(
        state
            .battlefield
            .iter()
            .filter(|p| p.printed.is_token())
            .count()
            >= 2,
        "the board really does hold tokens while the report is generated"
    );

    let report = sage_engine::compat::render_report(
        &db,
        &sage_engine::compat::bundled_exclusions().expect("the exclusion list parses"),
    )
    .expect("the report renders");

    for name in ["| Zombie |", "| Goblin |"] {
        assert!(
            !report.contains(name),
            "the report names a token ({name}); tokens are not cards and are not supported cards"
        );
    }
    // The cards that *create* tokens are supported, and say so.
    assert!(report.contains("goblin_instigator"));
    assert!(report.contains("| Goblin Instigator |"));
    // And the exclusion the object model retired is gone.
    assert!(
        !report.contains("no token object model"),
        "the token-creation exclusion was removed with the model that lifted it"
    );
}
