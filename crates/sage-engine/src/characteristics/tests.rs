//! Computed characteristics, end to end: counters, anthems, Auras, and grants folded
//! together by the layer system (CR 613).

#![allow(clippy::unwrap_used)]

use super::*;
use crate::ability::is_mana_ability;
use crate::fixtures::{fixture, id_in};
use crate::id::{CardId, CardInstanceId, PlayerId};
use crate::state::{Duration, Permanent};
use std::collections::BTreeMap;

/// Put a permanent for `card` on the battlefield and return its id.
fn place(state: &mut GameState, card: CardId) -> PermanentId {
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: CardInstanceId(0),
        printed: card.into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: BTreeMap::new(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    id
}

/// Set the count of `kind` counters on the permanent identified by `id`.
fn set_counters(state: &mut GameState, id: PermanentId, kind: CounterKind, count: u32) {
    let perm = state.battlefield.iter_mut().find(|p| p.id == id).unwrap();
    perm.counters.insert(kind, count);
}

/// Add a static anthem "+`power`/+`toughness` to creatures `controller`
/// controls" from a source with the given `source` object id (its timestamp).
fn add_anthem(
    state: &mut GameState,
    source: u64,
    controller: PlayerId,
    power: i32,
    toughness: i32,
) {
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::CreaturesControlledBy(controller),
        modification: Modification::PowerToughness { power, toughness },
        duration: Duration::WhileOnBattlefield,
    });
}

/// Add an "until end of turn" pump of +`power`/+`toughness` aimed at the
/// single permanent `target`, timestamped by `source` (its object id).
fn add_pump(state: &mut GameState, source: u64, target: PermanentId, power: i32, toughness: i32) {
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(target),
        modification: Modification::PowerToughness { power, toughness },
        duration: Duration::UntilEndOfTurn,
    });
}

#[test]
fn issue_150_pump_boosts_only_its_specific_target() {
    // A "+3/+3 until end of turn" pump aimed at one Ogre makes it a 7/5 and
    // leaves a second, unpumped Ogre at its printed 4/2 (the effect is keyed
    // to a specific permanent id, not a controller-wide selector).
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let pumped = place(&mut state, fixture("onakke_ogre"));
    let bystander = place(&mut state, fixture("onakke_ogre"));
    add_pump(&mut state, 100, pumped, 3, 3);

    let ch = characteristics(&state, pumped, &db);
    assert_eq!(ch.power, Some(7));
    assert_eq!(ch.toughness, Some(5));
    let other = characteristics(&state, bystander, &db);
    assert_eq!(other.power, Some(4));
    assert_eq!(other.toughness, Some(2));
}

#[test]
fn issue_150_two_pumps_on_one_target_stack_in_timestamp_order() {
    // Two pumps on the same Ogre sum (they are additive) and fold in ascending
    // timestamp order (CR 613.7) regardless of insertion order — printed 4/2 +
    // (+2/+0) + (+1/+2) = 7/4.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    add_pump(&mut state, 200, boar, 1, 2); // later timestamp, inserted first
    add_pump(&mut state, 100, boar, 2, 0);

    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(7));
    assert_eq!(ch.toughness, Some(4));

    let perm = state.battlefield.iter().find(|p| p.id == boar).unwrap();
    let ordered: Vec<u64> = ordered_pt_modifiers(&state, perm, true, &db)
        .iter()
        .map(|effect| effect.timestamp())
        .collect();
    assert_eq!(ordered, vec![100, 200]);
}

#[test]
fn issue_150_pump_on_a_noncreature_has_no_visible_effect() {
    // Layer 7c only adjusts an existing power/toughness: a pump keyed to a
    // Forest (no printed P/T) leaves it without any.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let forest = place(&mut state, fixture("forest"));
    add_pump(&mut state, 100, forest, 3, 3);

    let ch = characteristics(&state, forest, &db);
    assert_eq!(ch.power, None);
    assert_eq!(ch.toughness, None);
}

#[test]
fn vanilla_creature_current_pt_and_types_equal_printed() {
    // Onakke Ogre: a 4/2 Creature — Ogre Warrior with no modifiers, so its
    // current characteristics are exactly its printed ones.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));

    let ch = characteristics(&state, boar, &db);
    let printed = db.card(fixture("onakke_ogre")).unwrap();
    assert_eq!(ch.power, Some(4));
    assert_eq!(ch.toughness, Some(2));
    assert_eq!(ch.types, vec![CardType::Creature]);
    assert_eq!(ch.subtypes, vec!["Ogre".to_string(), "Warrior".to_string()]);
    assert_eq!(ch.mana_cost, "{2}{R}");
    // Every field mirrors the printed seed in this slice.
    assert_eq!(ch.supertypes, printed.supertypes);
    assert_eq!(ch.power, printed.power);
    assert_eq!(ch.toughness, printed.toughness);
    assert_eq!(ch.types, printed.types);
}

#[test]
fn plus_one_counters_add_to_printed_power_and_toughness() {
    // Onakke Ogre is a printed 4/2. Three +1/+1 counters make it a 7/5.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 3);

    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(7));
    assert_eq!(ch.toughness, Some(5));
    // Only P/T shifts; the printed types are untouched by counters.
    assert_eq!(ch.types, vec![CardType::Creature]);
}

#[test]
fn mixed_plus_and_minus_counters_net_correctly() {
    // 4/2 Ogre with two +1/+1 and one -1/-1 nets +1/+1 overall -> 5/3.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 2);
    set_counters(&mut state, boar, CounterKind::MinusOneMinusOne, 1);

    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(5));
    assert_eq!(ch.toughness, Some(3));
}

#[test]
fn minus_counters_can_drive_power_and_toughness_negative() {
    // Counters are folded verbatim; SBAs (a 0-or-less-toughness creature
    // dying, annihilation of +1/+1 vs -1/-1) are not this slice's concern,
    // so three -1/-1 on a 4/2 computes a raw 1/-1.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    set_counters(&mut state, boar, CounterKind::MinusOneMinusOne, 3);

    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(1));
    assert_eq!(ch.toughness, Some(-1));
}

#[test]
fn counters_on_a_permanent_without_pt_leave_it_without_pt() {
    // A Forest has no printed P/T; a stray +1/+1 counter does not conjure any
    // (layer 7c only adjusts an existing power/toughness).
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let forest = place(&mut state, fixture("forest"));
    set_counters(&mut state, forest, CounterKind::PlusOnePlusOne, 2);

    let ch = characteristics(&state, forest, &db);
    assert_eq!(ch.power, None);
    assert_eq!(ch.toughness, None);
}

#[test]
fn counter_count_defaults_to_zero_and_reports_stored_counts() {
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    let find = |state: &GameState| {
        state
            .battlefield
            .iter()
            .find(|p| p.id == boar)
            .unwrap()
            .clone()
    };
    assert_eq!(find(&state).counter_count(CounterKind::PlusOnePlusOne), 0);
    set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 4);
    assert_eq!(find(&state).counter_count(CounterKind::PlusOnePlusOne), 4);
    assert_eq!(find(&state).counter_count(CounterKind::MinusOneMinusOne), 0);
}

#[test]
fn basic_land_has_no_power_or_toughness_and_keeps_its_ability() {
    // Forest: a Basic Land with a mana ability and no P/T. Abilities
    // route through abilities_of, so the land's {T}: Add {G} is present.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let forest = place(&mut state, fixture("forest"));

    let ch = characteristics(&state, forest, &db);
    assert_eq!(ch.types, vec![CardType::Land]);
    assert_eq!(ch.supertypes, vec![Supertype::Basic]);
    assert_eq!(ch.power, None);
    assert_eq!(ch.toughness, None);
    assert_eq!(ch.mana_cost, "");
    assert_eq!(
        ch.abilities,
        crate::card::abilities_of(&db, fixture("forest"))
    );
    assert_eq!(ch.abilities.len(), 1);
    assert!(is_mana_ability(&ch.abilities[0]));
}

#[test]
fn unknown_permanent_id_follows_the_default_fallback() {
    // No permanent with this id is on the battlefield.
    let db = CardDatabase::bundled().unwrap();
    let state = GameState::new_two_player();
    assert!(state.battlefield.is_empty());

    assert_eq!(
        characteristics(&state, PermanentId(42), &db),
        Characteristics::default()
    );
}

#[test]
fn permanent_whose_card_is_absent_from_db_follows_the_default_fallback() {
    // The permanent exists on the battlefield, but its CardId is not in the db.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let ghost = place(&mut state, CardId(9999));
    assert!(db.card(CardId(9999)).is_none());

    assert_eq!(
        characteristics(&state, ghost, &db),
        Characteristics::default()
    );
}

#[test]
fn single_static_modifier_stacks_on_printed_pt_and_counters() {
    // Onakke Ogre is a printed 4/2. One +1/+1 counter and one static
    // +2/+2 anthem controlled by its controller compute 4+1+2 / 2+1+2 = 7/5,
    // exercising "printed + counters + modifier" together (ADR 0005 §3).
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 1);
    add_anthem(&mut state, 100, PlayerId(0), 2, 2);

    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(7));
    assert_eq!(ch.toughness, Some(5));
    // Only P/T shifts; static modifiers never touch the printed types.
    assert_eq!(ch.types, vec![CardType::Creature]);
}

#[test]
fn two_static_modifiers_apply_in_timestamp_order_and_sum() {
    // Two anthems whose sources were minted out of order in the state vector.
    // The result is their sum (they are additive), and the read path folds
    // them in ascending-timestamp order regardless of insertion order.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    // Inserted later-timestamp first to prove the pipeline sorts, not reads
    // insertion order.
    add_anthem(&mut state, 200, PlayerId(0), 0, 3); // +0/+3
    add_anthem(&mut state, 100, PlayerId(0), 4, 0); // +4/+0

    // Printed 4/2 + (+4/+0) + (+0/+3) = 8/5.
    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(8));
    assert_eq!(ch.toughness, Some(5));

    // The ordering is deterministic: ascending timestamp (source id), not the
    // Vec's insertion order.
    let perm = state.battlefield.iter().find(|p| p.id == boar).unwrap();
    let ordered: Vec<u64> = ordered_pt_modifiers(&state, perm, true, &db)
        .iter()
        .map(|effect| effect.timestamp())
        .collect();
    assert_eq!(ordered, vec![100, 200]);
}

#[test]
fn removing_the_source_reverts_the_computed_value() {
    // With the anthem in force the Ogre is a 6/4; dropping the effect (its
    // source leaving) reverts to the printed 4/2 with nothing cached to stale.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    add_anthem(&mut state, 100, PlayerId(0), 2, 2);

    let boosted = characteristics(&state, boar, &db);
    assert_eq!(boosted.power, Some(6));
    assert_eq!(boosted.toughness, Some(4));

    state.static_effects.clear();
    let reverted = characteristics(&state, boar, &db);
    assert_eq!(reverted.power, Some(4));
    assert_eq!(reverted.toughness, Some(2));
}

#[test]
fn anthem_only_affects_matching_controllers_creatures() {
    // An anthem for player 1 does not touch player 0's creature.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre")); // controlled by player 0
    add_anthem(&mut state, 100, PlayerId(1), 5, 5);

    let ch = characteristics(&state, boar, &db);
    assert_eq!(ch.power, Some(4));
    assert_eq!(ch.toughness, Some(2));
}

#[test]
fn anthem_does_not_grant_pt_to_a_noncreature() {
    // A Forest is not a creature, so a "creatures you control" anthem leaves
    // it without power/toughness (layer 7c only adjusts an existing P/T).
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let forest = place(&mut state, fixture("forest"));
    add_anthem(&mut state, 100, PlayerId(0), 2, 2);

    let ch = characteristics(&state, forest, &db);
    assert_eq!(ch.power, None);
    assert_eq!(ch.toughness, None);
}

/// Add a static "grant `keyword` to the single permanent `target`" continuous
/// effect timestamped by `source`, with the given `duration`.
fn add_keyword_grant(
    state: &mut GameState,
    source: u64,
    target: PermanentId,
    keyword: Keyword,
    duration: Duration,
) {
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(target),
        modification: Modification::GrantKeyword(keyword),
        duration,
    });
}

/// Attach the permanent `aura` to `host` (set its `attached_to`), the way a
/// resolving Aura enters (CR 303.4d).
fn attach(state: &mut GameState, aura: PermanentId, host: PermanentId) {
    let aura = state.battlefield.iter_mut().find(|p| p.id == aura).unwrap();
    aura.attached_to = Some(host);
}

/// An inline catalog for the keyword-granting Aura tests. M19 prints no Aura that
/// grants flying (its keyword-granting Aura, Prodigious Growth, grants trample
/// alongside +7/+7), so this shape is exercised by a `test_*` definition rather
/// than by a shipped card — ADR 0009's rule for an IR shape the set does not
/// cleanly represent.
fn flight_db() -> CardDatabase {
    CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_flight","name":"Test Flight",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{U}","colors":["blue"],
             "attachment":{"kind":"aura","attach_to":"any_creature","keywords":["flying"]}},
            {"schema_version":1,"functional_id":"test_ogre","name":"Test Ogre",
             "types":["creature"],"subtypes":["Ogre"],"mana_cost":"{2}{R}","colors":["red"],
             "power":4,"toughness":2}
        ]"#,
    )
    .unwrap()
}

/// An inline catalog for the restriction half of layer 6: a creature with a
/// printed restriction and an Aura that imposes two more.
fn restriction_db() -> CardDatabase {
    CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_bonds","name":"Test Bonds",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{2}{W}","colors":["white"],
             "attachment":{"kind":"aura","attach_to":"any_creature","restrictions":["cant_attack","cant_block"]}},
            {"schema_version":1,"functional_id":"test_evader","name":"Test Evader",
             "types":["creature"],"subtypes":["Horse"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":3,"restrictions":[{"cant_be_blocked_by":"black"}]}
        ]"#,
    )
    .unwrap()
}

#[test]
fn issue_606_printed_aura_and_stored_restrictions_fold_into_one_computed_set() {
    // CR 613.1f, the non-keyworded half of layer 6: the printed restriction, an
    // Aura's impositions, and a stored until-end-of-turn one all arrive in the same
    // computed list, and a bystander gets none of them.
    let db = restriction_db();
    let mut state = GameState::new_two_player();
    let host = place(&mut state, crate::fixtures::id_in(&db, "test_evader"));
    let bystander = place(&mut state, crate::fixtures::id_in(&db, "test_evader"));
    let aura = place(&mut state, crate::fixtures::id_in(&db, "test_bonds"));
    attach(&mut state, aura, host);
    state.static_effects.push(StaticEffect {
        source: 900,
        affects: EffectAffects::SpecificPermanent(host),
        modification: Modification::GrantRestriction(CombatRestriction::CantBeBlocked),
        duration: Duration::UntilEndOfTurn,
    });

    let mut computed = characteristics(&state, host, &db).restrictions;
    computed.sort_by_key(|r| format!("{r:?}"));
    assert_eq!(
        computed,
        vec![
            CombatRestriction::CantAttack,
            CombatRestriction::CantBeBlocked,
            CombatRestriction::CantBeBlockedBy(crate::mana::Color::Black),
            CombatRestriction::CantBlock,
        ]
    );
    assert_eq!(
        characteristics(&state, bystander, &db).restrictions,
        vec![CombatRestriction::CantBeBlockedBy(
            crate::mana::Color::Black
        )],
        "a bystander keeps only what it prints"
    );
}

#[test]
fn issue_606_a_restriction_imposed_twice_is_imposed_once() {
    // The idempotence keywords already have: a restriction granted on top of a
    // printed one, or granted twice, appears once — so nothing downstream can count
    // impositions instead of testing for one.
    let db = restriction_db();
    let mut state = GameState::new_two_player();
    let host = place(&mut state, crate::fixtures::id_in(&db, "test_evader"));
    for source in [901, 902] {
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(host),
            modification: Modification::GrantRestriction(CombatRestriction::CantBeBlockedBy(
                crate::mana::Color::Black,
            )),
            duration: Duration::UntilEndOfTurn,
        });
    }
    assert_eq!(
        characteristics(&state, host, &db).restrictions,
        vec![CombatRestriction::CantBeBlockedBy(
            crate::mana::Color::Black
        )]
    );
}

#[test]
fn issue_606_an_auras_restriction_vanishes_when_it_leaves() {
    // Derived from the attachment, never stored (ADR 0005): destroying the Aura is
    // the whole mechanism by which a pacified creature is freed.
    let db = restriction_db();
    let mut state = GameState::new_two_player();
    let host = place(&mut state, crate::fixtures::id_in(&db, "test_evader"));
    let aura = place(&mut state, crate::fixtures::id_in(&db, "test_bonds"));
    attach(&mut state, aura, host);
    assert!(characteristics(&state, host, &db)
        .restrictions
        .contains(&CombatRestriction::CantAttack));

    state.battlefield.retain(|p| p.id != aura);
    assert!(!characteristics(&state, host, &db)
        .restrictions
        .contains(&CombatRestriction::CantAttack));
}

#[test]
fn issue_374_aura_grants_flying_folds_into_computed_keywords_cr_613_1f() {
    // CR 613.1f: an Aura granting flying puts flying into the host's computed
    // keyword set, indistinguishable from a printed keyword. A bystander creature
    // with no Aura has none.
    let db = flight_db();
    let mut state = GameState::new_two_player();
    let host = place(&mut state, crate::fixtures::id_in(&db, "test_ogre"));
    let bystander = place(&mut state, crate::fixtures::id_in(&db, "test_ogre"));
    let aura = place(&mut state, crate::fixtures::id_in(&db, "test_flight"));
    attach(&mut state, aura, host);

    assert!(characteristics(&state, host, &db)
        .keywords
        .contains(&Keyword::Flying));
    assert!(!characteristics(&state, bystander, &db)
        .keywords
        .contains(&Keyword::Flying));
}

#[test]
fn issue_374_aura_grant_vanishes_when_the_aura_leaves() {
    // The grant is derived from the attachment (ADR 0005): detach the Aura and
    // the host's computed keyword set reverts with nothing to prune.
    let db = flight_db();
    let mut state = GameState::new_two_player();
    let host = place(&mut state, crate::fixtures::id_in(&db, "test_ogre"));
    let aura = place(&mut state, crate::fixtures::id_in(&db, "test_flight"));
    attach(&mut state, aura, host);
    assert!(characteristics(&state, host, &db)
        .keywords
        .contains(&Keyword::Flying));

    // The Aura leaves the battlefield entirely.
    state.battlefield.retain(|p| p.id != aura);
    assert!(!characteristics(&state, host, &db)
        .keywords
        .contains(&Keyword::Flying));
}

#[test]
fn issue_374_specific_permanent_grant_folds_into_computed_keywords() {
    // A pump-style "target creature gains trample" grant keyed to one permanent
    // folds into that permanent's keyword set and no other's.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let pumped = place(&mut state, fixture("onakke_ogre"));
    let bystander = place(&mut state, fixture("onakke_ogre"));
    add_keyword_grant(
        &mut state,
        100,
        pumped,
        Keyword::Trample,
        Duration::UntilEndOfTurn,
    );

    assert!(characteristics(&state, pumped, &db)
        .keywords
        .contains(&Keyword::Trample));
    assert!(!characteristics(&state, bystander, &db)
        .keywords
        .contains(&Keyword::Trample));
}

#[test]
fn issue_374_anthem_grant_affects_only_matching_controllers_creatures() {
    // A "creatures you control have vigilance" grant applies to a matching
    // controller's creature and not to an opponent's.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let mine = place(&mut state, fixture("onakke_ogre")); // controller PlayerId(0)
    state.static_effects.push(StaticEffect {
        source: 100,
        affects: EffectAffects::CreaturesControlledBy(PlayerId(0)),
        modification: Modification::GrantKeyword(Keyword::Vigilance),
        duration: Duration::WhileOnBattlefield,
    });
    assert!(characteristics(&state, mine, &db)
        .keywords
        .contains(&Keyword::Vigilance));

    // A creature the effect's controller does not control is untouched.
    state.static_effects[0].affects = EffectAffects::CreaturesControlledBy(PlayerId(1));
    assert!(!characteristics(&state, mine, &db)
        .keywords
        .contains(&Keyword::Vigilance));
}

#[test]
fn issue_374_duplicate_keyword_grants_are_redundant_not_stacking() {
    // CR 702: having a keyword twice is the same as having it once. A printed
    // flier (Snapping Drake) also granted flying twice appears with flying
    // exactly once — the grants collapse.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let drake = place(&mut state, fixture("snapping_drake")); // printed flying
    add_keyword_grant(
        &mut state,
        100,
        drake,
        Keyword::Flying,
        Duration::UntilEndOfTurn,
    );
    add_keyword_grant(
        &mut state,
        200,
        drake,
        Keyword::Flying,
        Duration::WhileOnBattlefield,
    );

    let ch = characteristics(&state, drake, &db);
    assert_eq!(
        ch.keywords
            .iter()
            .filter(|&&kw| kw == Keyword::Flying)
            .count(),
        1,
        "flying is present once despite a printed copy and two grants"
    );
}

#[test]
fn recomputes_fresh_and_never_mutates_state() {
    // Two calls agree and the state is untouched — the function is a pure query.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let boar = place(&mut state, fixture("onakke_ogre"));
    let before = state.clone();

    let first = characteristics(&state, boar, &db);
    let second = characteristics(&state, boar, &db);
    assert_eq!(first, second);
    assert_eq!(state, before);
}

// ---------------------------------------------------------------------
// Printed static abilities (CR 604.3): anthems and lords
// ---------------------------------------------------------------------

/// A catalog with an anthem, a subtype lord, a keyword granter, and two
/// creatures — one Elf, one not — to point them at.
fn lords_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_anthem","name":"Test Anthem",
         "types":["enchantment"],"subtypes":[],"mana_cost":"{1}{W}","colors":["white"],
         "abilities":[{"type":"static",
           "affects":{"scope":"creatures_you_control"},
           "modification":{"kind":"power_toughness","power":1,"toughness":1}}]},
        {"schema_version":1,"functional_id":"test_elf_lord","name":"Test Elf Lord",
         "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1,
         "abilities":[{"type":"static",
           "affects":{"scope":"creatures_you_control","subtype":"Elf","except_this":true},
           "modification":{"kind":"power_toughness","power":1,"toughness":1}}]},
        {"schema_version":1,"functional_id":"test_banner","name":"Test Banner",
         "types":["enchantment"],"subtypes":[],"mana_cost":"{2}{W}","colors":["white"],
         "abilities":[{"type":"static",
           "affects":{"scope":"creatures_you_control"},
           "modification":{"kind":"grant_keyword","keyword":"vigilance"}}]},
        {"schema_version":1,"functional_id":"test_elf","name":"Test Elf",
         "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
         "power":2,"toughness":2},
        {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
         "types":["creature"],"subtypes":["Bear"],"mana_cost":"{1}{G}","colors":["green"],
         "power":2,"toughness":2}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// Place a permanent for `card` controlled by `controller`.
fn place_for(state: &mut GameState, card: CardId, controller: PlayerId) -> PermanentId {
    let id = place(state, card);
    state
        .battlefield
        .iter_mut()
        .find(|p| p.id == id)
        .unwrap()
        .controller = controller;
    id
}

#[test]
fn cr_604_3_an_anthem_pumps_its_controllers_creatures() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));

    assert_eq!(characteristics(&state, bear, &db).power, Some(3));
    assert_eq!(characteristics(&state, bear, &db).toughness, Some(3));
}

#[test]
fn cr_604_3_an_anthem_leaves_an_opponents_creatures_alone() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let theirs = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(1));
    place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));

    // "Creatures you control" is the controller of the *source*, not of the
    // permanent being measured.
    assert_eq!(characteristics(&state, theirs, &db).power, Some(2));
}

#[test]
fn cr_604_3_an_anthem_does_not_give_a_noncreature_power() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let anthem = place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));

    // Layer 7c only adjusts an existing power/toughness; an enchantment has none
    // and must not acquire one by being adjacent to an anthem.
    assert_eq!(characteristics(&state, anthem, &db).power, None);
    assert_eq!(characteristics(&state, anthem, &db).toughness, None);
}

#[test]
fn cr_604_3_a_lord_pumps_only_its_named_subtype() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let elf = place_for(&mut state, id_in(&db, "test_elf"), PlayerId(0));
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_elf_lord"), PlayerId(0));

    assert_eq!(characteristics(&state, elf, &db).power, Some(3));
    // The Bear is a creature its controller controls, but it is not an Elf.
    assert_eq!(characteristics(&state, bear, &db).power, Some(2));
}

#[test]
fn cr_604_3_a_lord_does_not_pump_itself() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let lord = place_for(&mut state, id_in(&db, "test_elf_lord"), PlayerId(0));

    // "*Other* Elves you control" — the source is excluded, so a lone lord is
    // exactly its printed size.
    assert_eq!(characteristics(&state, lord, &db).power, Some(1));
}

#[test]
fn cr_604_3_two_lords_pump_each_other() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let first = place_for(&mut state, id_in(&db, "test_elf_lord"), PlayerId(0));
    let second = place_for(&mut state, id_in(&db, "test_elf_lord"), PlayerId(0));

    // "Other" excludes the specific object, not the card: each is "other" to the
    // other one, so both are 2/2. Comparing by card would leave both at 1/1.
    assert_eq!(characteristics(&state, first, &db).power, Some(2));
    assert_eq!(characteristics(&state, second, &db).power, Some(2));
}

#[test]
fn cr_604_3_anthems_stack() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));

    assert_eq!(characteristics(&state, bear, &db).power, Some(4));
}

#[test]
fn cr_613_1f_a_static_ability_grants_a_keyword() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_banner"), PlayerId(0));

    assert!(characteristics(&state, bear, &db)
        .keywords
        .contains(&Keyword::Vigilance));
}

#[test]
fn cr_604_3_the_effect_ends_the_instant_its_source_leaves() {
    // The load-bearing property of deriving rather than storing: nothing was
    // pushed when the anthem entered, so nothing has to be pruned when it goes.
    // A stored effect that outlived its source would leave a permanently buffed
    // board, and would only be caught by whatever pruning pass someone remembered
    // to write.
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    let anthem = place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));
    assert_eq!(characteristics(&state, bear, &db).power, Some(3));

    state.battlefield.retain(|p| p.id != anthem);

    assert_eq!(characteristics(&state, bear, &db).power, Some(2));
    assert!(
        state.static_effects.is_empty(),
        "a printed static ability must never enter stored state"
    );
}

#[test]
fn cr_613_7c_an_anthem_folds_together_with_counters_and_a_pump() {
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));
    set_counters(&mut state, bear, CounterKind::PlusOnePlusOne, 2);
    add_pump(&mut state, 9_000, bear, 3, 0);

    // 2/2 printed, +2/+2 in counters, +1/+1 from the anthem, +3/+0 from the pump.
    assert_eq!(characteristics(&state, bear, &db).power, Some(8));
    assert_eq!(characteristics(&state, bear, &db).toughness, Some(5));
}

// ----- CR 613 layer 2: control -----------------------------------------------

/// Give control of `target` to `player`, timestamped by `source` (its object id).
fn add_control_change(state: &mut GameState, source: u64, target: PermanentId, player: PlayerId) {
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(target),
        modification: Modification::GainControl(player),
        duration: Duration::UntilEndOfTurn,
    });
}

#[test]
fn cr_613_layer_2_the_latest_control_change_is_the_one_that_answers() {
    // Layer 2 is ordered by CR 613.7 like any other, so two effects in force resolve to
    // the later timestamp — and removing it leaves the earlier one answering, with
    // nothing to recompute. That is the whole reason control is derived rather than
    // written onto the permanent.
    let mut state = GameState::new_two_player();
    let ogre = place_for(&mut state, fixture("onakke_ogre"), PlayerId(0));
    assert_eq!(controller_of_id(&state, ogre), Some(PlayerId(0)));

    add_control_change(&mut state, 100, ogre, PlayerId(1));
    assert_eq!(controller_of_id(&state, ogre), Some(PlayerId(1)));

    add_control_change(&mut state, 200, ogre, PlayerId(0));
    assert_eq!(controller_of_id(&state, ogre), Some(PlayerId(0)));

    state.static_effects.retain(|effect| effect.source != 200);
    assert_eq!(
        controller_of_id(&state, ogre),
        Some(PlayerId(1)),
        "the effect underneath applies again on its own"
    );

    state.static_effects.clear();
    assert_eq!(controller_of_id(&state, ogre), Some(PlayerId(0)));
    assert_eq!(
        controller_of_id(&state, PermanentId(9_999)),
        None,
        "a permanent that is not on the battlefield has no controller"
    );
}

#[test]
fn cr_613_layer_2_is_applied_before_the_anthem_that_reads_it() {
    // Layer 2 comes before layer 7c, so "creatures you control" is read against the
    // *new* controller: an anthem lets go of a creature that has been taken and picks up
    // one that has been given.
    let db = lords_db();
    let mut state = GameState::new_two_player();
    let bear = place_for(&mut state, id_in(&db, "test_bear"), PlayerId(0));
    place_for(&mut state, id_in(&db, "test_anthem"), PlayerId(0));
    assert_eq!(characteristics(&state, bear, &db).power, Some(3));

    add_control_change(&mut state, 100, bear, PlayerId(1));
    assert_eq!(
        characteristics(&state, bear, &db).power,
        Some(2),
        "the anthem's controller no longer controls it"
    );

    // The other direction: the anthem itself changes hands, and its "you" moves with it.
    let mut swapped = GameState::new_two_player();
    let theirs = place_for(&mut swapped, id_in(&db, "test_bear"), PlayerId(1));
    let anthem = place_for(&mut swapped, id_in(&db, "test_anthem"), PlayerId(0));
    assert_eq!(characteristics(&swapped, theirs, &db).power, Some(2));
    add_control_change(&mut swapped, 100, anthem, PlayerId(1));
    assert_eq!(
        characteristics(&swapped, theirs, &db).power,
        Some(3),
        "a stolen lord speaks for the seat holding it"
    );
}
