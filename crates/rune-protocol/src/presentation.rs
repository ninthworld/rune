//! In-match **presentation metadata** (issue #553): the authoritative facts a
//! client needs to render a match's format and its seats' commander identity
//! *without inferring anything*.
//!
//! Two shapes live here, both **public** (spectator-safe) and both server-computed:
//!
//! - [`MatchFormat`] — what format the match is played under, so a client knows it
//!   is a Commander game even when the command zones, the tax list and the damage
//!   tally are all empty (every commander on the battlefield is exactly that state).
//! - [`CommanderIdentity`] — a seat's commander name and color identity (CR 903.4),
//!   keyed to the *designation* rather than to any object, so it survives the
//!   commander changing zones. Previously the only source was the `command` pile,
//!   which vanishes the moment the commander is cast.
//!
//! Neither carries hidden information: a commander is announced before the game
//! (CR 903.3), and the format is a public property of the room.

use serde::{Deserialize, Serialize};

use crate::{GameSetupId, PlayerId};

/// Every [`Color`] in WUBRG order, the canonical order a color identity is
/// serialized in. The single source of truth for the closed set, mirrored by the
/// TypeScript `COLORS` tuple so the union there cannot drift from its validator.
pub const COLORS: [Color; 5] = [
    Color::White,
    Color::Blue,
    Color::Black,
    Color::Red,
    Color::Green,
];

/// One of Magic's five colors (CR 105.1), on the wire as its conventional single
/// uppercase letter — `"W"`, `"U"`, `"B"`, `"R"`, `"G"`. A **closed** set: unlike
/// the free-form action `kind` strings, the five colors cannot grow, so a client
/// may match them exhaustively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Color {
    /// White (`"W"`).
    #[serde(rename = "W")]
    White,
    /// Blue (`"U"`).
    #[serde(rename = "U")]
    Blue,
    /// Black (`"B"`).
    #[serde(rename = "B")]
    Black,
    /// Red (`"R"`).
    #[serde(rename = "R")]
    Red,
    /// Green (`"G"`).
    #[serde(rename = "G")]
    Green,
}

/// The **format** a match is played under (issue #553), carried on every
/// [`GameView`](crate::GameView) and [`SpectatorView`](crate::SpectatorView).
///
/// This is the format *signal* a client renders format-specific presentation from.
/// It exists because every other commander-shaped field is legitimately empty in
/// ordinary Commander states — a game whose commanders are all on the battlefield
/// has an empty `command`, an all-zero (elided) `commander_tax`, and an empty
/// `commander_damage` — so "is this Commander?" cannot be inferred from zone
/// contents without being wrong.
///
/// Absent from the wire ⇒ **unknown id, not a Commander game**: a client that
/// receives no `format` renders exactly what it rendered before this field
/// existed. Room state, not engine state — the room owns it, exactly like
/// `player_names`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchFormat {
    /// The room's `game_setup` identifier (e.g. `"standard"`, `"commander"`). Free
    /// form — the server's format registry may grow — so a client keys presentation
    /// off the typed flag below and uses this only as a label. Omitted when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: GameSetupId,
    /// Whether this match is played under the **Commander** rules (CR 903): the
    /// typed signal a client keys commander-specific presentation off, rather than
    /// string-matching [`Self::id`] or guessing from zone contents. Omitted from the
    /// wire when `false`, so a non-Commander match is unchanged.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub commander: bool,
}

/// One seat's **commander identity** (issue #553): the name and color identity of
/// the commander that seat designated before the game (CR 903.3/903.4).
///
/// Keyed by the owning player's id — the same designation key
/// [`CommanderDamage`](crate::CommanderDamage) and [`CommanderTax`](crate::CommanderTax)
/// use, since one player designates at most one commander today — so it is
/// **stable for the whole game**: it does not change when the commander is cast,
/// dies, is exiled, or returns to the command zone. That stability is the point.
/// The only previous source, the `command` pile's `CardView`, disappears the
/// instant the commander leaves the command zone, which made a seat's identity gem
/// and nameplate flicker with the commander's location.
///
/// **Public information**: a commander is revealed at the start of the game, and its
/// color identity is what the deck was validated against, so every seat and every
/// spectator sees the same entry. Server-computed; the client never derives a color
/// identity from a mana cost or a name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderIdentity {
    /// The commander this describes, named by its owning player's id — the
    /// designation key (one commander per player today).
    pub commander: PlayerId,
    /// The commander card's display name (CR 903.3). Empty (and omitted) only for a
    /// card the server cannot resolve, a defensive placeholder.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The commander's **color identity** (CR 903.4) in WUBRG order: the colors of
    /// its mana cost plus any colored mana symbols in its rules text, which is what
    /// a Commander deck is validated against. Empty (and omitted) for a colorless
    /// commander — a legal, meaningful value, not a missing one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub color_identity: Vec<Color>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn issue_553_colors_serialize_as_single_wubrg_letters() {
        // The closed color set rides the wire as its conventional letter, in WUBRG
        // order; the `COLORS` constant is the single source of that order.
        assert_eq!(
            serde_json::to_value(COLORS).unwrap(),
            serde_json::json!(["W", "U", "B", "R", "G"])
        );
        let back: Vec<Color> = serde_json::from_value(serde_json::json!(["G", "W"])).unwrap();
        assert_eq!(back, vec![Color::Green, Color::White]);
    }

    #[test]
    fn issue_553_match_format_elides_at_its_non_commander_default() {
        // A non-Commander format carries only its id; the `commander` flag elides,
        // so the absent-field default ("not a Commander game") is the wire default.
        let standard = MatchFormat {
            id: "standard".into(),
            commander: false,
        };
        assert_eq!(
            serde_json::to_value(&standard).unwrap(),
            serde_json::json!({ "id": "standard" })
        );

        // A Commander format states it explicitly and round-trips.
        let commander = MatchFormat {
            id: "commander".into(),
            commander: true,
        };
        let json = serde_json::to_value(&commander).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "commander", "commander": true })
        );
        assert_eq!(
            serde_json::from_value::<MatchFormat>(json).unwrap(),
            commander
        );

        // An empty object (an older server's absent format) is the safe default.
        let unknown: MatchFormat = serde_json::from_str("{}").unwrap();
        assert!(unknown.id.is_empty());
        assert!(!unknown.commander);
    }

    #[test]
    fn issue_553_commander_identity_round_trips_and_elides_a_colorless_identity() {
        let jedit = CommanderIdentity {
            commander: "p0".into(),
            name: "Jedit Ojanen".into(),
            color_identity: vec![Color::Green],
        };
        let json = serde_json::to_value(&jedit).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "commander": "p0",
                "name": "Jedit Ojanen",
                "color_identity": ["G"]
            })
        );
        assert_eq!(
            serde_json::from_value::<CommanderIdentity>(json).unwrap(),
            jedit
        );

        // A colorless commander has an *empty* identity, which is a real value —
        // it elides from the wire exactly like every other empty collection.
        let colorless = CommanderIdentity {
            commander: "p1".into(),
            name: "Karn".into(),
            color_identity: Vec::new(),
        };
        let json = serde_json::to_value(&colorless).unwrap();
        assert!(json.get("color_identity").is_none());
        assert_eq!(
            serde_json::from_value::<CommanderIdentity>(json).unwrap(),
            colorless
        );
    }
}
