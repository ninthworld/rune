//! Projecting the game log and terminal result into wire entries.

use super::*;

/// Project the bounded engine history into receiver-safe structured protocol events.
/// Every referenced card here was already public at the event boundary; hidden draws
/// are represented only by a count in the engine event and reveal no identity.
///
/// Names come from the identity **recorded in each event**, never re-resolved against
/// the current battlefield, so a snapshot's history stays stable even after a
/// referenced permanent has left play (died, bounced): a combatant or dead creature
/// keeps its name for the life of the window.
pub(crate) fn log_entries(state: &GameState, db: &CardDatabase) -> Vec<GameLogEntry> {
    state
        .log
        .iter()
        .map(|entry| {
            let event = match &entry.event {
                GameEvent::SpellCast { player, card } => GameLogEvent::SpellCast {
                    player: player_id(*player),
                    card: log_card(card.id, card.card, db),
                },
                GameEvent::SpellResolved { player, card } => GameLogEvent::SpellResolved {
                    player: player_id(*player),
                    card: log_card(card.id, card.card, db),
                },
                GameEvent::SpellCountered { player, card } => GameLogEvent::SpellCountered {
                    player: player_id(*player),
                    card: log_card(card.id, card.card, db),
                },
                GameEvent::SpellFizzled { player, card } => GameLogEvent::SpellFizzled {
                    player: player_id(*player),
                    card: log_card(card.id, card.card, db),
                },
                GameEvent::AttackersDeclared { player, attackers } => {
                    GameLogEvent::AttackersDeclared {
                        player: player_id(*player),
                        attackers: attackers.iter().map(|lp| log_permanent(lp, db)).collect(),
                    }
                }
                GameEvent::BlockersDeclared { player, blocks } => GameLogEvent::BlockersDeclared {
                    player: player_id(*player),
                    blocks: blocks
                        .iter()
                        .map(|(blocker, attacker)| LogBlock {
                            blocker: log_permanent(blocker, db),
                            attacker: log_permanent(attacker, db),
                        })
                        .collect(),
                },
                GameEvent::Mulligan { player } => GameLogEvent::Mulligan {
                    player: player_id(*player),
                },
                GameEvent::HandKept { player } => GameLogEvent::HandKept {
                    player: player_id(*player),
                },
                GameEvent::LifeChanged { player, amount } => GameLogEvent::LifeChanged {
                    player: player_id(*player),
                    amount: *amount,
                },
                GameEvent::DamageDealt { target, amount } => GameLogEvent::DamageDealt {
                    target: log_damage_target(target, db),
                    amount: *amount,
                },
                GameEvent::CardsDrawn { player, count } => GameLogEvent::CardsDrawn {
                    player: player_id(*player),
                    count: *count,
                },
                GameEvent::PermanentDied { permanent } => GameLogEvent::PermanentDied {
                    permanent: log_permanent(permanent, db),
                },
                GameEvent::StepChanged {
                    turn,
                    active_player,
                    step,
                } => GameLogEvent::StepChanged {
                    turn: *turn,
                    active_player: player_id(*active_player),
                    phase: phase_of(*step),
                },
                GameEvent::PlayerEliminated { player, reason } => GameLogEvent::PlayerEliminated {
                    player: player_id(*player),
                    reason: game_over_reason(*reason),
                },
                GameEvent::GameOver { result } => GameLogEvent::GameOver {
                    result: result_view(result.clone()),
                },
                // CR 903.9a: the commander's owner returned it from a graveyard or
                // exile to the command zone. The commander card is public (designated
                // openly, moving between public zones), so it is named exactly like the
                // other zone-movement events — the identity recorded in the event, never
                // re-resolved against the current board.
                GameEvent::CommanderReturnedToCommandZone { player, card } => {
                    GameLogEvent::CommanderReturnedToCommandZone {
                        player: player_id(*player),
                        card: log_card(card.id, card.card, db),
                    }
                }
            };
            GameLogEntry {
                sequence: entry.sequence,
                event,
            }
        })
        .collect()
}

fn log_card(instance: CardInstanceId, card: CardId, db: &CardDatabase) -> LogEntity {
    LogEntity {
        id: card_entity_id(instance),
        name: db
            .card(card)
            .map_or_else(|| "Unknown card".into(), |c| c.name.clone()),
    }
}

/// Name a logged permanent from the **card identity recorded in the event**, not the
/// current battlefield — so the entry stays stable once the permanent has left play.
fn log_permanent(logged: &LoggedPermanent, db: &CardDatabase) -> LogEntity {
    LogEntity {
        id: permanent_entity_id(logged.permanent),
        name: db
            .card(logged.card)
            .map_or_else(|| "Unknown permanent".into(), |card| card.name.clone()),
    }
}

fn log_damage_target(target: &DamageTarget, db: &CardDatabase) -> LogDamageTarget {
    match target {
        DamageTarget::Player(player) => LogDamageTarget::Player {
            player: player_id(*player),
        },
        DamageTarget::Permanent(logged) => LogDamageTarget::Permanent {
            permanent: log_permanent(logged, db),
        },
    }
}

/// The wire name for an engine [`LossReason`], as the client expects it in
/// [`GameOverReason`]. Kept exhaustive so a new engine reason forces a matching
/// wire variant here rather than silently going unnamed.
fn game_over_reason(reason: LossReason) -> GameOverReason {
    match reason {
        LossReason::ZeroLife => GameOverReason::LifeZero,
        LossReason::DrewFromEmptyLibrary => GameOverReason::Decked,
        LossReason::Concede => GameOverReason::Concede,
        LossReason::CommanderDamage => GameOverReason::CommanderDamage,
    }
}

/// Project the engine's per-designation commander-damage tally (CR 903.10a, issue
/// #371) onto the wire [`CommanderDamageView`]. **Public information** — the same
/// for every receiver — so both seated and spectator views carry it verbatim. Each
/// commander is named by its owning player's `p{N}` id, the stable designation key.
pub(crate) fn commander_damage_view(state: &GameState) -> Vec<CommanderDamageView> {
    state
        .commander_damage
        .iter()
        .map(|entry| CommanderDamageView {
            commander: player_id(entry.commander),
            damaged: player_id(entry.damaged),
            amount: entry.amount,
        })
        .collect()
}

/// Project each designated commander's tax (CR 903.8, issue #372) onto the wire
/// [`CommanderTaxView`]. **Public information** — the tax is `{2}` per prior cast
/// from the command zone — so both seated and spectator views carry it verbatim.
/// One entry per player that has a commander designation, named by that player's
/// `p{N}` id (one commander per player today).
pub(crate) fn commander_tax_view(state: &GameState) -> Vec<CommanderTaxView> {
    state
        .players
        .iter()
        .enumerate()
        .filter_map(|(seat, player)| {
            player.commander.map(|commander| CommanderTaxView {
                commander: player_id(PlayerId(seat)),
                casts: commander.casts,
                tax: commander.tax_generic(),
            })
        })
        .collect()
}

/// Project the engine's terminal [`GameResult`] onto the wire [`GameResultView`],
/// naming each seat by its `p{N}` id (CR 104.2a). Pure translation, no game logic.
pub(crate) fn result_view(result: GameResult) -> GameResultView {
    GameResultView {
        winner: result.winner.map(player_id),
        losers: result.losers.into_iter().map(player_id).collect(),
        reason: game_over_reason(result.reason),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;

    /// A terminal game (issue #119) projects its result onto the view: the winner,
    /// losers, and reason are named, and `valid_actions` is empty (CR 104.2a).
    #[test]
    fn issue_119_terminal_result_projects_onto_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.players[1].has_lost = true;
        state.players[1].loss_reason = Some(LossReason::Concede);

        let view = personalized_view(&state, &db, PlayerId(0));
        let result = view.result.expect("a terminal state carries a result");
        assert_eq!(result.winner.as_deref(), Some("p0"));
        assert_eq!(result.losers, vec!["p1".to_string()]);
        assert_eq!(result.reason, GameOverReason::Concede);
        assert!(
            view.valid_actions.is_empty(),
            "a terminal state offers no actions (CR 104.2a)"
        );

        // A live game omits the result entirely.
        let live = personalized_view(&GameState::new_two_player(), &db, PlayerId(0));
        assert!(live.result.is_none());
    }

    #[test]
    fn issue_259_a_dead_combatant_keeps_its_name_in_the_projected_history() {
        // Review P2: an attacker/blocker event names its permanents from the identity
        // recorded in the event, not the current battlefield — so the entry stays
        // stable after the creature has died and is no longer on the battlefield. A
        // re-resolving projection would show "Unknown permanent" here.
        use rune_engine::{GameEvent, GameLogEntry, LoggedPermanent};
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar_card = fixture("onakke_ogre");
        let attacker = PermanentId(7);
        // The event records the combatant's identity; the permanent itself is *not* on
        // the battlefield (it has already left play).
        state.log.push(GameLogEntry {
            sequence: 1,
            event: GameEvent::AttackersDeclared {
                player: PlayerId(0),
                attackers: vec![LoggedPermanent {
                    permanent: attacker,
                    card: boar_card,
                }],
            },
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let GameLogEvent::AttackersDeclared { attackers, .. } = &view.log[0].event else {
            panic!("expected an attackers_declared event");
        };
        assert_eq!(attackers.len(), 1);
        assert_eq!(
            attackers[0].id,
            permanent_entity_id(attacker),
            "the id is the (never-reused) permanent handle"
        );
        assert_eq!(
            attackers[0].name,
            db.card(boar_card).unwrap().name,
            "the name comes from the recorded identity, not a battlefield lookup"
        );
    }

    #[test]
    fn issue_259_a_hidden_draw_projects_a_count_with_no_card_identity() {
        // Redaction: a draw is a player + count in the engine event, so the projected
        // event can carry no card identity to leak.
        use rune_engine::{GameEvent, GameLogEntry};
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.log.push(GameLogEntry {
            sequence: 1,
            event: GameEvent::CardsDrawn {
                player: PlayerId(1),
                count: 2,
            },
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        assert!(matches!(
            view.log[0].event,
            GameLogEvent::CardsDrawn { count: 2, .. }
        ));
    }

    #[test]
    fn issue_342_elimination_projects_a_player_eliminated_log_event() {
        // A player leaving a 3-seat game under CR 800.4a projects as a
        // `player_eliminated` log event carrying the seat and the loss reason. The
        // engine records it in the state's log window; the projection maps it 1:1.
        use rune_engine::{GameEvent, GameLogEntry};
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_multiplayer(3);
        state.log.push(GameLogEntry {
            sequence: 1,
            event: GameEvent::PlayerEliminated {
                player: PlayerId(1),
                reason: LossReason::ZeroLife,
            },
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let GameLogEvent::PlayerEliminated { player, reason } = &view.log[0].event else {
            panic!("expected a player_eliminated event");
        };
        assert_eq!(player, &player_id(PlayerId(1)));
        assert_eq!(reason, &GameOverReason::LifeZero);
    }

    #[test]
    fn issue_397_commander_return_projects_a_zone_movement_log_event() {
        // CR 903.9a (issue #397): a commander returned from a graveyard or exile to the
        // command zone projects as a `commander_returned_to_command_zone` wire event,
        // naming the commander card from the identity recorded in the event — the same
        // public treatment other zone-movement events get.
        use rune_engine::{CardInstance, GameEvent, GameLogEntry};
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let commander = state.new_instance(fixture("onakke_ogre"));
        state.log.push(GameLogEntry {
            sequence: 1,
            event: GameEvent::CommanderReturnedToCommandZone {
                player: PlayerId(0),
                card: CardInstance {
                    id: commander.id,
                    card: commander.card,
                },
            },
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let GameLogEvent::CommanderReturnedToCommandZone { player, card } = &view.log[0].event
        else {
            panic!("expected a commander_returned_to_command_zone event");
        };
        assert_eq!(player, &player_id(PlayerId(0)));
        assert_eq!(
            card.name,
            db.card(commander.card).unwrap().name,
            "the commander is named from the recorded identity"
        );

        // The same public event reaches a spectator's log verbatim.
        let spectator = spectator_view(&state, &db);
        assert_eq!(spectator.log, view.log);
    }

    #[test]
    fn issue_371_commander_damage_tally_projects_as_public_information() {
        // CR 903.10a (issue #371): the engine's per-designation commander-damage
        // tally is public, so every seated view and the spectator view carry it
        // verbatim, each commander named by its owning player's `p{N}` id.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_multiplayer(3);
        // Public tally set directly (the engine's incrementing seam is crate-private).
        state.commander_damage.push(rune_engine::CommanderDamage {
            commander: PlayerId(0),
            damaged: PlayerId(1),
            amount: 14,
        });

        let seated = personalized_view(&state, &db, PlayerId(2));
        assert_eq!(seated.commander_damage.len(), 1);
        let entry = &seated.commander_damage[0];
        assert_eq!(entry.commander, player_id(PlayerId(0)));
        assert_eq!(entry.damaged, player_id(PlayerId(1)));
        assert_eq!(entry.amount, 14);

        // A spectator sees the same public tally.
        let spectator = spectator_view(&state, &db);
        assert_eq!(spectator.commander_damage, seated.commander_damage);

        // A game with no commander damage elides the field entirely.
        let empty = personalized_view(&GameState::new_two_player(), &db, PlayerId(0));
        assert!(empty.commander_damage.is_empty());
    }
}
