//! Effects with no chosen target: mana, draws, life, and the class-wide forms.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_card_effects_etb_draw_end_to_end() {
    // Full vertical slice: three Forests already in play tap for {G}{G}{G}, cast
    // Skyscanner ({3}, an ETB "draw a card"), resolve it (ETB triggers), then
    // resolve the trigger (controller draws).
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let forests: Vec<PermanentId> = (0..3)
        .map(|_| place_permanent(&mut state, fixture("forest"), PlayerId(0), false, 0))
        .collect();
    let scanner = state.new_instance(fixture("skyscanner"));
    let draw_card = state.new_instance(fixture("onakke_ogre"));
    state.players[0].hand = vec![scanner];
    state.players[0].library = vec![draw_card];

    // Tap the three Forests for {G} each (mana abilities resolve immediately).
    for forest in forests {
        state = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: forest,
                index: 0,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
    }
    assert_eq!(state.players[0].mana_pool.green, 3);
    assert!(state.stack.is_empty());

    // Cast Skyscanner ({3} paid from the three green).
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: scanner,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.players[0].mana_pool.green, 0);

    // Pass twice: the creature resolves and its ETB trigger goes on the stack.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state
        .battlefield
        .iter()
        .any(|p| p.printed.card() == Some(fixture("skyscanner"))));
    assert_eq!(state.stack.len(), 1);

    // Pass twice more: the ETB ability resolves and player 0 draws.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state.stack.is_empty());
    assert!(state.players[0].hand.iter().any(|c| c.id == draw_card.id));
    assert!(state.players[0].library.is_empty());
}

#[test]
fn issue_119_zero_life_loss_records_its_reason_cr_704_5a() {
    // CR 704.5a: the life ≤ 0 loss now carries its reason and consumes into a
    // terminal result naming the winner (CR 104.2a).
    let db = db();
    let mut state = GameState::new_two_player();
    state.players[1].life = 0;
    let after = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(after.players[1].loss_reason, Some(LossReason::ZeroLife));
    let result = after.result().unwrap();
    assert_eq!(result.winner, Some(PlayerId(0)));
    assert_eq!(result.reason, LossReason::ZeroLife);
}

// ----- damage dealt to a class rather than to a target (issue #611) ---------

/// Push a player-0 ability that deals `amount` damage to `subject`, one full
/// priority round from resolving. The class forms choose nothing, so no target
/// list is ever threaded through. The source is a land, so it is never itself a
/// member of a creature class and cannot quietly change what a test observes.
fn push_class_damage(state: &mut GameState, subject: DamageSubject, amount: u32) {
    let source = place_permanent(state, fixture("forest"), PlayerId(0), false, 0);
    push_ability(
        state,
        source,
        vec![Effect::DealDamage { subject, amount }],
        Vec::new(),
    );
}

#[test]
fn issue_611_damage_to_each_opponent_hits_every_seat_not_one() {
    // "Each opponent" is the reason this is not a target: in a game of three it
    // must hit *both* other seats. A single-target implementation passes the
    // two-player case and silently halves this one.
    let db = db();
    let mut state = GameState::new_multiplayer(3);
    state.step = Step::PrecombatMain;
    push_class_damage(
        &mut state,
        DamageSubject::Players(PlayerRef::EachOpponent),
        2,
    );

    // Three seats, so three passes before the top of the stack resolves.
    for _ in 0..3 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    assert!(state.stack.is_empty(), "the ability resolved");
    assert_eq!(
        state.players[0].life, 20,
        "the controller is not an opponent"
    );
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[2].life, 18, "the second opponent too");
}

#[test]
fn issue_611_class_damage_resolves_where_a_targeted_form_would_fizzle() {
    // The same 2 damage, authored two ways, over a board that lost the creature
    // between announcement and resolution: the targeted form has no legal target
    // left and is removed without effect (CR 608.2b), while the class form chose
    // nothing, so there is nothing to re-check and nothing to fizzle.
    let db = db();
    let mut state = main_phase_p0();
    let doomed = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let source = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), false, 0);
    push_ability(
        &mut state,
        source,
        vec![Effect::DealDamage {
            subject: DamageSubject::Target(TargetSpec::AnyCreature),
            amount: 2,
        }],
        vec![Target::Permanent(doomed)],
    );
    push_ability(
        &mut state,
        source,
        vec![Effect::DealDamage {
            subject: DamageSubject::Players(PlayerRef::EachOpponent),
            amount: 2,
        }],
        Vec::new(),
    );
    // The chosen creature leaves before either ability resolves.
    state.battlefield.retain(|p| p.id != doomed);

    // Top of the stack (the class form) first, then the targeted one.
    let state = pass_full_round(&state, &db);
    assert_eq!(
        state.players[1].life, 18,
        "the class form dealt its damage with no target to lose"
    );
    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty(), "both abilities left the stack");
    assert_eq!(
        state.players[1].life, 18,
        "the targeted form fizzled — its only target was gone (CR 608.2b)"
    );
}

#[test]
fn issue_611_class_damage_to_creatures_is_marked_and_drives_the_lethal_sba() {
    // Damage dealt through the class form is *damage*, not life loss: it is marked
    // on each creature (CR 120.3d) and the CR 704.5g state-based action destroys
    // the one it is lethal to, exactly as a targeted burn spell does.
    let db = db();
    let mut state = main_phase_p0();
    // Onakke Ogre is a 4/2 — 2 damage is lethal. Pelakka Wurm is a 7/7 and lives
    // with 2 marked.
    let ogre = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let wurm = place_permanent(&mut state, fixture("pelakka_wurm"), PlayerId(0), false, 0);
    push_class_damage(
        &mut state,
        DamageSubject::Permanents(MassAffects::EachCreature),
        2,
    );

    let state = pass_full_round(&state, &db);

    assert!(
        !alive(&state, ogre),
        "2 damage is lethal to a 4/2 (CR 704.5g)"
    );
    assert_eq!(
        state.players[1].graveyard.len(),
        1,
        "it went to its owner's graveyard, the same path a burn spell uses"
    );
    assert_eq!(
        find_perm(&state, wurm).damage,
        2,
        "the survivor carries marked damage, not lost life"
    );
}

#[test]
fn issue_611_a_one_sided_sweeper_spares_the_creatures_you_control() {
    // The class is read relative to the ability's controller, so one authored
    // card means "your opponents" from either seat.
    let db = db();
    let mut state = main_phase_p0();
    let theirs = place_permanent(&mut state, fixture("pelakka_wurm"), PlayerId(1), false, 0);
    let yours = place_permanent(&mut state, fixture("pelakka_wurm"), PlayerId(0), false, 0);
    push_class_damage(
        &mut state,
        DamageSubject::Permanents(MassAffects::CreaturesYourOpponentsControl),
        3,
    );

    let state = pass_full_round(&state, &db);

    assert_eq!(find_perm(&state, theirs).damage, 3);
    assert_eq!(find_perm(&state, yours).damage, 0, "your own are spared");
}

#[test]
fn issue_611_the_class_is_enumerated_on_resolution_not_on_announcement() {
    // CR 611.2c: the class is turned into a set when the ability resolves, so a
    // creature that arrived while it waited on the stack is in it — the rule
    // `PumpAll` already follows, reused here rather than re-derived. Announcement
    // chose nothing, so there is no earlier moment it could have been fixed at.
    let db = db();
    let mut state = main_phase_p0();
    push_class_damage(
        &mut state,
        DamageSubject::Permanents(MassAffects::EachCreature),
        1,
    );
    let late = place_permanent(&mut state, fixture("pelakka_wurm"), PlayerId(1), false, 0);

    let state = pass_full_round(&state, &db);

    assert_eq!(
        find_perm(&state, late).damage,
        1,
        "a creature that arrived after announcement is included"
    );
}

#[test]
fn issue_611_the_targeted_damage_form_is_unchanged() {
    // The shape every existing burn card is authored in still fills exactly one
    // slot and still reports its spec, so nothing about announcement changed.
    assert_eq!(
        Effect::DealDamage {
            subject: DamageSubject::Target(TargetSpec::AnyTarget),
            amount: 2,
        }
        .target_spec(),
        Some(TargetSpec::AnyTarget),
    );
    for subject in [
        DamageSubject::Players(PlayerRef::EachOpponent),
        DamageSubject::Players(PlayerRef::Controller),
        DamageSubject::Permanents(MassAffects::EachCreature),
    ] {
        assert_eq!(
            Effect::DealDamage { subject, amount: 2 }.target_spec(),
            None,
            "a class fills no target slot (CR 115.1)",
        );
    }
}

#[test]
fn issue_256_divination_draws_two_cards() {
    // Divination is a {2}{U} sorcery that draws two — DrawCard flowing through the
    // spell-resolution path (until now it was only ever a triggered-ability effect,
    // so this proves the cast → resolve routing).
    let db = db();
    let mut state = main_phase_p0();
    let study = state.new_instance(fixture("divination"));
    state.players[0].hand = vec![study];
    let first = state.new_instance(fixture("forest"));
    let second = state.new_instance(fixture("forest"));
    state.players[0].library = vec![first, second];
    state.players[0].mana_pool.add(Color::Blue, 1);
    state.players[0].mana_pool.colorless = 2;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: study,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(state.players[0].hand.contains(&first));
    assert!(state.players[0].hand.contains(&second));
    assert!(state.players[0].library.is_empty());
}

#[test]
fn issue_256_enchantment_etb_gains_life_when_it_enters() {
    // A {G} enchantment whose enters-the-battlefield trigger gains its controller
    // 4 life — an ETB trigger on a *non-creature* permanent, and GainLife as an
    // ability effect rather than a spell effect. No M19 card carries this, so it
    // is exercised inline.
    let json = r#"[{"schema_version":1,"functional_id":"test_blessing","name":"Test Blessing",
        "types":["enchantment"],"subtypes":[],"mana_cost":"{G}","colors":["green"],
        "abilities":[{"type":"triggered","event":"self_enters_battlefield",
          "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]}]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = main_phase_p0();
    let life_before = state.players[0].life;
    let blessing = state.new_instance(id_in(&db, "test_blessing"));
    state.players[0].hand = vec![blessing];
    state.players[0].mana_pool.add(Color::Green, 1);

    // Cast it; pass twice so it resolves and its ETB trigger goes on the stack.
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: blessing,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    assert!(state
        .battlefield
        .iter()
        .any(|p| p.printed.card() == Some(id_in(&db, "test_blessing"))));
    assert_eq!(state.stack.len(), 1, "its ETB trigger is on the stack");

    // Pass twice more: the trigger resolves and the controller gains 4 life.
    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].life, life_before + 4);
}

#[test]
fn issue_256_mana_rock_taps_for_colorless_mana() {
    // A {1} mana rock — {T}: Add {C}. Its ability is a mana ability, so it
    // resolves immediately without using the stack (CR 605.3). The colorless-mana
    // verb has no M19 representative, so it is exercised inline.
    let json = r#"[{"schema_version":1,"functional_id":"test_lodestone","name":"Test Lodestone",
        "types":["artifact"],"mana_cost":"{1}","colors":[],
        "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
          "effects":[{"kind":"add_colorless_mana","amount":1}]}]}]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = main_phase_p0();
    let lodestone = place_permanent(
        &mut state,
        id_in(&db, "test_lodestone"),
        PlayerId(0),
        false,
        0,
    );

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: lodestone,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.players[0].mana_pool.colorless, 1);
    assert!(find_perm(&after, lodestone).tapped);
    assert!(after.stack.is_empty());
}

#[test]
fn issue_149_life_gain_adds_to_a_low_life_total_cr_119() {
    let db = db();
    let mut state = main_phase_p0();
    state.players[0].life = 1;
    let balm = state.new_instance(fixture("revitalize")); // Revitalize {1}{W}: gain 3, draw 1
    state.players[0].hand = vec![balm];
    // Revitalize also draws, so seed a card to avoid decking out (CR 704.5c).
    state.players[0].library = vec![state.new_instance(fixture("forest"))];
    state.players[0].mana_pool.add(Color::White, 1);
    state.players[0].mana_pool.add_colorless(1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: balm,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert_eq!(state.players[0].life, 4);
    assert!(!state.players[0].has_lost);
}

#[test]
fn issue_149_life_loss_to_exactly_zero_triggers_the_loss_cr_704_5a() {
    // The lose-life verb has no M19 representative, so it is exercised inline.
    let json = r#"[{"schema_version":1,"functional_id":"test_drain","name":"Test Drain",
        "types":["instant"],"mana_cost":"{B}","colors":["black"],
        "spell_effects":[{"kind":"lose_life","player_ref":"controller","amount":2}]}]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = main_phase_p0();
    state.players[0].life = 2;
    let ordeal = state.new_instance(id_in(&db, "test_drain")); // {B} instant, lose 2
    state.players[0].hand = vec![ordeal];
    state.players[0].mana_pool.add(Color::Black, 1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: ordeal,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert_eq!(state.players[0].life, 0);
    assert!(state.players[0].has_lost);
    assert_eq!(state.players[0].loss_reason, Some(LossReason::ZeroLife));
}

#[test]
fn issue_401_highland_game_gains_two_life_when_it_dies() {
    // Highland Game: "When Highland Game dies, you gain 2 life." Killed by a
    // Destroy effect, its dies trigger resolves and its controller gains 2.
    use crate::ability::TargetSpec;
    let db = db();
    let mut state = main_phase_p0();
    let life_before = state.players[0].life;
    let elk = place_permanent(&mut state, fixture("highland_game"), PlayerId(0), false, 0);
    push_ability(
        &mut state,
        elk,
        vec![Effect::Destroy {
            targets: crate::ability::TargetCount::default(),
            target: TargetSpec::AnyCreature,
        }],
        vec![Target::Permanent(elk)],
    );

    // Resolve the destroy: the Elk dies and its dies trigger lands on the stack.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!alive(&state, elk), "the Destroy killed the Elk");
    assert_eq!(state.stack.len(), 1, "the dies trigger is on the stack");

    // Resolve the dies trigger.
    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].life, life_before + 2);
}

#[test]
fn issue_401_rhox_oracle_draws_a_card_when_it_enters() {
    // Rhox Oracle: a {4}{G} 4/2 whose ETB draws a card.
    let db = db();
    let mut state = main_phase_p0();
    let oracle = state.new_instance(fixture("rhox_oracle"));
    let card = state.new_instance(fixture("forest"));
    state.players[0].hand = vec![oracle];
    state.players[0].library = vec![card];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 4;

    // Cast; pass twice so it resolves and its ETB trigger goes on the stack.
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: oracle,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    assert!(state
        .battlefield
        .iter()
        .any(|p| p.printed.card() == Some(fixture("rhox_oracle"))));
    assert_eq!(state.stack.len(), 1, "its ETB trigger is on the stack");

    // Pass twice more: the trigger resolves and player 0 draws.
    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty());
    assert!(state.players[0].hand.contains(&card));
}

#[test]
fn issue_401_pelakka_wurm_gains_seven_life_on_etb_and_draws_when_it_dies() {
    // Pelakka Wurm carries two triggers: ETB gain 7 life, and dies draw a card.
    use crate::ability::TargetSpec;
    let db = db();

    // ETB: cast it and resolve the enters trigger.
    let mut state = main_phase_p0();
    let life_before = state.players[0].life;
    let wurm = state.new_instance(fixture("pelakka_wurm"));
    state.players[0].hand = vec![wurm];
    state.players[0].mana_pool.add(Color::Green, 3);
    state.players[0].mana_pool.colorless = 4;
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: wurm,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db); // resolves the creature; ETB on stack
    assert_eq!(
        state.stack.len(),
        1,
        "the ETB gain-life trigger is on the stack"
    );
    let state = pass_full_round(&state, &db); // resolves the ETB trigger
    assert_eq!(state.players[0].life, life_before + 7);

    // Dies: place one and destroy it, then resolve the dies-draw trigger.
    let mut state = main_phase_p0();
    let onbf = place_permanent(&mut state, fixture("pelakka_wurm"), PlayerId(0), false, 0);
    let card = state.new_instance(fixture("forest"));
    state.players[0].library = vec![card];
    push_ability(
        &mut state,
        onbf,
        vec![Effect::Destroy {
            targets: crate::ability::TargetCount::default(),
            target: TargetSpec::AnyCreature,
        }],
        vec![Target::Permanent(onbf)],
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!alive(&state, onbf), "the Wurm died to the Destroy");
    assert_eq!(
        state.stack.len(),
        1,
        "the dies-draw trigger is on the stack"
    );
    let state = pass_full_round(&state, &db);
    assert!(state.players[0].hand.contains(&card));
}
