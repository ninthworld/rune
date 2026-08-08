//! Layer 6 subtracting: a keyword removed, all abilities removed, and the timestamp
//! that decides between a removal and a grant (CR 613.1f).
//!
//! The ability-adding layer only added, and Gargoyle Sentinel is the smallest card that
//! needs it to do the other thing: a wall that pays `{3}` to stop being a wall. Two
//! claims are worth proving separately. Removing a *named* keyword is arithmetic on one
//! set, and the only hard part is ordering — a grant after a removal grants, and the
//! reverse does not. Removing *all* abilities is not arithmetic on anything: it has to
//! reach every collector that walks a permanent's abilities, or a removed trigger still
//! fires and a silenced permanent still offers its activation. Both are asserted here,
//! never assumed.
//!
//! Every test drives the **real** [`apply_action`] pipeline and walks the real turn
//! structure. Cards are named by their authored `functional_id`, never by an interned
//! handle (ADR 0008 §3); the shapes M19 does not print are exercised through inline
//! definitions (ADR 0009).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attacker_candidates, characteristics, valid_actions, Ability, Action, Attack,
    AttackTarget, CardDatabase, CardId, Color, FunctionalId, GameState, Keyword, Permanent,
    PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk several whole turns; a settle that has not arrived by then is
/// a hang, and failing beats spinning.
const SETTLE_LIMIT: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a card in this catalog")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability never
/// decides a test about a layer, and both libraries stocked out of `library_card` so a
/// multi-turn walk never trips the CR 704.5c decking loss.
fn main_phase(db: &CardDatabase, library_card: &str) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let filler = cid(db, library_card);
    for seat in 0..2 {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            state.players[seat].mana_pool.add(color, 20);
        }
        state.players[seat].mana_pool.add_colorless(20);
        state.players[seat].library = (0..20).map(|_| state.new_instance(filler)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free
/// of summoning sickness, and return its battlefield identity.
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

/// The requirement form of `permanent`'s ability `index` — the shape `valid_actions`
/// advertises for an activation that names no target.
fn activation(permanent: PermanentId, index: usize) -> Action {
    Action::ActivateAbility {
        permanent,
        index,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// Activate ability `index` of `permanent` and let it resolve, through the ordinary
/// offer gate: an activation `valid_actions` would not offer fails here rather than
/// silently doing nothing.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> GameState {
    let action = activation(permanent, index);
    assert!(
        valid_actions(state, db).contains(&action),
        "ability {index} was not offered"
    );
    let state = apply_action(state, &action, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is.
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if done(&state) {
            return state;
        }
        let offered = valid_actions(&state, db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|action| action != &Action::Concede)
                .expect("some action is always available")
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the game never reached the state under test");
}

/// Declare `attacker` as an attacker on `defender`, from wherever the game currently is.
fn attack_with(
    state: &GameState,
    db: &CardDatabase,
    attacker: PermanentId,
    defender: PlayerId,
) -> GameState {
    let state = settle_until(state, db, |s| s.step == Step::DeclareAttackers);
    apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(defender),
            }],
        },
        db,
    )
}

/// Whether `id` currently has `keyword`, read through the computed characteristics —
/// the one path combat, evasion, and damage read.
fn has(state: &GameState, db: &CardDatabase, id: PermanentId, keyword: Keyword) -> bool {
    characteristics(state, id, db).keywords.contains(&keyword)
}

// ----- Gargoyle Sentinel ----------------------------------------------------

#[test]
fn gargoyle_sentinel_prints_defender_and_flies_for_nobody() {
    // The starting position the rest of the card is measured against. If the printed
    // keyword were not really there, "it loses defender" would prove nothing.
    let db = db();
    let mut state = main_phase(&db, "forest");
    let gargoyle = place(&mut state, &db, "gargoyle_sentinel", PlayerId(0));

    assert!(has(&state, &db, gargoyle, Keyword::Defender));
    assert!(!has(&state, &db, gargoyle, Keyword::Flying));
    assert!(
        !attacker_candidates(&state, &db).contains(&gargoyle),
        "defender is what keeps it out of the attacker set (CR 702.3)"
    );
}

#[test]
fn cr_613_1f_the_activation_takes_defender_off_and_puts_flying_on() {
    // One effect for one printed sentence: the loss and the gain are the same clause,
    // share one timestamp, and land together.
    let db = db();
    let mut state = main_phase(&db, "forest");
    let gargoyle = place(&mut state, &db, "gargoyle_sentinel", PlayerId(0));

    let after = activate(&state, &db, gargoyle, 0);

    assert!(
        !has(&after, &db, gargoyle, Keyword::Defender),
        "the printed keyword is gone at layer 6, not merely overridden"
    );
    assert!(has(&after, &db, gargoyle, Keyword::Flying));
}

#[test]
fn gargoyle_sentinel_attacks_while_the_effect_is_up() {
    // The point of the card, driven through the real declaration: the attacker candidate
    // set reads the *computed* keywords, so removing defender is the whole of what makes
    // the declaration legal.
    let db = db();
    let mut state = main_phase(&db, "forest");
    let gargoyle = place(&mut state, &db, "gargoyle_sentinel", PlayerId(0));

    let after = activate(&state, &db, gargoyle, 0);
    assert!(
        attacker_candidates(&after, &db).contains(&gargoyle),
        "with defender gone it is an attacker its controller may declare"
    );

    let declared = attack_with(&after, &db, gargoyle, PlayerId(1));
    let damaged = settle_until(&declared, &db, |s| s.step == Step::EndCombat);
    assert_eq!(
        damaged.players[1].life, 17,
        "a 3/4 with nothing to block it hit for three"
    );
}

#[test]
fn and_cannot_once_the_effect_has_ended() {
    // "Until end of turn" ends at CR 514.2 with nothing written back: the removal is
    // simply gone from the stored effects and the printed keyword answers again, because
    // it was never taken off the card (ADR 0005). Turn 3 is player 0's next turn.
    let db = db();
    let mut state = main_phase(&db, "forest");
    let gargoyle = place(&mut state, &db, "gargoyle_sentinel", PlayerId(0));

    let after = activate(&state, &db, gargoyle, 0);
    let later = settle_until(&after, &db, |s| {
        s.turn == 3 && s.step == Step::DeclareAttackers
    });

    assert!(
        has(&later, &db, gargoyle, Keyword::Defender),
        "defender came back at cleanup"
    );
    assert!(!has(&later, &db, gargoyle, Keyword::Flying));
    assert!(
        !attacker_candidates(&later, &db).contains(&gargoyle),
        "so it is a wall again on its controller's next turn"
    );
}

// ----- the ordering CR 613.1f turns on --------------------------------------

/// A creature that prints flying and can take it off or put it back at will — two
/// clauses over one keyword, which is the only shape in which "which of these spoke
/// last?" is observable. No M19 card says both halves, so it is authored inline
/// (ADR 0009).
const SHIFTER: &str = r#"[{"schema_version":1,"functional_id":"test_shifter","name":"Test Shifter",
    "types":["creature"],"subtypes":["Shapeshifter"],"mana_cost":"{2}","colors":["blue"],
    "power":2,"toughness":2,"keywords":["flying"],
    "abilities":[
      {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
       "effects":[{"kind":"alter_abilities_self","lose":["flying"]}]},
      {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
       "effects":[{"kind":"alter_abilities_self","gain":["flying"]}]}]}]"#;

#[test]
fn cr_613_1f_a_grant_after_a_removal_leaves_the_keyword_present() {
    let db = CardDatabase::from_json(SHIFTER).expect("an inline definition");
    let mut state = main_phase(&db, "test_shifter");
    let shifter = place(&mut state, &db, "test_shifter", PlayerId(0));

    let removed = activate(&state, &db, shifter, 0);
    assert!(!has(&removed, &db, shifter, Keyword::Flying));

    let regranted = activate(&removed, &db, shifter, 1);
    assert!(
        has(&regranted, &db, shifter, Keyword::Flying),
        "the later timestamp speaks last, so the grant puts it back"
    );
}

#[test]
fn cr_613_1f_a_removal_after_a_grant_leaves_the_keyword_absent() {
    // The same two effects, the other way round. Both are still in force — nothing was
    // pruned — so the only thing deciding the answer is which one is later.
    let db = CardDatabase::from_json(SHIFTER).expect("an inline definition");
    let mut state = main_phase(&db, "test_shifter");
    let shifter = place(&mut state, &db, "test_shifter", PlayerId(0));

    let granted = activate(&state, &db, shifter, 1);
    assert!(has(&granted, &db, shifter, Keyword::Flying));

    let removed = activate(&granted, &db, shifter, 0);
    assert!(
        !has(&removed, &db, shifter, Keyword::Flying),
        "the removal is later, so it wins"
    );
    assert_eq!(
        removed.static_effects.len(),
        2,
        "and it wins by ordering rather than by having deleted the grant"
    );
}

// ----- losing all abilities -------------------------------------------------

/// A creature carrying one of each kind of ability that has to go quiet: an activated
/// one, a triggered one, and — on a second card — a static one whose subject is somebody
/// else. Nothing in M19 says "loses all abilities" without also needing type-changing,
/// so the shape is authored inline (ADR 0009).
const MUTE: &str = r#"[
    {"schema_version":1,"functional_id":"test_mute","name":"Test Mute",
     "types":["creature"],"subtypes":["Golem"],"mana_cost":"{2}","colors":[],
     "power":2,"toughness":2,"keywords":["vigilance"],
     "abilities":[
       {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
        "effects":[{"kind":"alter_abilities_self","lose_all":true}]},
       {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
        "effects":[{"kind":"gain_life","player_ref":"controller","amount":3}]},
       {"type":"triggered","event":"self_attacks",
        "effects":[{"kind":"gain_life","player_ref":"controller","amount":7}]}]},
    {"schema_version":1,"functional_id":"test_hushed_lord","name":"Test Hushed Lord",
     "types":["creature"],"subtypes":["Giant"],"mana_cost":"{3}","colors":["green"],
     "power":3,"toughness":3,
     "abilities":[
       {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
        "effects":[{"kind":"alter_abilities_self","lose_all":true}]},
       {"type":"static","affects":{"scope":"creatures_you_control","except_this":true},
        "modification":{"kind":"power_toughness","power":1,"toughness":1}}]},
    {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
     "types":["creature"],"subtypes":["Bear"],"mana_cost":"{2}","colors":["green"],
     "power":2,"toughness":2},
    {"schema_version":1,"functional_id":"test_hexed","name":"Test Hexed",
     "types":["creature"],"subtypes":["Spirit"],"mana_cost":"{2}","colors":["white"],
     "power":2,"toughness":2,"keywords":["defender"],
     "abilities":[
       {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
        "effects":[{"kind":"alter_abilities_self","lose_all":true,"gain":["flying"]}]}]}
]"#;

#[test]
fn losing_all_abilities_empties_the_computed_ability_set_and_the_keywords_with_it() {
    // A keyword ability is an ability (CR 702.1), so the printed vigilance goes too —
    // and so does everything the ability list carries.
    let db = CardDatabase::from_json(MUTE).expect("an inline definition");
    let mut state = main_phase(&db, "test_bear");
    let mute = place(&mut state, &db, "test_mute", PlayerId(0));

    assert_eq!(characteristics(&state, mute, &db).abilities.len(), 3);
    assert!(has(&state, &db, mute, Keyword::Vigilance));

    let after = activate(&state, &db, mute, 0);

    assert!(
        characteristics(&after, mute, &db).abilities.is_empty(),
        "every ability is gone from the one accessor all of them are read through"
    );
    assert!(!has(&after, &db, mute, Keyword::Vigilance));
}

#[test]
fn losing_all_abilities_silences_an_activated_ability() {
    // Asserted at the offer, which is the only place it can be observed: an activation
    // that is not enumerated cannot be taken, and `apply_action` re-checks the offer, so
    // a forged action id cannot take it either.
    let db = CardDatabase::from_json(MUTE).expect("an inline definition");
    let mut state = main_phase(&db, "test_bear");
    let mute = place(&mut state, &db, "test_mute", PlayerId(0));
    let life = state.players[0].life;

    assert!(valid_actions(&state, &db).contains(&activation(mute, 1)));

    let after = activate(&state, &db, mute, 0);

    assert!(
        !valid_actions(&after, &db).contains(&activation(mute, 1)),
        "the gain-life ability is no longer offered"
    );
    assert!(
        !valid_actions(&after, &db).contains(&activation(mute, 0)),
        "and neither is the ability that did the silencing — it silenced itself too"
    );
    let forged = apply_action(&after, &activation(mute, 1), &db);
    assert_eq!(
        forged.players[0].life, life,
        "an action id from before the silencing changes nothing"
    );
}

#[test]
fn losing_all_abilities_silences_a_triggered_ability() {
    // The claim the whole feature turns on: trigger collection walks the same accessor,
    // so an ability that is gone at layer 6 does not fire. The control below is what
    // makes it an assertion rather than a coincidence — the very same attack, with the
    // silencing skipped, does gain the life.
    let db = CardDatabase::from_json(MUTE).expect("an inline definition");
    let mut state = main_phase(&db, "test_bear");
    let mute = place(&mut state, &db, "test_mute", PlayerId(0));
    let life = state.players[0].life;

    let silenced = activate(&state, &db, mute, 0);
    let attacked = attack_with(&silenced, &db, mute, PlayerId(1));
    let settled = settle_until(&attacked, &db, |s| s.step == Step::EndCombat);
    assert_eq!(
        settled.players[0].life, life,
        "the attack trigger did not fire for a creature with no abilities"
    );

    // The control: the same creature, the same attack, nothing silenced.
    let attacked = attack_with(&state, &db, mute, PlayerId(1));
    let settled = settle_until(&attacked, &db, |s| s.step == Step::EndCombat);
    assert_eq!(
        settled.players[0].life,
        life + 7,
        "left alone, that very trigger gains the life"
    );
}

#[test]
fn losing_all_abilities_silences_a_printed_static_so_the_lord_stops_pumping() {
    // The third collector: continuous effects are derived by walking every source's
    // abilities on every read, so a silenced lord contributes nothing — and the creature
    // it was pumping shrinks back the instant the effect resolves, with nothing pruned.
    let db = CardDatabase::from_json(MUTE).expect("an inline definition");
    let mut state = main_phase(&db, "test_bear");
    let lord = place(&mut state, &db, "test_hushed_lord", PlayerId(0));
    let bear = place(&mut state, &db, "test_bear", PlayerId(0));

    assert_eq!(characteristics(&state, bear, &db).power, Some(3));

    let after = activate(&state, &db, lord, 0);

    assert_eq!(
        characteristics(&after, bear, &db).power,
        Some(2),
        "the lord's static ability is gone, so the anthem it derived is gone with it"
    );
}

#[test]
fn cr_613_1f_a_keyword_gained_in_the_same_clause_survives_the_loss_of_everything() {
    // Losing all abilities is applied before the gain within one clause, which is the
    // order the card prints and the only order in which `loses all abilities and gains
    // flying` means anything at all.
    let db = CardDatabase::from_json(MUTE).expect("an inline definition");
    let mut state = main_phase(&db, "test_bear");
    let hexed = place(&mut state, &db, "test_hexed", PlayerId(0));

    assert!(has(&state, &db, hexed, Keyword::Defender));

    let after = activate(&state, &db, hexed, 0);

    assert!(
        !has(&after, &db, hexed, Keyword::Defender),
        "the printed keyword went with everything else"
    );
    assert!(
        has(&after, &db, hexed, Keyword::Flying),
        "and the keyword the same clause granted is still there"
    );
    assert_eq!(
        characteristics(&after, hexed, &db).abilities,
        Vec::<Ability>::new(),
        "a granted keyword is not an entry in the ability list"
    );
}

// ----- Resplendent Angel: layer 6 adding, on the same row -------------------

/// The additive counterpart, and the reason it is not spelled with the verb above.
///
/// `{3}{W}{W}{W}: Until end of turn, this creature gets +2/+2 and gains lifelink` was
/// **missing from the catalog entirely** (#819, #821), and the shape of the vocabulary is
/// why: the only self-directed keyword *addition* was `alter_abilities_self`'s `gain`,
/// whose sibling field is `lose_all`. Reaching for the lose-all-abilities verb to say
/// "gains lifelink" reads as a card doing something it does not do, so a `pump_self`
/// carries its own keywords now — one printed sentence, one effect, one CR 613.7
/// timestamp.
#[test]
fn issue_821_the_angel_pumps_and_grants_in_one_breath() {
    let db = db();
    let mut state = main_phase(&db, "forest");
    let angel = place(&mut state, &db, "resplendent_angel", PlayerId(0));

    let before = characteristics(&state, angel, &db);
    assert_eq!((before.power, before.toughness), (Some(3), Some(3)));
    assert!(before.keywords.contains(&Keyword::Flying), "printed flying");
    assert!(!before.keywords.contains(&Keyword::Lifelink));

    let state = activate(&state, &db, angel, 1);

    let after = characteristics(&state, angel, &db);
    assert_eq!(
        (after.power, after.toughness),
        (Some(5), Some(5)),
        "a 3/3 with +2/+2"
    );
    assert!(after.keywords.contains(&Keyword::Lifelink), "and lifelink");
    assert!(
        after.keywords.contains(&Keyword::Flying),
        "and the flying it printed — this grants, it does not replace, which is the \
         whole difference from the removal verb above"
    );
}

/// Both halves are `until end of turn` and both are gone at cleanup (CR 514.2), with
/// nothing written onto the permanent to undo (ADR 0005).
#[test]
fn issue_821_the_angels_pump_and_its_lifelink_wear_off_together() {
    let db = db();
    let mut state = main_phase(&db, "forest");
    let angel = place(&mut state, &db, "resplendent_angel", PlayerId(0));

    let state = activate(&state, &db, angel, 1);
    assert!(characteristics(&state, angel, &db)
        .keywords
        .contains(&Keyword::Lifelink));

    let next_turn = settle_until(&state, &db, |s| s.turn == 2 && s.step == Step::Upkeep);
    let after = characteristics(&next_turn, angel, &db);
    assert_eq!(
        (after.power, after.toughness),
        (Some(3), Some(3)),
        "the pump is gone"
    );
    assert!(
        !after.keywords.contains(&Keyword::Lifelink),
        "and so is the keyword it came with"
    );
    assert!(
        after.keywords.contains(&Keyword::Flying),
        "the printed keyword was never touched"
    );
}
