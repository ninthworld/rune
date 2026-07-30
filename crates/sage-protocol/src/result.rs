//! Game-end outcome and commander-tally types (CR 104 / CR 903).

use serde::{Deserialize, Serialize};

use crate::PlayerId;

/// Why a game ended, as carried in [`GameResult::reason`]. A closed, snake_case
/// enum mirroring the engine's losing conditions (CR 104.3 / CR 704.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameOverReason {
    /// A player was reduced to 0 or less life (CR 704.5a).
    LifeZero,
    /// A player attempted to draw from an empty library (CR 704.5c).
    Decked,
    /// A player conceded (CR 104.3a).
    Concede,
    /// A player was dealt 21 or more combat damage over the game by a single
    /// commander (CR 903.10a).
    CommanderDamage,
}

/// One player's cumulative **combat** damage from one commander this game
/// (CR 903.10a), as carried in [`GameView::commander_damage`]. **Public
/// information** — every player and spectator may see it — projected from the
/// engine's per-designation tally.
///
/// The shape is minimal: per damaged player, per commander. A commander is named
/// by its owning player's id ([`Self::commander`]), since one player designates at
/// most one commander today; this is the stable key the engine's tally uses, so it
/// survives the commander's zone changes. A client renders 21 as the lethal
/// threshold ([`GameOverReason::CommanderDamage`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderDamage {
    /// The commander that dealt the damage, named by its owning player's id — the
    /// designation key (one commander per player today).
    pub commander: PlayerId,
    /// The player that has taken the damage.
    pub damaged: PlayerId,
    /// Cumulative combat damage this commander has dealt this player this game.
    pub amount: u32,
}

/// The **commander tax** currently owed on one player's commander (CR 903.8), as
/// carried in [`GameView::commander_tax`] (issue #372). **Public information** —
/// the tax climbs `{2}` per prior cast from the command zone, so every player can
/// see how much a recast will cost. Projected from the engine's per-designation
/// cast count, so it survives the commander's zone changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderTax {
    /// The commander this tax applies to, named by its owning player's id — the
    /// designation key (one commander per player today).
    pub commander: PlayerId,
    /// How many times this commander has been cast from the command zone this game
    /// (CR 903.8). Zero before its first cast.
    #[serde(default, skip_serializing_if = "crate::is_zero")]
    pub casts: u32,
    /// The generic mana the tax adds to the next cast from the command zone: `{2}`
    /// per prior cast (`2 * casts`). Zero before the first cast.
    #[serde(default, skip_serializing_if = "crate::is_zero")]
    pub tax: u32,
}

/// The terminal outcome of a game (CR 104.2a), present on a [`GameView`] only once
/// the game is over. While the game is live the field is omitted entirely (the
/// empty-optional convention), so its mere presence signals game over to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameResult {
    /// The winning player (CR 104.2a), or omitted for a draw where every remaining
    /// player lost at once (CR 104.4a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<PlayerId>,
    /// The players who lost, in seat order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub losers: Vec<PlayerId>,
    /// Why the game ended.
    pub reason: GameOverReason,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn issue_371_commander_damage_tally_uses_snake_case_and_elides_when_empty() {
        // The public commander-damage tally serializes with snake_case keys, and the
        // `commander_damage` field is omitted entirely when empty (additive shape:
        // a non-commander game and an older client see no change).
        let entry = CommanderDamage {
            commander: "p0".into(),
            damaged: "p2".into(),
            amount: 21,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["commander"], "p0");
        assert_eq!(json["damaged"], "p2");
        assert_eq!(json["amount"], 21);

        let view: GameView =
            serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
        assert!(view.commander_damage.is_empty());
        let round = serde_json::to_value(&view).unwrap();
        assert!(
            round.get("commander_damage").is_none(),
            "an empty tally is elided from the wire"
        );
    }

    #[test]
    fn issue_371_commander_damage_loss_reason_is_snake_case() {
        // CR 903.10a: the commander-damage loss reason mirrors onto the wire as a
        // snake_case `commander_damage`, distinguishable from the other reasons.
        let json = serde_json::to_value(GameOverReason::CommanderDamage).unwrap();
        assert_eq!(json, serde_json::json!("commander_damage"));
    }
}
