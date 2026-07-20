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
    abilities_of, attacker_candidates, attackers_needing_damage_order, attacking_defender_of,
    blocker_candidates_for, bottom_requirement, characteristics, declared_attackers,
    defender_candidates, defending_player, is_mana_ability, pending_blocker_declarer,
    scripted_rules_text, target_requirements, valid_actions, Action, Attack, Block, CardData,
    CardDatabase, CardId, CardInstance, CardInstanceId, CounterKind, DamageOrder, DamageTarget,
    GameEvent, GameResult, GameState, Keyword, LoggedPermanent, LossReason, PermanentId, Player,
    PlayerId, StackId, StackObject, StackObjectKind, Step, Target, TargetSpec,
};

use crate::rules_text::{ability_text, effects_description, rules_text};
use rune_protocol::{
    CardView, ChooseAction, CommanderDamage as CommanderDamageView,
    CommanderTax as CommanderTaxView, Counter, GameLogEntry, GameLogEvent, GameOverReason,
    GameResult as GameResultView, GameView, LogBlock, LogDamageTarget, LogEntity, OpponentView,
    Permanent as PermanentView, Phase, Prompt, PromptOption, SelfView, SpectatorView, StackItem,
    TargetChoice, TargetRequirement, ValidAction, ZonePile,
};

mod actions;
mod binding;
mod cards;
mod ids;
mod log;
mod prompt;
mod requirements;
#[cfg(test)]
mod test_support;

pub(crate) use actions::*;
pub(crate) use binding::*;
pub(crate) use cards::*;
pub(crate) use ids::*;
pub(crate) use log::*;
pub(crate) use prompt::*;
pub(crate) use requirements::*;

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
            // Eliminated state (CR 800.4a, issue #342/#345): an opponent who left the
            // game. Additive — false (and omitted) in a two-player game.
            eliminated: player.has_lost,
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
            // Current (computed) characteristics, so counters/pumps/Auras show on
            // the host's P/T (CR 613.7c), not the printed values.
            card: permanent_card_view(state, perm, db),
            tapped: perm.tapped,
            // Combat declaration state (CR 508/509): whether this permanent is
            // attacking, whom it attacks (issue #341/#345), and which attacker it is
            // blocking (as an entity id). `attacking` stays the boolean fact for
            // back-compat; `attacking_player` names the defending player so a client
            // can render split attacks — omitted when not attacking.
            attacking: perm.attacking.is_some(),
            attacking_player: perm.attacking.map(player_id),
            blocking: perm.blocking.map(permanent_entity_id),
            // Marked combat damage (CR 120.3 / 510), for lethal-damage display.
            damage: perm.damage,
            // Aura attachment (CR 303.4): the host this permanent is attached to,
            // projected from the engine's `PermanentId` to its view entity id
            // exactly as `blocking` above. `None` for an unattached permanent.
            attached_to: perm.attached_to.map(permanent_entity_id),
            counters: permanent_counters(perm),
        })
        .collect();

    let stack = state
        .stack
        .iter()
        .map(|o| stack_item(state, o, db))
        .collect();
    let graveyards = zone_piles(state, |p| &p.graveyard, db);
    let exile = zone_piles(state, |p| &p.exile, db);
    // The command zone (CR 903.6, issue #372): the same public pile treatment as
    // graveyards/exile, over each player's command pile.
    let command = zone_piles(state, |p| &p.command, db);

    let mana_pool = state
        .players
        .get(viewer.0)
        .map(|player| player.mana_pool.pips())
        .unwrap_or_default();

    // The receiver's own public stats — the same life and library-size numbers every
    // opponent already sees about this player (CR 104.3a / public information), which
    // the view previously carried for opponents but not for the receiver themself.
    let me = state
        .players
        .get(viewer.0)
        .map(|player| SelfView {
            life: player.life,
            library_size: count(player.library.len()),
        })
        .unwrap_or_default();

    let holds_priority = state.priority_holder().is_some();
    let priority_player = holds_priority.then(|| player_id(state.priority));

    let valid_actions = if holds_priority && state.priority == viewer {
        projected_actions(state, db)
            .into_iter()
            .map(|projected| projected.view)
            .collect()
    } else {
        Vec::new()
    };

    GameView {
        you: player_id(viewer),
        my_hand,
        me,
        opponents,
        battlefield,
        stack,
        graveyards,
        exile,
        command,
        phase: phase_of(state.step),
        // Turn structure (issue #267): the engine owns turn counting and whose turn
        // it is; the view carries them so the client's phase/turn ribbon renders
        // without counting anything itself.
        turn: state.turn,
        active_player: player_id(state.active_player),
        // Explicit seat order (issue #345): every seat's id in order, so a
        // multiplayer client can place opponents in a stable arrangement rather than
        // relying on the projection's incidental ordering.
        seat_order: (0..state.players.len())
            .map(|seat| player_id(PlayerId(seat)))
            .collect(),
        mana_pool,
        priority_player,
        valid_actions,
        action_deadline: None,
        // The terminal result once the game is over (CR 104.2a); `None` — and so
        // omitted from the wire — while the game is live.
        result: state.result().map(result_view),
        log: log_entries(state, db),
        // Priority-stop preferences and the auto-pass indicator are room/session
        // state, not engine state; the room fills them in after projection (issue
        // #264), exactly as it does the player names. Defaults here (no stops, not
        // auto-passed) keep this pure shim automation-agnostic and elide from the wire.
        stops: Vec::new(),
        auto_passed: false,
        // Rejected-action feedback is likewise a room concern, not engine state (issue
        // #265): only the room knows an action was rejected, and it flags the one
        // re-sent view answering that rejection. Not-rejected here by default, so it
        // elides from the wire on every normal projection.
        action_rejected: false,
        // Player display names are a lobby/session concern, not engine state; the room
        // fills this in after projection (issue #294). Empty here so this pure shim
        // stays name-agnostic and the field elides from the wire by default.
        player_names: std::collections::BTreeMap::new(),
        // Commander combat-damage tally (CR 903.10a, issue #371): public information,
        // projected verbatim from the engine's per-designation totals.
        commander_damage: commander_damage_view(state),
        // Commander tax (CR 903.8, issue #372): public information — {2} per prior
        // cast from the command zone — projected from each designation's cast count.
        commander_tax: commander_tax_view(state),
    }
}

/// Project the game state onto a **spectator** view (ADR 0022, issue #351): the
/// public intersection only, for a non-seated observer. Unlike [`personalized_view`]
/// there is **no viewer** — nothing indexes a seat's hand, mana pool, or actions, so
/// the projection *cannot* reach any hidden information. Redaction is structural: the
/// [`SpectatorView`] type simply has no field that could hold a hand, a library's
/// contents, or a decision surface, so the worst a projection bug could do is *omit* a
/// public fact, never *leak* a private one.
///
/// Every seat is projected as the same public [`OpponentView`] shape seated views use
/// for a non-receiver seat (life, hand/library/graveyard *sizes*, eliminated flag);
/// the battlefield, stack, public zone piles, phase, turn, active/priority player, seat
/// order, terminal result, and public log are the same public projections
/// [`personalized_view`] shares. Player display names, like there, are filled by the
/// room after projection (they are a lobby/session concern, not engine state).
pub(crate) fn spectator_view(state: &GameState, db: &CardDatabase) -> SpectatorView {
    // Every seat as a public OpponentView — there is no privileged "self".
    let players = state
        .players
        .iter()
        .enumerate()
        .map(|(seat, player)| OpponentView {
            player_id: player_id(PlayerId(seat)),
            hand_size: count(player.hand.len()),
            life: player.life,
            library_size: count(player.library.len()),
            graveyard_size: count(player.graveyard.len()),
            statuses: Vec::new(),
            eliminated: player.has_lost,
        })
        .collect();

    let battlefield = state
        .battlefield
        .iter()
        .map(|perm| PermanentView {
            id: permanent_entity_id(perm.id),
            controller: player_id(perm.controller),
            owner: player_id(perm.controller),
            card: permanent_card_view(state, perm, db),
            tapped: perm.tapped,
            attacking: perm.attacking.is_some(),
            attacking_player: perm.attacking.map(player_id),
            blocking: perm.blocking.map(permanent_entity_id),
            damage: perm.damage,
            attached_to: perm.attached_to.map(permanent_entity_id),
            counters: permanent_counters(perm),
        })
        .collect();

    let stack = state
        .stack
        .iter()
        .map(|o| stack_item(state, o, db))
        .collect();
    let holds_priority = state.priority_holder().is_some();

    SpectatorView {
        players,
        battlefield,
        stack,
        graveyards: zone_piles(state, |p| &p.graveyard, db),
        exile: zone_piles(state, |p| &p.exile, db),
        command: zone_piles(state, |p| &p.command, db),
        phase: phase_of(state.step),
        turn: state.turn,
        active_player: player_id(state.active_player),
        seat_order: (0..state.players.len())
            .map(|seat| player_id(PlayerId(seat)))
            .collect(),
        // Whose turn it is to act — public, decision-free. A spectator sees *that* a
        // seat holds priority, never the actions offered to it.
        priority_player: holds_priority.then(|| player_id(state.priority)),
        result: state.result().map(result_view),
        log: log_entries(state, db),
        // Names are a lobby/session concern; the room fills them after projection.
        player_names: std::collections::BTreeMap::new(),
        // Commander combat-damage tally (CR 903.10a, issue #371): public information a
        // spectator sees exactly as seated players do.
        commander_damage: commander_damage_view(state),
        // Commander tax (CR 903.8, issue #372): public information a spectator sees
        // exactly as seated players do.
        commander_tax: commander_tax_view(state),
    }
}

/// Resolve a returned [`ChooseAction`] into the engine [`Action`] to apply, or
/// `None` if the answer does not name — and correctly bind to — an action this
/// `seat` was actually offered.
///
/// This is pure routing, not rules: the engine already decided legality (in
/// [`projected_actions`]) and re-checks it in [`apply_action`](rune_engine::apply_action);
/// this only checks the answer against what was offered and threads the chosen
/// selection onto the concrete engine action. Because the engine offers actions to
/// exactly one seat (the priority holder), an answer from any other seat resolves to
/// `None`. Resolution rejects, rather than applies, when:
///
/// - the seat does not hold priority, or the id names no offered action;
/// - the returned [`token`](ChooseAction::token) is present but does not match the
///   token the server currently issues for that id — a stale/redirected id whose
///   action content has changed hashes to a different token, so it can never rebind
///   to a *different* action (ADR 0009 §Protocol, content binding);
/// - the token is absent (`""`) yet the offered action carries `requirements` and so
///   *requires* binding — a bound action must be answered with its token, never on
///   the legacy positional path;
/// - the returned selection does not map onto the offered action's requirement slots
///   from their current legal candidate sets (see the per-kind `bind_*` helpers).
///
/// The mulligan bottoming and ability-target slots are mandatory (each must be
/// filled from its candidates), while the combat declarations are optional
/// multi-selects — an empty selection legally declares no attackers/blockers. An
/// empty token is still accepted for a plain, requirement-less action (a
/// requirement-less combat declaration included), so the token-less positional path
/// keeps working for sequential actions (ADR 0009 §Context).
pub(crate) fn resolve_action(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    choice: &ChooseAction,
) -> Option<Action> {
    if state.priority_holder().is_none() || state.priority != seat {
        return None;
    }

    // Regenerate the offered wire actions from current state, so the token,
    // requirement candidates, and prompt content are all recomputed (stateless
    // routing), then find the one this answer names.
    let Projected {
        view: offered,
        bind,
    } = projected_actions(state, db)
        .into_iter()
        .find(|projected| projected.view.id == choice.action_id)?;

    // Content binding: verify the token (or, for a token-less answer, permit only a
    // plain action on the legacy positional path — one carrying neither requirements
    // nor prompts to bind).
    if choice.token.is_empty() {
        if !offered.requirements.is_empty() || !offered.prompts.is_empty() {
            return None;
        }
    } else if choice.token != offered.token {
        return None;
    }

    match bind {
        // The collapsed richer-prompt actions (issue #156) map their option /
        // select-from-zone answers back onto the concrete engine action.
        Bind::MulliganDecision => bind_mulligan_decision(state, &offered, &choice.targets),
        Bind::DiscardFromHand => bind_discard(state, &offered, &choice.targets),
        // A 1:1 engine-action projection. The combat declarations are optional
        // multi-selects (empty is legal), so they bind directly against their fresh
        // candidate sets; the ability targets are mandatory slots, gated by
        // [`targets_fill_requirements`] first.
        Bind::Standard(action) => match &action {
            Action::DeclareAttackers { .. } => {
                bind_attackers(state, db, &offered.requirements, &choice.targets)
            }
            Action::DeclareBlockers { .. } => bind_blockers(state, db, &choice.targets),
            Action::OrderCombatDamage { .. } => bind_order_combat_damage(state, &choice.targets),
            Action::ActivateAbility { .. } | Action::CastSpell { .. } => {
                if !targets_fill_requirements(&choice.targets, &offered.requirements) {
                    return None;
                }
                bind_ability_targets(state, db, &action, &choice.targets)
            }
            _ => {
                if !targets_fill_requirements(&choice.targets, &offered.requirements) {
                    return None;
                }
                Some(action)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;
    use crate::view::test_support::put_permanent;

    #[test]
    fn issue_255_own_life_and_library_project_onto_the_view() {
        // The receiver sees their own public stats — the gap that let a player read
        // every opponent's life but not their own.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.players[0].life = 17;
        state.players[1].life = 12;
        state.players[0].library = (0..30)
            .map(|_| state.new_instance(fixture("forest")))
            .collect();

        let view = personalized_view(&state, &db, PlayerId(0));
        // Own stats present and correct...
        assert_eq!(view.me.life, 17);
        assert_eq!(view.me.library_size, 30);
        // ...while the opponent's public life is still projected, and distinct.
        assert_eq!(view.opponents.len(), 1);
        assert_eq!(view.opponents[0].life, 12);

        // Seat 1's own view shows *their* life, never seat 0's — the projection is
        // per-viewer, so `me` is always the receiver.
        let other = personalized_view(&state, &db, PlayerId(1));
        assert_eq!(other.me.life, 12);
    }

    #[test]
    fn issue_267_turn_number_and_active_player_project_onto_the_view() {
        // The phase/turn ribbon reads the turn number and whose turn it is straight
        // from the view; the engine owns both, so the projection just carries them.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.turn = 4;
        state.active_player = PlayerId(1);

        // Every viewer sees the same public turn structure (it is not redacted).
        for viewer in [PlayerId(0), PlayerId(1)] {
            let view = personalized_view(&state, &db, viewer);
            assert_eq!(view.turn, 4);
            assert_eq!(view.active_player, "p1");
        }
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

    #[test]
    fn issue_345_view_carries_attack_targets_eliminated_flags_and_seat_order() {
        // A 3-seat mid-combat state projects each attacker's defending player, each
        // opponent's eliminated flag, and the table's explicit seat order.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_multiplayer(3);
        state.turn = 2;
        state.step = Step::DeclareBlockers;
        state.attackers_declared = true;
        // Seat 0 attacks seat 2. Seat 1 has been eliminated.
        let attacker = put_permanent(
            &mut state,
            fixture("walking_corpse"),
            PlayerId(0),
            true,
            false,
        );
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == attacker)
            .unwrap()
            .attacking = Some(PlayerId(2));
        state.players[1].has_lost = true;

        let view = personalized_view(&state, &db, PlayerId(0));

        // Seat order lists every seat in order, including the eliminated one.
        assert_eq!(
            view.seat_order,
            vec![
                player_id(PlayerId(0)),
                player_id(PlayerId(1)),
                player_id(PlayerId(2))
            ]
        );
        // The attacker names whom it attacks (seat 2).
        let atk = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(attacker))
            .unwrap();
        assert!(atk.attacking);
        assert_eq!(atk.attacking_player, Some(player_id(PlayerId(2))));
        // The eliminated opponent is flagged; the live one is not.
        let opp1 = view
            .opponents
            .iter()
            .find(|o| o.player_id == player_id(PlayerId(1)))
            .unwrap();
        let opp2 = view
            .opponents
            .iter()
            .find(|o| o.player_id == player_id(PlayerId(2)))
            .unwrap();
        assert!(opp1.eliminated, "seat 1 left the game");
        assert!(!opp2.eliminated, "seat 2 is still in");
    }

    #[test]
    fn issue_351_spectator_view_is_the_public_intersection_with_no_hidden_fields() {
        // A 3-seat game with cards in every hand, a battlefield permanent, and one
        // eliminated seat. The spectator projection must show every seat as public
        // counts, expose no hand contents or decision surface, and — structurally —
        // carry no receiver fields at all (ADR 0022, issue #351).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_multiplayer(3);
        state.step = Step::PrecombatMain;
        state.priority = PlayerId(0);
        // Distinctive hands per seat, so a leak of any seat's hand would be visible.
        let hand0 = vec![
            state.new_instance(fixture("forest")),
            state.new_instance(fixture("onakke_ogre")),
        ];
        let hand1 = vec![state.new_instance(fixture("snapping_drake"))];
        state.players[0].hand = hand0.clone();
        state.players[1].hand = hand1.clone();
        state.players[2].has_lost = true;
        let perm = put_permanent(
            &mut state,
            fixture("walking_corpse"),
            PlayerId(0),
            false,
            false,
        );

        let view = spectator_view(&state, &db);

        // Every seat appears as a public OpponentView — there is no privileged self.
        assert_eq!(view.players.len(), 3);
        let seat = |id: &str| view.players.iter().find(|p| p.player_id == id).unwrap();
        assert_eq!(seat("p0").hand_size, 2, "hand SIZE is public…");
        assert_eq!(seat("p1").hand_size, 1);
        assert!(seat("p2").eliminated, "the eliminated seat is flagged");
        // The public counts equal what any seated player already sees about a seat
        // (the intersection of all seated players' public information).
        let seated = personalized_view(&state, &db, PlayerId(1));
        let opp0 = seated
            .opponents
            .iter()
            .find(|o| o.player_id == "p0")
            .unwrap();
        assert_eq!(seat("p0").hand_size, opp0.hand_size);
        assert_eq!(seat("p0").library_size, opp0.library_size);
        // The public battlefield permanent is present (public board is shared).
        assert!(view
            .battlefield
            .iter()
            .any(|p| p.id == permanent_entity_id(perm)));

        // …but no hand CONTENTS leak: no seat's hand card entity id appears anywhere.
        let json = serde_json::to_value(&view).unwrap();
        let text = json.to_string();
        for inst in hand0.iter().chain(hand1.iter()) {
            assert!(
                !text.contains(&card_entity_id(inst.id)),
                "a hidden hand card id leaked to a spectator"
            );
        }
        // Structural redaction: the receiver/decision fields do not exist on the type.
        for hidden in [
            "you",
            "me",
            "my_hand",
            "mana_pool",
            "valid_actions",
            "action_deadline",
            "stops",
        ] {
            assert!(
                json.get(hidden).is_none(),
                "a spectator view must never carry `{hidden}`"
            );
        }
    }
}
