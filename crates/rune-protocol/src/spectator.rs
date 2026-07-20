//! The redacted [`SpectatorView`] a non-seated observer receives (ADR 0022).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    CommanderDamage, CommanderTax, GameLogEntry, GameResult, OpponentView, Permanent, Phase,
    PlayerId, StackItem, ZonePile,
};

/// The state a **spectator** connection receives (ADR 0022, issue #351): a
/// non-seated observer watching a live game with all hidden information redacted
/// **by construction**. It shares [`GameView`]'s public component types verbatim —
/// [`OpponentView`], [`Permanent`], [`StackItem`], [`ZonePile`], [`GameLogEntry`],
/// [`Phase`], [`PlayerId`], [`GameResult`] — but carries **no receiver fields**:
/// there is no `you`, `me`, `my_hand`, `mana_pool`, `valid_actions`, `action_deadline`,
/// or per-seat prompt, because those fields simply do not exist on the type. A
/// projection therefore *cannot* leak a hand, a library's contents, or a decision
/// surface to a spectator — the worst case is a missing public fact, never a leaked
/// private one (ADR 0022 §Consequences).
///
/// Every seat appears as the public [`OpponentView`] shape (life, hand *size*, library
/// *size*, graveyard *size*, public statuses, and the eliminated flag); there is no
/// privileged "self". A spectator reconstructs the whole public board from a single
/// `SpectatorView` with no history (the complete-view principle), so it may join
/// mid-game.
///
/// The client distinguishes this from a seated [`GameView`] structurally: a
/// `SpectatorView` carries no `you` field, whereas a `GameView` always serializes one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectatorView {
    /// Every player at the table as the public [`OpponentView`] shape — no seat is
    /// "self". In seat order (see [`Self::seat_order`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<OpponentView>,
    /// All permanents in play (the same public projection seated views share).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battlefield: Vec<Permanent>,
    /// The stack, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackItem>,
    /// Each player's public graveyard pile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyards: Vec<ZonePile>,
    /// Each player's public exile zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<ZonePile>,
    /// Each player's public **command zone** (CR 903.6, issue #372) — the same
    /// public pile seated views carry (see [`GameView::command`]). Omitted in a
    /// non-commander game or while every commander is elsewhere.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<ZonePile>,
    /// The current turn step.
    pub phase: Phase,
    /// The current turn number (1-based).
    #[serde(default)]
    pub turn: u32,
    /// The player whose turn it is (the active player).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_player: PlayerId,
    /// The table's seat order: every player's id in seat order, including eliminated
    /// players — the same public promise seated views carry (issue #345).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_order: Vec<PlayerId>,
    /// Which player currently holds priority, if any. Public, decision-free
    /// information — a spectator sees *whose* turn it is to act but never the actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    /// The terminal outcome once the game is over; omitted while it is live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
    /// The bounded, sequence-numbered window of **public** game history (ADR 0021's
    /// per-viewer redaction gives a spectator the public log for free).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<GameLogEntry>,
    /// Public display names, keyed by [`PlayerId`] (issue #294) — the same public map
    /// seated views carry, so a spectator labels every player without a lobby round-trip.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub player_names: BTreeMap<PlayerId, String>,
    /// Cumulative commander combat damage per `(commander, damaged)` pair (CR
    /// 903.10a, issue #371) — the same **public** tally seated views carry (see
    /// [`GameView::commander_damage`]). Omitted (defaults to empty) in a
    /// non-commander game.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_damage: Vec<CommanderDamage>,
    /// The **commander tax** owed on each designated commander (CR 903.8, issue
    /// #372) — the same **public** projection seated views carry (see
    /// [`GameView::commander_tax`]). Omitted (defaults to empty) in a non-commander
    /// game.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_tax: Vec<CommanderTax>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use std::collections::BTreeMap;

    use crate::*;

    #[test]
    fn issue_351_spectator_view_round_trips_and_has_no_receiver_fields() {
        // A populated, live spectator view over a three-player game with one seat
        // eliminated. Every seat is an OpponentView (public counts only).
        let view = SpectatorView {
            players: vec![
                OpponentView {
                    player_id: "p0".into(),
                    hand_size: 4,
                    life: 18,
                    library_size: 33,
                    graveyard_size: 2,
                    statuses: vec![],
                    eliminated: false,
                },
                OpponentView {
                    player_id: "p1".into(),
                    hand_size: 0,
                    life: 0,
                    library_size: 0,
                    graveyard_size: 7,
                    statuses: vec![],
                    eliminated: true,
                },
                OpponentView {
                    player_id: "p2".into(),
                    hand_size: 6,
                    life: 20,
                    library_size: 34,
                    graveyard_size: 1,
                    statuses: vec![],
                    eliminated: false,
                },
            ],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::PrecombatMain,
            turn: 9,
            active_player: "p0".into(),
            seat_order: vec!["p0".into(), "p1".into(), "p2".into()],
            priority_player: Some("p0".into()),
            result: None,
            log: vec![],
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
        };
        let json = serde_json::to_value(&view).unwrap();
        // Redaction is structural: the type has no receiver/decision fields at all.
        for hidden in [
            "you",
            "me",
            "my_hand",
            "mana_pool",
            "valid_actions",
            "action_deadline",
            "stops",
            "auto_passed",
            "action_rejected",
        ] {
            assert!(
                json.get(hidden).is_none(),
                "a spectator view must never carry `{hidden}`"
            );
        }
        // Every seat appears as a public OpponentView; the eliminated seat is flagged.
        assert_eq!(json["players"].as_array().unwrap().len(), 3);
        assert_eq!(json["players"][1]["eliminated"], true);
        let back: SpectatorView =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(back, view);
    }
}
