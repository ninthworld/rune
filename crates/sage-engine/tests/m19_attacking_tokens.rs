//! Tokens created **attacking** (CR 506.3c, issue #734).
//!
//! "Create two tokens that are tapped and attacking" is two facts that pull in opposite
//! directions, and every test here is about keeping both. The tokens really are in the
//! combat already in progress — they attack what their creator attacks, a defender may
//! block them, and they deal combat damage this turn. And they were never *declared*, so
//! an ability that watches for a creature attacking does not see them arrive.
//!
//! Every test drives the real [`apply_action`] pipeline. Cards are named by their
//! authored `functional_id`, never by an interned handle (ADR 0008 §3); the cases no
//! bundled card expresses are driven from an inline definition (ADR 0009).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attack_target_of, characteristics, valid_actions, Ability, Action, Attack,
    AttackTarget, CardDatabase, CardId, CardInstance, CardInstanceId, CardType, Color, Effect,
    FunctionalId, GameState, Keyword, Permanent, PermanentId, PlayerId, PlayerRef, Printed, Step,
    Target, TokenData, TriggerCondition,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with both pools stocked so payability
/// never decides a test that is about an effect.
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

/// Put a hand-built token permanent onto the battlefield under `controller` — an object
/// some earlier effect created, used where the test is about what the token *does* rather
/// than about what made it.
fn place_token(state: &mut GameState, token: TokenData, controller: PlayerId) -> PermanentId {
    let id = PermanentId(state.mint_id());
    let instance = CardInstanceId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: Printed::Token(Box::new(token)),
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

/// Take whatever the pipeline offers, preferring a pass — the same one-step walk the
/// token suite uses, so no test hand-writes a turn structure.
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

/// Walk the pipeline until `step` is reached (or the game ends).
fn advance_to(state: &GameState, db: &CardDatabase, step: Step) -> GameState {
    let mut state = state.clone();
    for _ in 0..200 {
        if state.step == step || state.result().is_some() {
            return state;
        }
        state = advance(&state, db);
    }
    panic!("the pipeline never reached {step:?}");
}

/// Resolve everything on the stack, leaving priority where the pipeline puts it.
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

/// Declare `attacker` as an attacker against player 1, in the declare-attackers step.
fn declare_attack(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let mut state = advance_to(state, db, Step::DeclareAttackers);
    state.priority = PlayerId(0);
    assert!(
        sage_engine::attacker_candidates(&state, db).contains(&attacker),
        "the creature was offered as an attacker"
    );
    apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    )
}

/// Every token on the battlefield, in battlefield order.
fn tokens(state: &GameState) -> Vec<&Permanent> {
    state
        .battlefield
        .iter()
        .filter(|p| p.printed.is_token())
        .collect()
}

// ----- the card -------------------------------------------------------------

/// Leonin Warleader's Cats arrive in the attack the Warleader declared: attacking the
/// same player, tapped as the card says, and with the lifelink the token is printed with.
#[test]
fn issue_734_leonin_warleader_s_cats_join_the_attack_it_declared() {
    let db = db();
    let mut state = main_phase();
    let warleader = place(&mut state, &db, "leonin_warleader", PlayerId(0));

    let state = declare_attack(&state, &db, warleader);
    assert_eq!(state.stack.len(), 1, "the attack trigger is on the stack");
    let state = resolve_stack(&state, &db);

    let cats = tokens(&state);
    assert_eq!(cats.len(), 2, "two Cat tokens, two objects");
    assert_ne!(cats[0].id, cats[1].id);
    let defender = attack_target_of(&state, warleader).expect("the Warleader is attacking");
    assert_eq!(defender, AttackTarget::Player(PlayerId(1)));
    for cat in &cats {
        assert_eq!(
            attack_target_of(&state, cat.id),
            Some(defender),
            "the token attacks what its creator's attack named"
        );
        assert!(cat.tapped, "the card creates them tapped");
        let current = characteristics(&state, cat.id, &db);
        assert_eq!((current.power, current.toughness), (Some(1), Some(1)));
        assert_eq!(current.subtypes, vec!["Cat".to_string()]);
        assert!(current.keywords.contains(&Keyword::Lifelink));
    }

    // They are attackers everywhere combat asks the question, not just in their own
    // field: the defending player is offered them to block.
    let declared = sage_engine::declared_attackers(&state);
    assert!(cats.iter().all(|cat| declared.contains(&cat.id)));
    assert_eq!(
        sage_engine::attacked_players(&state),
        vec![PlayerId(1)],
        "one defender, attacked by all three"
    );
}

/// The Cats deal combat damage in the combat they were created into — the point of
/// arriving attacking, and the half a "creates two tokens" effect would miss entirely.
/// Six damage lands (4 + 1 + 1) and their lifelink gains 2, all from one unblocked combat.
#[test]
fn issue_734_the_cats_deal_combat_damage_this_combat() {
    let db = db();
    let mut state = main_phase();
    let warleader = place(&mut state, &db, "leonin_warleader", PlayerId(0));

    let state = declare_attack(&state, &db, warleader);
    let state = resolve_stack(&state, &db);
    let state = advance_to(&state, &db, Step::PostcombatMain);

    assert_eq!(
        state.players[1].life, 14,
        "the 4/4 and both 1/1 Cats connected"
    );
    assert_eq!(
        state.players[0].life, 22,
        "two lifelinking Cats dealt 1 each (CR 702.15e)"
    );
}

/// A creature the defender keeps back may block a token that arrived attacking, exactly
/// as it may block a declared one: being in combat is a fact about the object, not about
/// how it got there.
#[test]
fn issue_734_a_created_attacker_can_be_blocked() {
    let db = db();
    let mut state = main_phase();
    let warleader = place(&mut state, &db, "leonin_warleader", PlayerId(0));
    let spider = place(&mut state, &db, "giant_spider", PlayerId(1));
    state.players[1].turn_began = state.turn;

    let state = declare_attack(&state, &db, warleader);
    let state = resolve_stack(&state, &db);
    let mut state = advance_to(&state, &db, Step::DeclareBlockers);

    let cat = tokens(&state)[0].id;
    assert!(
        sage_engine::blocker_candidates_for(&state, PlayerId(1), &db).contains(&spider),
        "the defender has a blocker to assign"
    );
    state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![sage_engine::Block {
                blocker: spider,
                attacker: cat,
            }],
        },
        &db,
    );
    let state = advance_to(&state, &db, Step::PostcombatMain);

    assert!(
        !state.battlefield.iter().any(|p| p.id == cat),
        "the blocked 1/1 died to the 2/4"
    );
    assert_eq!(
        state.players[1].life, 15,
        "the Warleader and the unblocked Cat connected; the blocked one did not"
    );
}

// ----- what it is *not* -----------------------------------------------------

/// CR 506.3c: a token put onto the battlefield attacking was never **declared** as an
/// attacker, so a "whenever this creature attacks" ability does not trigger for it.
///
/// Inline (ADR 0009) because no bundled card creates an attacking token that carries an
/// attack trigger of its own — and that is the only shape in which the rule is
/// observable. The same token declared as an attacker in the ordinary way *does* trigger,
/// asserted alongside it, so this is the declaration rule and not "tokens never trigger".
#[test]
fn issue_734_no_attack_trigger_fires_for_a_token_created_attacking_cr_506_3c() {
    let json = r#"[{"schema_version":1,"functional_id":"test_warcaller","name":"Test Warcaller",
        "types":["creature"],"subtypes":["Cat"],"mana_cost":"{1}","colors":["white"],
        "power":2,"toughness":2,
        "abilities":[{"type":"triggered","event":"self_attacks","effects":[
          {"kind":"create_token","attacking":true,
           "token":{"name":"Herald","types":["creature"],"subtypes":["Cat"],
                    "colors":["white"],"power":1,"toughness":1,
                    "abilities":[{"type":"triggered","event":"self_attacks",
                                  "effects":[{"kind":"gain_life","player_ref":"controller","amount":7}]}]}}]}]}]"#;
    let db = CardDatabase::from_json(json).expect("an inline definition");
    let mut state = main_phase();
    let warcaller = place(&mut state, &db, "test_warcaller", PlayerId(0));
    let life = state.players[0].life;

    let state = declare_attack(&state, &db, warcaller);
    let state = resolve_stack(&state, &db);

    let herald = tokens(&state)[0];
    assert!(
        herald.attacking.is_some(),
        "the token really did arrive attacking — otherwise this proves nothing"
    );
    assert_eq!(
        state.players[0].life, life,
        "the token was never declared as an attacker, so its own attack trigger did not \
         fire (CR 506.3c)"
    );
    assert!(
        state.stack.is_empty(),
        "nothing else went on the stack when the tokens arrived"
    );

    // The control: the same token, on the battlefield beforehand and declared as an
    // attacker in the ordinary way, triggers.
    let mut state = main_phase();
    let herald = place_token(&mut state, herald_token(), PlayerId(0));
    let life = state.players[0].life;
    let state = declare_attack(&state, &db, herald);
    let state = resolve_stack(&state, &db);
    assert_eq!(
        state.players[0].life,
        life + 7,
        "declared as an attacker, the very same ability triggers"
    );
}

/// The 1/1 Herald the inline definition above creates: a token that carries an attack
/// trigger of its own, which is the only way "no attack trigger fires for it" is
/// observable at all.
fn herald_token() -> TokenData {
    TokenData {
        name: "Herald".to_string(),
        types: vec![CardType::Creature],
        subtypes: vec!["Cat".to_string()],
        colors: vec![Color::White],
        power: Some(1),
        toughness: Some(1),
        abilities: vec![Ability::Triggered {
            event: TriggerCondition::SelfAttacks,
            effects: vec![Effect::GainLife {
                player_ref: PlayerRef::Controller,
                amount: 7,
            }],
        }],
        ..TokenData::default()
    }
}

/// Outside combat there is no attack to join, so the effect creates the tokens and simply
/// nothing is attacking. Inline (ADR 0009): the bundled cards that create attacking tokens
/// are attack-triggered, which is exactly the case this must not rely on.
#[test]
fn issue_734_outside_combat_the_effect_creates_nothing_attacking() {
    let json = r#"[{"schema_version":1,"functional_id":"test_vanguard","name":"Test Vanguard",
        "types":["creature"],"subtypes":["Cat"],"mana_cost":"{1}","colors":["white"],
        "power":2,"toughness":2,
        "abilities":[{"type":"triggered","event":"self_enters_battlefield","effects":[
          {"kind":"create_token","count":2,"attacking":true,
           "token":{"name":"Cat","types":["creature"],"subtypes":["Cat"],
                    "colors":["white"],"power":1,"toughness":1}}]}]}]"#;
    let db = CardDatabase::from_json(json).expect("an inline definition");
    let mut state = main_phase();
    let instance = to_hand(&mut state, &db, "test_vanguard", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = resolve_stack(&state, &db);

    let cats = tokens(&state);
    assert_eq!(cats.len(), 2, "the tokens are still created");
    assert!(
        cats.iter().all(|cat| cat.attacking.is_none()),
        "there was no attack to join, so nothing is attacking"
    );
    assert!(
        sage_engine::declared_attackers(&state).is_empty(),
        "an effect resolving in a main phase puts no one in combat"
    );
}

/// The source left combat before its own trigger resolved: the tokens are created, and
/// they attack nothing. The effect never invents a defender — "attacking" is always the
/// attack its source is making, and there is no longer one to point at.
#[test]
fn issue_734_a_creator_removed_from_combat_creates_no_attackers() {
    let db = db();
    let mut state = main_phase();
    let warleader = place(&mut state, &db, "leonin_warleader", PlayerId(0));
    let murder = to_hand(&mut state, &db, "murder", PlayerId(1));

    let state = declare_attack(&state, &db, warleader);
    assert_eq!(state.stack.len(), 1);
    // The defender answers the trigger by killing the Warleader; the trigger resolves
    // afterwards, with nothing left in combat to join.
    let mut state = apply_action(&state, &Action::PassPriority, &db);
    // Pools empty as each step ends (CR 500.4), and the walk to combat crossed several.
    state.players[1].mana_pool.add(Color::Black, 3);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: murder,
            targets: vec![Target::Permanent(warleader)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = resolve_stack(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == warleader),
        "the Warleader was destroyed in response to its own trigger"
    );
    let cats = tokens(&state);
    assert_eq!(
        cats.len(),
        2,
        "the trigger still resolved and made its Cats"
    );
    assert!(
        cats.iter().all(|cat| cat.attacking.is_none()),
        "with its source out of combat the effect names no defender"
    );
    let state = advance_to(&state, &db, Step::PostcombatMain);
    assert_eq!(
        state.players[1].life, 20,
        "nothing was attacking, so no combat damage was dealt"
    );
}
