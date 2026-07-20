#![cfg(test)]
#![allow(clippy::unwrap_used)]

use super::*;

// Re-export every engine name the moved tests reference. The original inline test
// module reached these through `use super::*` over the (then much larger) parent
// import list plus a couple of interior `use` aliases; funneling them through this
// shared support module lets each split-out `mod tests` pick them up with a single
// glob (`use crate::apply::test_support::*`).
pub(crate) use crate::ability::{Effect, Target};
pub(crate) use crate::actions::{
    valid_actions, valid_actions as valid, Action, Attack, Block, DamageOrder,
};
pub(crate) use crate::card::Keyword;
pub(crate) use crate::combat::{attacker_candidates, blocker_candidates, pending_blocker_declarer};
pub(crate) use crate::fixtures::{fixture, id_in};
pub(crate) use crate::id::{CardId, CardInstance, PermanentId, PlayerId};
pub(crate) use crate::mana::Color;
pub(crate) use crate::phase::Step;
pub(crate) use crate::player::{LossReason, MAX_HAND_SIZE};
pub(crate) use crate::stack::{StackId, StackObject, StackObjectKind};
pub(crate) use crate::state::{
    Duration, EffectAffects, GameState, Modification, Permanent, StaticEffect,
};
pub(crate) use crate::CardDatabase;

/// The bundled card database, for tests that need oracle data.
pub(crate) fn db() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// An inline combat catalog whose bodies carry the exact P/T and keywords the
/// old invented combat fixtures had, so the damage/life arithmetic the combat
/// tests assert stays unchanged. First strike, deathtouch, trample, lifelink and
/// a bare "when this dies, draw" have no clean M19 representative, so the combat
/// and dies-trigger tests build their own definitions (ADR 0025).
pub(crate) fn combat_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
         "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
         "power":3,"toughness":2},
        {"schema_version":1,"functional_id":"test_basilisk","name":"Test Basilisk",
         "types":["creature"],"subtypes":["Basilisk"],"mana_cost":"{4}{G}","colors":["green"],
         "power":4,"toughness":5},
        {"schema_version":1,"functional_id":"test_otter","name":"Test Otter",
         "types":["creature"],"subtypes":["Otter"],"mana_cost":"{1}{U}","colors":["blue"],
         "power":1,"toughness":3},
        {"schema_version":1,"functional_id":"test_duelist","name":"Test Duelist",
         "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{1}{W}","colors":["white"],
         "power":2,"toughness":2,"keywords":["first_strike"]},
        {"schema_version":1,"functional_id":"test_adder","name":"Test Adder",
         "types":["creature"],"subtypes":["Snake"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1,"keywords":["deathtouch"]},
        {"schema_version":1,"functional_id":"test_trampler","name":"Test Trampler",
         "types":["creature"],"subtypes":["Beast"],"mana_cost":"{4}{G}","colors":["green"],
         "power":5,"toughness":4,"keywords":["trample"]},
        {"schema_version":1,"functional_id":"test_baneclaw","name":"Test Baneclaw",
         "types":["creature"],"subtypes":["Beast"],"mana_cost":"{2}{G}{G}","colors":["green"],
         "power":4,"toughness":4,"keywords":["trample","deathtouch"]},
        {"schema_version":1,"functional_id":"test_lifelinker","name":"Test Lifelinker",
         "types":["creature"],"subtypes":["Cleric"],"mana_cost":"{2}{W}","colors":["white"],
         "power":2,"toughness":3,"keywords":["lifelink"]},
        {"schema_version":1,"functional_id":"test_lurker","name":"Test Lurker",
         "types":["creature"],"subtypes":["Horror"],"mana_cost":"{1}{B}","colors":["black"],
         "power":2,"toughness":2,
         "abilities":[{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}]},
        {"schema_version":1,"functional_id":"test_twinstrike","name":"Test Twinstrike",
         "types":["creature"],"subtypes":["Cat"],"mana_cost":"{2}{W}","colors":["white"],
         "power":2,"toughness":2,"keywords":["double_strike"]},
        {"schema_version":1,"functional_id":"test_twintrample","name":"Test Twintrample",
         "types":["creature"],"subtypes":["Beast"],"mana_cost":"{3}{G}","colors":["green"],
         "power":3,"toughness":3,"keywords":["double_strike","trample"]},
        {"schema_version":1,"functional_id":"test_ward","name":"Test Ward",
         "types":["creature"],"subtypes":["Soldier"],"mana_cost":"{2}{W}","colors":["white"],
         "power":3,"toughness":3,"keywords":["first_strike"]},
        {"schema_version":1,"functional_id":"test_twinsoldier","name":"Test Twinsoldier",
         "types":["creature"],"subtypes":["Cat","Soldier"],"mana_cost":"{2}{W}{W}","colors":["white"],
         "power":3,"toughness":4,"keywords":["double_strike"]},
        {"schema_version":1,"functional_id":"test_twinjugg","name":"Test Twinjugg",
         "types":["creature"],"subtypes":["Beast"],"mana_cost":"{4}{G}","colors":["green"],
         "power":5,"toughness":5,"keywords":["double_strike","trample"]}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// A two-player game in the precombat main phase with player 0 holding a
/// Forest and Llanowar Elves, and one card to draw in the library. Each card
/// is a freshly minted [`CardInstance`] so copies stay distinguishable.
pub(crate) fn slice_state() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let forest = state.new_instance(fixture("forest"));
    let scout = state.new_instance(fixture("llanowar_elves"));
    let draw = state.new_instance(fixture("onakke_ogre"));
    state.players[0].hand = vec![forest, scout];
    state.players[0].library = vec![draw];
    state
}

/// The first hand instance in `seat`'s hand whose printed card is `card`.
pub(crate) fn hand_instance(state: &GameState, seat: usize, card: CardId) -> CardInstance {
    *state.players[seat]
        .hand
        .iter()
        .find(|c| c.card == card)
        .unwrap()
}

/// Put a permanent of `card` on the battlefield under `controller`, with the
/// given tapped and marked-damage state; returns its fresh id.
pub(crate) fn place_permanent(
    state: &mut GameState,
    card: CardId,
    controller: PlayerId,
    tapped: bool,
    damage: u32,
) -> PermanentId {
    let inst = state.new_instance(card);
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        instance: inst.id,
        card,
        controller,
        tapped,
        entered_turn: 0,
        attacking: None,
        blocking: None,
        damage,
        counters: Default::default(),
        attached_to: None,
    });
    PermanentId(id)
}

/// Borrow the permanent with id `id`; panics if it is gone.
pub(crate) fn find_perm(state: &GameState, id: PermanentId) -> &Permanent {
    state.battlefield.iter().find(|p| p.id == id).unwrap()
}

/// Both seats pass priority in succession, ending the current step.
pub(crate) fn pass_full_round(state: &GameState, db: &CardDatabase) -> GameState {
    let s = apply_action(state, &Action::PassPriority, db);
    apply_action(&s, &Action::PassPriority, db)
}

/// Push an "until end of turn" pump of +`power`/+`toughness` onto `target`,
/// timestamped by a freshly minted object id, and return that id.
pub(crate) fn pump(state: &mut GameState, target: PermanentId, power: i32, toughness: i32) -> u64 {
    let source = state.mint_id();
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(target),
        modification: Modification::PowerToughness { power, toughness },
        duration: Duration::UntilEndOfTurn,
    });
    source
}

/// Push an "until end of turn" grant of `keyword` onto `target`, timestamped by
/// a freshly minted object id, and return that id.
pub(crate) fn grant_keyword(state: &mut GameState, target: PermanentId, keyword: Keyword) -> u64 {
    let source = state.mint_id();
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(target),
        modification: Modification::GrantKeyword(keyword),
        duration: Duration::UntilEndOfTurn,
    });
    source
}

/// A two-player game paused at the declare-attackers step, turn 2 so that
/// permanents which entered on turn 0/1 are free of summoning sickness. Player
/// 0 is the active/attacking player, player 1 the defender.
pub(crate) fn at_declare_attackers() -> GameState {
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.step = Step::DeclareAttackers;
    state.active_player = PlayerId(0);
    state.priority = PlayerId(0);
    state
}

/// Mark the permanent `id` as having entered on turn `turn` (its summoning-
/// sickness clock).
pub(crate) fn set_entered_turn(state: &mut GameState, id: PermanentId, turn: u32) {
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
        perm.entered_turn = turn;
    }
}

/// Whether a permanent id is still on the battlefield.
pub(crate) fn alive(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

/// Wrap plain attacker ids as attacks on the sole opponent, seat 1 — the
/// two-player default these combat tests exercise (issue #341).
pub(crate) fn atk1(ids: &[PermanentId]) -> Vec<Attack> {
    ids.iter()
        .map(|&attacker| Attack {
            attacker,
            defender: PlayerId(1),
        })
        .collect()
}

/// Drive combat from the declare-attackers step through the combat-damage
/// step: declare `attackers`, pass to declare-blockers, declare `blocks`, then
/// pass into combat damage (where the turn-based damage assignment runs and the
/// state-based-actions loop resolves). Returns the state paused at
/// [`Step::CombatDamage`].
pub(crate) fn run_combat(
    state: &GameState,
    attackers: Vec<PermanentId>,
    blocks: Vec<Block>,
    db: &CardDatabase,
) -> GameState {
    let state = apply_action(
        state,
        &Action::DeclareAttackers {
            attackers: atk1(&attackers),
        },
        db,
    );
    let state = pass_full_round(&state, db);
    assert_eq!(state.step, Step::DeclareBlockers);
    let state = apply_action(&state, &Action::DeclareBlockers { blocks }, db);
    // Issue #346: a multi-blocked attacker owes a combat-damage assignment order
    // before the priority round; submit the battlefield-order default so these
    // tests keep exercising the pre-#346 assignment order.
    let state = match default_damage_order(&state) {
        Some(order) => apply_action(&state, &order, db),
        None => state,
    };
    let state = pass_full_round(&state, db);
    assert_eq!(state.step, Step::CombatDamage);
    state
}

/// The battlefield-order `OrderCombatDamage` for every attacker that owes an
/// order (issue #346), or `None` when none does. The deterministic default the
/// server also falls back to on a timeout.
pub(crate) fn default_damage_order(state: &GameState) -> Option<Action> {
    let owed = crate::combat::attackers_needing_damage_order(state);
    if owed.is_empty() {
        return None;
    }
    let orders = owed
        .into_iter()
        .map(|attacker| DamageOrder {
            attacker,
            blockers: state
                .battlefield
                .iter()
                .filter(|p| p.blocking == Some(attacker))
                .map(|p| p.id)
                .collect(),
        })
        .collect();
    Some(Action::OrderCombatDamage { orders })
}

/// A precombat-main two-player game with player 0 the active player holding
/// priority — so player 0 may cast at both instant and sorcery speed, an empty
/// stack in front of it. Player 0 is the caster in the tests below.
pub(crate) fn main_phase_p0() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state
}

/// Drive from declare-attackers up to — but not into — combat damage, recording
/// the attacking player's chosen combat-damage assignment `orders` (issue #346).
/// Returns the state paused in declare-blockers with `state.damage_orders` set, so
/// a test can step the first-strike and regular damage steps itself and observe
/// each one (the survivor set can differ between them under double strike).
pub(crate) fn ordered_before_damage(
    state: &GameState,
    attackers: Vec<PermanentId>,
    blocks: Vec<Block>,
    orders: Vec<DamageOrder>,
    db: &CardDatabase,
) -> GameState {
    let state = apply_action(
        state,
        &Action::DeclareAttackers {
            attackers: atk1(&attackers),
        },
        db,
    );
    let state = pass_full_round(&state, db);
    assert_eq!(state.step, Step::DeclareBlockers);
    let state = apply_action(&state, &Action::DeclareBlockers { blocks }, db);
    let state = apply_action(&state, &Action::OrderCombatDamage { orders }, db);
    assert_eq!(
        state.step,
        Step::DeclareBlockers,
        "not yet in combat damage"
    );
    state
}

/// Like [`run_combat`], but submits the attacking player's chosen `orders`
/// (issue #346) instead of the battlefield-order default, then runs the whole
/// combat-damage step through the real `apply_action` pipeline. Returns the state
/// paused at [`Step::CombatDamage`].
pub(crate) fn run_combat_ordered(
    state: &GameState,
    attackers: Vec<PermanentId>,
    blocks: Vec<Block>,
    orders: Vec<DamageOrder>,
    db: &CardDatabase,
) -> GameState {
    let state = ordered_before_damage(state, attackers, blocks, orders, db);
    let state = pass_full_round(&state, db);
    assert_eq!(state.step, Step::CombatDamage);
    state
}

/// Push a player-0-controlled ability with `effects` (aimed at `targets`) onto
/// the stack, one full priority round from resolving. Mirrors how a real cast
/// or activation seats an ability, so the whole death-then-trigger pipeline is
/// driven through the public `apply_action` path.
pub(crate) fn push_ability(
    state: &mut GameState,
    source: PermanentId,
    effects: Vec<Effect>,
    targets: Vec<Target>,
) {
    let id = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(id),
        controller: PlayerId(0),
        kind: StackObjectKind::Ability { source, effects },
        targets,
    });
}

/// A precombat-main two-player game with the dies fixture (`test_lurker`, a
/// 2/2 with a self-dies draw) on the battlefield under player 0 with `damage`
/// marked, and a single card in player 0's library to draw. Cards are resolved
/// from `db` (a [`combat_db`]). Returns the state and the lurker's id.
pub(crate) fn state_with_lurker(
    db: &CardDatabase,
    damage: u32,
) -> (GameState, PermanentId, CardInstance) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let lurker = place_permanent(
        &mut state,
        id_in(db, "test_lurker"),
        PlayerId(0),
        false,
        damage,
    );
    let draw = state.new_instance(id_in(db, "test_boar"));
    state.players[0].library = vec![draw];
    (state, lurker, draw)
}

/// A 3-seat state parked at declare-blockers: seat 0 attacks seat 1 (with
/// attacker A) and seat 2 (with attacker B), each defender has one untapped
/// blocker, and priority sits with the first attacked player to declare.
pub(crate) fn split_combat_at_declare_blockers() -> (
    GameState,
    PermanentId,
    PermanentId,
    PermanentId,
    PermanentId,
) {
    let mut state = GameState::new_multiplayer(3);
    state.turn = 2;
    state.step = Step::DeclareBlockers;
    state.active_player = PlayerId(0);
    state.attackers_declared = true;
    let atk_a = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, 0);
    let atk_b = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, 0);
    for (id, defender) in [(atk_a, PlayerId(1)), (atk_b, PlayerId(2))] {
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == id)
            .unwrap()
            .attacking = Some(defender);
    }
    let blk1 = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let blk2 = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(2), false, 0);
    state.priority = pending_blocker_declarer(&state).unwrap();
    (state, atk_a, atk_b, blk1, blk2)
}

/// Big vanilla commanders whose exact power sets how hard each hit lands, so the
/// commander-damage arithmetic is unambiguous (ADR 0025/0026 — no clean M19 body).
pub(crate) fn commander_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_general","name":"Test General",
         "types":["creature"],"subtypes":["Giant"],"mana_cost":"{5}{G}","colors":["green"],
         "power":7,"toughness":7},
        {"schema_version":1,"functional_id":"test_marshal","name":"Test Marshal",
         "types":["creature"],"subtypes":["Giant"],"mana_cost":"{5}{R}","colors":["red"],
         "power":10,"toughness":10},
        {"schema_version":1,"functional_id":"test_captain","name":"Test Captain",
         "types":["creature"],"subtypes":["Giant"],"mana_cost":"{5}{W}","colors":["white"],
         "power":11,"toughness":11},
        {"schema_version":1,"functional_id":"test_warlord","name":"Test Warlord",
         "types":["creature"],"subtypes":["Giant"],"mana_cost":"{5}{B}","colors":["black"],
         "power":21,"toughness":21}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// Put a fresh permanent for the physical commander `instance` on the
/// battlefield under `controller`, attacking `defender`. A fresh `PermanentId`
/// each call models a recast / battlefield re-entry of the *same* instance.
pub(crate) fn place_commander_permanent(
    state: &mut GameState,
    card: CardId,
    instance: crate::id::CardInstanceId,
    controller: PlayerId,
    defender: PlayerId,
) -> PermanentId {
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        card,
        controller,
        tapped: false,
        entered_turn: 0,
        attacking: Some(defender),
        blocking: None,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
    });
    id
}

/// Designate `card` as `controller`'s commander (CR 903.3) and place it on the
/// battlefield attacking `defender`. Returns the permanent id and the stable
/// commander instance id (which outlives every permanent it becomes).
pub(crate) fn place_commander_attacker(
    state: &mut GameState,
    card: CardId,
    controller: PlayerId,
    defender: PlayerId,
) -> (PermanentId, crate::id::CardInstanceId) {
    let instance = state.new_instance(card);
    state.players[controller.0].commander =
        Some(crate::commander::CommanderState::new(card, instance.id));
    let pid = place_commander_permanent(state, card, instance.id, controller, defender);
    (pid, instance.id)
}
