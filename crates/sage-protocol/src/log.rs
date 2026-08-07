//! Structured, receiver-safe game-log events (docs/protocol.md).

use serde::{Deserialize, Serialize};

use crate::{EntityId, GameOverReason, GameResult, Phase, PlayerId};

/// One structured, receiver-safe entry in the authoritative recent game history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLogEntry {
    /// Monotonically increasing sequence number. A bounded window may start after
    /// sequence one; clients must render the entries it carries without filling gaps.
    pub sequence: u64,
    /// The event to render as local prose.
    pub event: GameLogEvent,
}

/// A structured game-log event. Entity ids are opaque references for presentation
/// only; a client may highlight one but never infer legality from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameLogEvent {
    /// A player cast a publicly identified spell.
    SpellCast {
        /// The caster.
        player: PlayerId,
        /// The cast card.
        card: LogEntity,
    },
    /// A spell finished resolving (it was neither countered nor fizzled).
    SpellResolved {
        /// The spell's controller.
        player: PlayerId,
        /// The card that resolved.
        card: LogEntity,
    },
    /// A spell was countered and put into its owner's graveyard.
    SpellCountered {
        /// The countered spell's controller.
        player: PlayerId,
        /// The card that was countered.
        card: LogEntity,
    },
    /// A spell left the stack without resolving because all of its targets became
    /// illegal (a "fizzle").
    SpellFizzled {
        /// The fizzled spell's controller.
        player: PlayerId,
        /// The card that fizzled.
        card: LogEntity,
    },
    /// A player declared attackers (possibly none).
    AttackersDeclared {
        /// The attacking player.
        player: PlayerId,
        /// The declared attackers.
        attackers: Vec<LogEntity>,
    },
    /// A player declared blocker-to-attacker assignments.
    BlockersDeclared {
        /// The defending player.
        player: PlayerId,
        /// The assignments they declared.
        blocks: Vec<LogBlock>,
    },
    /// A player took a London mulligan.
    Mulligan {
        /// The player taking the mulligan.
        player: PlayerId,
    },
    /// A player kept their opening hand, ending their mulligan decisions.
    HandKept {
        /// The player who kept.
        player: PlayerId,
    },
    /// A player's life total changed by this signed amount from a non-damage source
    /// (life gain, or life paid/lost). Damage is reported as [`Self::DamageDealt`].
    LifeChanged {
        /// The affected player.
        player: PlayerId,
        /// Signed life-total delta.
        amount: i32,
    },
    /// A source dealt damage to a player or permanent (including nonlethal damage).
    DamageDealt {
        /// What the damage was dealt to.
        target: LogDamageTarget,
        /// How much damage.
        amount: u32,
    },
    /// A player drew cards. Card identities are intentionally absent.
    CardsDrawn {
        /// The player who drew.
        player: PlayerId,
        /// Number of cards drawn.
        count: u32,
    },
    /// A player milled cards — put them from the top of their library into their
    /// graveyard (CR 701.13). Not a draw: it never causes a decking loss, and the
    /// count is what actually moved.
    CardsMilled {
        /// The player who milled.
        player: PlayerId,
        /// Number of cards put into the graveyard.
        count: u32,
    },
    /// Cards were exiled from a library, face up (CR 701.16a) — the digging of a card
    /// that looks for something in a library and exiles what it passes. Card identities
    /// are absent for the same reason a mill's are: the cards are visible in the exile
    /// pile on their own, so the count is the fact the log adds.
    CardsExiled {
        /// The player whose library they came from.
        player: PlayerId,
        /// Number of cards exiled.
        count: u32,
    },
    /// A player discarded cards from their hand (CR 701.8). Card identities are
    /// intentionally absent — a hand is hidden, and the cards show up on their own in
    /// the public graveyard. The count is what actually moved.
    CardsDiscarded {
        /// The player who discarded.
        player: PlayerId,
        /// Number of cards put into the graveyard.
        count: u32,
    },
    /// A player searched their library and shuffled it (CR 701.19). That the search
    /// happened is public; what they looked at and what they found are not, and are
    /// deliberately absent.
    LibrarySearched {
        /// The player who searched.
        player: PlayerId,
    },
    /// A player took an optional effect they were offered ("you may …"), paying its
    /// cost if it had one. What was offered is not carried — the offering ability's
    /// text is public and already says so.
    OptionalApplied {
        /// The player who accepted.
        player: PlayerId,
    },
    /// A player declined an optional effect, or was never asked because its cost was
    /// beyond anything they could pay. The two are one public fact — the effect was
    /// offered and did not happen — and telling them apart would report on a hidden
    /// mana pool.
    OptionalDeclined {
        /// The player who declined.
        player: PlayerId,
    },
    /// A creature died; it may no longer be present on the battlefield.
    PermanentDied {
        /// The permanent that died.
        permanent: LogEntity,
    },
    /// The game reached this turn/step.
    StepChanged {
        /// New turn number.
        turn: u32,
        /// Player taking that turn.
        active_player: PlayerId,
        /// Entered phase.
        phase: Phase,
    },
    /// A player left the game under CR 800.4a — they lost while two or more players
    /// remained, so play continues without them and their objects are removed. This
    /// is the mid-game "leaves the game" event, distinct from [`Self::GameOver`],
    /// which fires only once one player is left. A two-player loss produces
    /// `GameOver`, never this.
    PlayerEliminated {
        /// The player who left the game.
        player: PlayerId,
        /// Why they lost (CR 104.3 / 704.5).
        reason: GameOverReason,
    },
    /// A commander was returned from a graveyard or exile to its owner's command
    /// zone at that owner's choice (CR 903.9a). The card is publicly identified — a
    /// commander is designated openly and moves between public zones — so its name
    /// is carried like any other zone-movement event.
    CommanderReturnedToCommandZone {
        /// The commander's owner, who made the choice.
        player: PlayerId,
        /// The commander card that moved to the command zone.
        card: LogEntity,
    },
    /// The game ended with this already-decided result.
    GameOver {
        /// The terminal result.
        result: GameResult,
    },
    /// A player **undid** the last accepted transition and the table was returned to
    /// the state before it (issue #648). The one entry here that reports something a
    /// player did to the game rather than something the game did.
    ///
    /// It is the record of the rollback, and it is deliberately the *only* one: the
    /// entries the undone transition wrote went back with the state that held them, so
    /// the window a client renders after an undo is the window as it stood at the
    /// restored checkpoint, plus this. Nothing says what was taken back, because the
    /// state that would have described it no longer exists — what a player needs to
    /// know is that the board they are looking at is an earlier one and who asked for
    /// it.
    Undone {
        /// The player who requested the rollback.
        player: PlayerId,
    },
}

/// A clickable named entity reference in a game log event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntity {
    /// Opaque object or player id.
    pub id: EntityId,
    /// Server-supplied display name; clients do not look it up from hidden state.
    pub name: String,
}

/// What a [`GameLogEvent::DamageDealt`] was dealt to: a player or a permanent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogDamageTarget {
    /// Damage dealt to a player.
    Player {
        /// The player who took the damage.
        player: PlayerId,
    },
    /// Damage marked on a permanent.
    Permanent {
        /// The permanent the damage was dealt to.
        permanent: LogEntity,
    },
}

/// One blocker assignment in a declaration event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogBlock {
    /// The declared blocker.
    pub blocker: LogEntity,
    /// The attacker it blocks.
    pub attacker: LogEntity,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn game_log_events_tag_their_type_and_round_trip() {
        // The new #259 vocabulary is a contract: each event serializes under its
        // snake_case `type`, and `damage_dealt` nests a `kind`-tagged target.
        let resolved = GameLogEvent::SpellResolved {
            player: "p0".into(),
            card: LogEntity {
                id: "card_3".into(),
                name: "Quickfire Bolt".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(&resolved).unwrap(),
            serde_json::json!({
                "type": "spell_resolved",
                "player": "p0",
                "card": { "id": "card_3", "name": "Quickfire Bolt" },
            })
        );

        let damage = GameLogEvent::DamageDealt {
            target: LogDamageTarget::Permanent {
                permanent: LogEntity {
                    id: "perm_7".into(),
                    name: "Thornback Boar".into(),
                },
            },
            amount: 3,
        };
        let json = serde_json::to_value(&damage).unwrap();
        assert_eq!(json["type"], "damage_dealt");
        assert_eq!(json["amount"], 3);
        assert_eq!(json["target"]["kind"], "permanent");
        assert_eq!(json["target"]["permanent"]["name"], "Thornback Boar");

        // Every new variant survives a JSON round trip.
        for event in [
            resolved,
            damage,
            GameLogEvent::SpellCountered {
                player: "p1".into(),
                card: LogEntity {
                    id: "card_9".into(),
                    name: "Runic Negation".into(),
                },
            },
            GameLogEvent::SpellFizzled {
                player: "p0".into(),
                card: LogEntity {
                    id: "card_3".into(),
                    name: "Quickfire Bolt".into(),
                },
            },
            GameLogEvent::HandKept {
                player: "p0".into(),
            },
            GameLogEvent::DamageDealt {
                target: LogDamageTarget::Player {
                    player: "p1".into(),
                },
                amount: 2,
            },
            GameLogEvent::PlayerEliminated {
                player: "p2".into(),
                reason: GameOverReason::LifeZero,
            },
            GameLogEvent::CommanderReturnedToCommandZone {
                player: "p0".into(),
                card: LogEntity {
                    id: "card_5".into(),
                    name: "Lathliss, Dragon Queen".into(),
                },
            },
        ] {
            let text = serde_json::to_string(&event).unwrap();
            let back: GameLogEvent = serde_json::from_str(&text).unwrap();
            assert_eq!(event, back);
        }
    }

    #[test]
    fn issue_397_commander_returned_event_tags_its_type_and_names_the_card() {
        // The CR 903.9a return log event (issue #397) serializes under its snake_case
        // `type` and names the moved commander like any other zone-movement event.
        let event = GameLogEvent::CommanderReturnedToCommandZone {
            player: "p1".into(),
            card: LogEntity {
                id: "card_2".into(),
                name: "Lathliss, Dragon Queen".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "type": "commander_returned_to_command_zone",
                "player": "p1",
                "card": { "id": "card_2", "name": "Lathliss, Dragon Queen" },
            })
        );
    }

    #[test]
    fn issue_648_undone_event_names_who_rolled_the_table_back() {
        // The log is where a rollback is recorded and the only place it is named, so
        // the entry says the one thing nobody could work out from the restored board:
        // which player asked for it.
        let event = GameLogEvent::Undone {
            player: "p1".into(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "undone", "player": "p1" })
        );
        let back: GameLogEvent =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn issue_342_player_eliminated_event_tags_its_type_and_reason() {
        // The elimination log event (issue #342) serializes under its snake_case
        // `type` and carries the same GameOverReason enum `game_over` uses.
        let event = GameLogEvent::PlayerEliminated {
            player: "p1".into(),
            reason: GameOverReason::Concede,
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "type": "player_eliminated",
                "player": "p1",
                "reason": "concede",
            })
        );
    }
}
