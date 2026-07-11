//! Deriving personalized [`rune_protocol`] views from engine [`GameState`].
//!
//! This is the engine→protocol shim for the room task (issue #31). The engine
//! speaks in seat indices and card ids; the wire protocol speaks in opaque string
//! entity ids and redacts hidden zones. Everything here is a pure translation of
//! an already-computed state — it holds **no game logic** (the engine owns
//! legality and effects) and does no I/O. It only decides what one seat is allowed
//! to see and how to name the entities the engine already produced.
//!
//! Kept inside `rune-server` deliberately: it maps between two crates the server
//! already depends on, and adds nothing to the wire contract in `rune-protocol`.

use rune_engine::{
    valid_actions, Action, CardData, CardDatabase, CardId, CardInstance, CardInstanceId, Effect,
    GameState, PermanentId, Player, PlayerId, StackId, StackObject, StackObjectKind, Step,
};
use rune_protocol::{
    CardView, GameView, OpponentView, Permanent as PermanentView, Phase, StackItem, ValidAction,
    ZonePile,
};

/// The opaque protocol id for a seat (an engine [`PlayerId`]).
fn player_id(seat: PlayerId) -> String {
    format!("p{}", seat.0)
}

/// The opaque protocol id for a card referenced from a hand or a public pile.
///
/// Keyed by the per-copy [`CardInstanceId`], so two copies of the same printing
/// get distinct entity ids (`card_5` vs `card_6`) and the action a client echoes
/// back names an unambiguous instance — the engine no longer falls back to "the
/// first matching copy".
fn card_entity_id(instance: CardInstanceId) -> String {
    format!("card_{}", instance.0)
}

/// The opaque protocol id for a permanent on the battlefield.
fn permanent_entity_id(id: PermanentId) -> String {
    format!("perm_{}", id.0)
}

/// The opaque protocol id for an object on the stack.
fn stack_entity_id(id: StackId) -> String {
    format!("stack_{}", id.0)
}

/// Saturating `usize`→`u32` for wire counts; avoids both a panic and a lossy cast.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Map the engine's turn [`Step`] onto the protocol [`Phase`]. The two enums are
/// deliberately decoupled (`rune-engine` never depends on `rune-protocol`), so the
/// mapping is written out here.
fn phase_of(step: Step) -> Phase {
    match step {
        Step::Untap => Phase::Untap,
        Step::Upkeep => Phase::Upkeep,
        Step::Draw => Phase::Draw,
        Step::PrecombatMain => Phase::PrecombatMain,
        Step::BeginCombat => Phase::BeginCombat,
        Step::DeclareAttackers => Phase::DeclareAttackers,
        Step::DeclareBlockers => Phase::DeclareBlockers,
        Step::CombatDamage => Phase::CombatDamage,
        Step::EndCombat => Phase::EndCombat,
        Step::PostcombatMain => Phase::PostcombatMain,
        Step::End => Phase::End,
        Step::Cleanup => Phase::Cleanup,
    }
}

/// The display name of a card, or a stable placeholder if the id is unknown.
fn card_name(card: CardId, db: &CardDatabase) -> String {
    db.card(card)
        .map(|data| data.name.clone())
        .unwrap_or_else(|| format!("Unknown card {}", card.0))
}

/// Build the full [`CardView`] for a card the viewer is entitled to see.
fn card_view(entity_id: String, card: CardId, db: &CardDatabase) -> CardView {
    match db.card(card) {
        Some(data) => full_card_view(entity_id, data),
        None => CardView {
            id: entity_id,
            name: format!("Unknown card {}", card.0),
            type_line: String::new(),
            mana_cost: None,
            oracle_text: String::new(),
            power: None,
            toughness: None,
        },
    }
}

/// Project engine [`CardData`] onto the wire [`CardView`]. Power/toughness become
/// strings so non-numeric values round-trip (`rune-protocol`); an empty mana cost
/// is elided rather than sent as `""`.
fn full_card_view(entity_id: String, data: &CardData) -> CardView {
    CardView {
        id: entity_id,
        name: data.name.clone(),
        type_line: data.type_line(),
        mana_cost: (!data.mana_cost.is_empty()).then(|| data.mana_cost.clone()),
        oracle_text: data.oracle_text.clone(),
        power: data.power.map(|p| p.to_string()),
        toughness: data.toughness.map(|t| t.to_string()),
    }
}

/// A short human description of an ability's effects, for the stack view.
fn ability_description(effects: &[Effect]) -> String {
    let parts: Vec<String> = effects
        .iter()
        .map(|effect| match effect {
            Effect::AddMana { color, amount } => format!("Add {} {}", amount, color.pip()),
            Effect::DrawCard { count } => format!("Draw {count} card(s)"),
        })
        .collect();
    if parts.is_empty() {
        "Ability".to_string()
    } else {
        parts.join(", ")
    }
}

/// Project one engine [`StackObject`] onto its wire [`StackItem`].
fn stack_item(object: &StackObject, db: &CardDatabase) -> StackItem {
    match &object.kind {
        StackObjectKind::Spell { card } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: card_name(card.card, db),
            source: None,
        },
        StackObjectKind::Ability { source, effects } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: ability_description(effects),
            source: Some(permanent_entity_id(*source)),
        },
    }
}

/// Build the [`ZonePile`]s for a public per-player pile (graveyard or exile),
/// skipping empty piles so the wire stays terse.
fn zone_piles(
    state: &GameState,
    pick: impl Fn(&Player) -> &Vec<CardInstance>,
    db: &CardDatabase,
) -> Vec<ZonePile> {
    state
        .players
        .iter()
        .enumerate()
        .filter_map(|(seat, player)| {
            let cards = pick(player);
            if cards.is_empty() {
                return None;
            }
            Some(ZonePile {
                player_id: player_id(PlayerId(seat)),
                cards: cards
                    .iter()
                    .map(|&inst| card_view(card_entity_id(inst.id), inst.card, db))
                    .collect(),
            })
        })
        .collect()
}

/// The actions the engine currently offers the priority holder, each paired with
/// the opaque id a client echoes back to choose it.
///
/// The ids are positional and therefore deterministic: recomputing this list from
/// the same [`GameState`] yields the identical id→action mapping. That is what
/// lets the room resolve a returned `action_id` (see [`resolve_action`]) without
/// storing any per-connection state — the full-state invariant applies to routing
/// too. Empty when no one holds priority.
fn issued_actions(state: &GameState, db: &CardDatabase) -> Vec<(String, Action)> {
    valid_actions(state, db)
        .into_iter()
        .enumerate()
        .map(|(index, action)| (format!("a{index}"), action))
        .collect()
}

/// Project one engine [`Action`] onto its wire [`ValidAction`], attaching the
/// subject entity so the client can render the action on the card/permanent it
/// belongs to (ADR 0004).
fn valid_action_view(id: String, action: &Action, db: &CardDatabase) -> ValidAction {
    match action {
        Action::PassPriority => ValidAction {
            id,
            kind: "pass_priority".to_string(),
            label: "Pass priority".to_string(),
            subject: Vec::new(),
        },
        Action::PlayLand { card } => ValidAction {
            id,
            kind: "play_land".to_string(),
            label: format!("Play {}", card_name(card.card, db)),
            subject: vec![card_entity_id(card.id)],
        },
        Action::CastSpell { card } => ValidAction {
            id,
            kind: "cast_spell".to_string(),
            label: format!("Cast {}", card_name(card.card, db)),
            subject: vec![card_entity_id(card.id)],
        },
        Action::ActivateAbility { permanent, .. } => ValidAction {
            id,
            kind: "activate_ability".to_string(),
            label: "Activate ability".to_string(),
            subject: vec![permanent_entity_id(*permanent)],
        },
    }
}

/// Build the [`GameView`] the seat `viewer` is entitled to see.
///
/// Hidden information is redacted: the viewer receives full [`CardView`]s only for
/// their own hand and mana pool; every other seat is reduced to an
/// [`OpponentView`] of public counts (hand size, life, library and graveyard
/// sizes). Public state — battlefield, stack, graveyards, exile, phase — is shared
/// verbatim. `valid_actions` are populated only when `viewer` holds priority,
/// because the engine offers actions to exactly one seat at a time.
///
/// The view names its receiver in `you` (the viewer's `p{N}` seat id), so the
/// client identifies itself directly rather than inferring it from the zones.
///
/// A view is a complete snapshot: a client can reconstruct its entire UI from this
/// one message, which is what makes reconnect a plain re-send (`docs/protocol.md`).
pub(crate) fn personalized_view(
    state: &GameState,
    db: &CardDatabase,
    viewer: PlayerId,
) -> GameView {
    let my_hand = state
        .players
        .get(viewer.0)
        .map(|player| {
            player
                .hand
                .iter()
                .map(|&inst| card_view(card_entity_id(inst.id), inst.card, db))
                .collect()
        })
        .unwrap_or_default();

    let opponents = state
        .players
        .iter()
        .enumerate()
        .filter(|(seat, _)| *seat != viewer.0)
        .map(|(seat, player)| OpponentView {
            player_id: player_id(PlayerId(seat)),
            hand_size: count(player.hand.len()),
            life: player.life,
            library_size: count(player.library.len()),
            graveyard_size: count(player.graveyard.len()),
            statuses: Vec::new(),
        })
        .collect();

    let battlefield = state
        .battlefield
        .iter()
        .map(|perm| PermanentView {
            id: permanent_entity_id(perm.id),
            controller: player_id(perm.controller),
            // The engine models control but not separate ownership yet, so owner
            // mirrors controller until zone-ownership lands.
            owner: player_id(perm.controller),
            card: card_view(permanent_entity_id(perm.id), perm.card, db),
            tapped: perm.tapped,
            counters: Vec::new(),
        })
        .collect();

    let stack = state.stack.iter().map(|o| stack_item(o, db)).collect();
    let graveyards = zone_piles(state, |p| &p.graveyard, db);
    let exile = zone_piles(state, |p| &p.exile, db);

    let mana_pool = state
        .players
        .get(viewer.0)
        .map(|player| player.mana_pool.pips())
        .unwrap_or_default();

    let holds_priority = state.priority_holder().is_some();
    let priority_player = holds_priority.then(|| player_id(state.priority));

    let valid_actions = if holds_priority && state.priority == viewer {
        issued_actions(state, db)
            .into_iter()
            .map(|(id, action)| valid_action_view(id, &action, db))
            .collect()
    } else {
        Vec::new()
    };

    GameView {
        you: player_id(viewer),
        my_hand,
        opponents,
        battlefield,
        stack,
        graveyards,
        exile,
        phase: phase_of(state.step),
        mana_pool,
        priority_player,
        valid_actions,
        action_deadline: None,
    }
}

/// Resolve an `action_id` a seat returned into the engine [`Action`] to apply, or
/// `None` if that id was not among the actions offered to `seat`.
///
/// This is pure routing, not rules: it never decides legality (the engine already
/// did, in [`issued_actions`]) — it only checks that the id names something that
/// seat was actually offered. Because the engine offers actions to exactly one
/// seat (the priority holder), an id returned by any other seat resolves to `None`
/// and the room rejects it. An unknown or stale id resolves to `None` too.
pub(crate) fn resolve_action(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    action_id: &str,
) -> Option<Action> {
    if state.priority_holder().is_none() || state.priority != seat {
        return None;
    }
    issued_actions(state, db)
        .into_iter()
        .find(|(id, _)| id == action_id)
        .map(|(_, action)| action)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// Two copies of the same printed card in one hand must project to distinct
    /// entity ids and independently routable actions (issue #51). Before
    /// per-instance identity both copies shared `card_5`, so a returned action
    /// resolved against "the first matching copy".
    #[test]
    fn issue_51_duplicate_hand_cards_get_distinct_entities_and_actions() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest_a = state.new_instance(CardId(5));
        let forest_b = state.new_instance(CardId(5));
        state.players[0].hand = vec![forest_a, forest_b];

        let view = personalized_view(&state, &db, PlayerId(0));

        // Each physical copy gets its own hand entity id.
        let hand_ids: Vec<&str> = view.my_hand.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(hand_ids.len(), 2);
        assert_ne!(hand_ids[0], hand_ids[1]);
        assert!(hand_ids.contains(&card_entity_id(forest_a.id).as_str()));
        assert!(hand_ids.contains(&card_entity_id(forest_b.id).as_str()));

        // Two land actions, each carrying its own copy's entity id as subject.
        let land_actions: Vec<&ValidAction> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "play_land")
            .collect();
        assert_eq!(land_actions.len(), 2);
        let subjects: Vec<&str> = land_actions.iter().map(|a| a.subject[0].as_str()).collect();
        assert_ne!(subjects[0], subjects[1]);

        // Each action id routes back to a PlayLand naming the exact instance its
        // subject entity referenced — no ambiguity, no "first matching copy".
        for action in &land_actions {
            let resolved = resolve_action(&state, &db, PlayerId(0), &action.id).unwrap();
            let Action::PlayLand { card } = resolved else {
                panic!("play_land action must resolve to a PlayLand");
            };
            assert_eq!(action.subject[0], card_entity_id(card.id));
        }

        // The two actions route to two different instances between them.
        let routed: Vec<CardInstance> = land_actions
            .iter()
            .map(
                |a| match resolve_action(&state, &db, PlayerId(0), &a.id).unwrap() {
                    Action::PlayLand { card } => card,
                    other => panic!("expected PlayLand, got {other:?}"),
                },
            )
            .collect();
        assert_ne!(routed[0].id, routed[1].id);
        assert!(routed.contains(&forest_a));
        assert!(routed.contains(&forest_b));
    }

    /// The engine RNG seed must never surface in a personalized view: it would
    /// leak future shuffle outcomes. Two states differing only in `rng_seed`
    /// therefore project to byte-identical views for the same seat.
    #[test]
    fn rng_seed_never_appears_in_a_personalized_view() {
        let db = CardDatabase::bundled().unwrap();
        let base = GameState::new_two_player_with_seed(0);
        let reseeded = GameState::new_two_player_with_seed(0xABCD_1234_5678_9ABC);

        for seat in 0..base.players.len() {
            let viewer = PlayerId(seat);
            assert_eq!(
                personalized_view(&base, &db, viewer),
                personalized_view(&reseeded, &db, viewer),
            );
        }
    }
}
