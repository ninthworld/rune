//! The public card catalog and per-format deck rules a lobby connection can
//! request before a game exists (issue #367, docs/decisions/0012-lobby-protocol.md).

use serde::{Deserialize, Serialize};

use crate::{CardIdentity, GameSetupId};

/// The current schema version carried in [`CatalogView::catalog_version`]. A single
/// frame carries the whole catalog today; the version leaves room to add paging (or
/// other additive fields) later without breaking older clients, so "the whole catalog
/// fits one frame" is deliberately **not** a hard wire invariant.
pub const CATALOG_VERSION: u32 = 1;

/// One supported card's public characteristics, as listed in a [`CatalogView`] (issue
/// #367). This is the browse-time counterpart of the in-game [`CardView`]: it carries
/// the card's stable identity and the same server-computed characteristics — including
/// the **generated** rules text an in-game `CardView` shows — but no per-game entity id,
/// because a catalog entry names a card by identity, not a specific instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogCard {
    /// The card's stable functional identity (ADR 0018 §3) — the same handle a
    /// [`SubmitDeck`] decklist uses ([`CardIdentity`]). Always present.
    pub functional_id: CardIdentity,
    /// Display name.
    pub name: String,
    /// The full type line, including any basic supertype, e.g. `"Basic Land — Forest"`
    /// or `"Creature — Elf Warrior"`.
    pub type_line: String,
    /// Displayed mana cost string, e.g. `"{1}{G}"`. `None`/omitted for a card without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// The card's rules text, **generated** by the server from its ability IR — byte-for-byte
    /// what an in-game [`CardView::rules_text`] shows (ADR 0018 §7). Empty (and omitted from
    /// the wire) for a vanilla card with no rules.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rules_text: String,
    /// Displayed power (a string so `*` and other non-numeric values round-trip). Present
    /// only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CatalogCard::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    /// The card's keyword abilities as lowercase wire names (e.g. `"flying"`), the same
    /// projection [`CardView::keywords`] carries. Omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// One advertised format's public deck rules and seat range, as listed in a
/// [`CatalogView`] (issue #367, ADR 0013 §4). It exposes exactly the server-side
/// deck-legality policy a [`SubmitDeck`] is validated against so a client can build a
/// legal deck ahead of time; a **permissive** format advertises its permissiveness
/// honestly, as `None` upper bounds rather than a sentinel number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogFormat {
    /// The `game_setup` identifier that names this format ([`GameSetupId`]) — the same
    /// id a [`RoomConfig`] carries to create a room using it.
    pub game_setup: GameSetupId,
    /// Fewest cards a legal deck may hold (inclusive). `0` for a format with no minimum.
    pub min_deck_size: u32,
    /// Most cards a legal deck may hold (inclusive), or `None`/omitted for no upper bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_deck_size: Option<u32>,
    /// The most copies of any single non-exempt card a deck may hold, or `None`/omitted
    /// for **no copy limit** (an honestly permissive format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_copies: Option<u32>,
    /// Whether basic lands are exempt from [`max_copies`](CatalogFormat::max_copies) (the
    /// usual Magic rule, CR 100.2a).
    pub basic_land_exempt: bool,
    /// Whether a legal deck must designate a **commander** (CR 903.3), projected from
    /// the server's deck rules so a client learns the requirement from advertised
    /// metadata instead of hardcoding the format name (issue #394). Additive and
    /// default-elided: omitted (and defaults to `false`) for a non-commander format, so
    /// the frame stays byte-for-byte the pre-#394 shape.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub requires_commander: bool,
    /// Whether the format enforces **color-identity containment** — every card's color
    /// identity must fit within the commander's (CR 903.4). Projected from the server's
    /// deck rules (issue #394). Meaningful only alongside
    /// [`requires_commander`](CatalogFormat::requires_commander). Additive and
    /// default-elided, like [`requires_commander`](CatalogFormat::requires_commander).
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub enforce_color_identity: bool,
    /// Fewest seats a room using this format may be created with (inclusive).
    pub min_seats: u8,
    /// Most seats a room using this format may be created with (inclusive).
    pub max_seats: u8,
}

/// The public card catalog and format deck rules, a lobby-phase **server → client**
/// frame answered to a [`LobbyCommand::RequestCatalog`] (issue #367). It is a versioned
/// single-frame projection of the complete supported card pool and every advertised
/// format, built server-side from the one embedded card database and the format
/// registry. Public data only — it never carries a deck, a roster, or any game state.
///
/// On the wire it is distinguished from a [`LobbyView`] by its `catalog_version` field
/// (a `LobbyView` carries none) and from a [`GameView`]/[`SpectatorView`] by carrying no
/// `phase`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogView {
    /// The projection's schema version ([`CATALOG_VERSION`]). Always present; it both
    /// versions the shape and serves as the wire discriminator against a `LobbyView`.
    pub catalog_version: u32,
    /// Every supported card, in a stable order. Omitted from the wire only for an empty
    /// catalog (a client treats a missing field as an empty list).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CatalogCard>,
    /// Every advertised format's deck rules and seat range. Omitted from the wire when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<CatalogFormat>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn issue_367_request_catalog_command_is_a_bare_tag() {
        // The catalog request is a unit command: just its `type` discriminator, like
        // `leave`, so it round-trips as `{"type":"request_catalog"}`.
        let msg = LobbyCommand::RequestCatalog;
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "request_catalog" }));
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_367_catalog_view_round_trips_and_elides_defaults() {
        // A card with rules text and P/T, and one with neither, exercise both the
        // present and the elided wire shapes; the format advertises its bounds and a
        // permissive `None` copy limit.
        let view = CatalogView {
            catalog_version: CATALOG_VERSION,
            cards: vec![
                CatalogCard {
                    functional_id: "serra_angel".into(),
                    name: "Serra Angel".into(),
                    type_line: "Creature — Angel".into(),
                    mana_cost: Some("{3}{W}{W}".into()),
                    rules_text: "Flying, vigilance".into(),
                    power: Some("4".into()),
                    toughness: Some("4".into()),
                    keywords: vec!["flying".into(), "vigilance".into()],
                },
                CatalogCard {
                    functional_id: "forest".into(),
                    name: "Forest".into(),
                    type_line: "Basic Land — Forest".into(),
                    mana_cost: None,
                    rules_text: "{T}: Add {G}.".into(),
                    power: None,
                    toughness: None,
                    keywords: vec![],
                },
            ],
            formats: vec![CatalogFormat {
                game_setup: "standard_2p".into(),
                min_deck_size: 0,
                max_deck_size: None,
                max_copies: None,
                basic_land_exempt: true,
                requires_commander: false,
                enforce_color_identity: false,
                min_seats: 2,
                max_seats: 8,
            }],
        };
        let json = serde_json::to_value(&view).unwrap();
        // The version is the wire discriminator (a `LobbyView` never carries it).
        assert_eq!(json["catalog_version"], 1);
        // A basic land elides its absent mana cost and P/T; a permissive format elides
        // its `None` upper bounds.
        assert_eq!(json["cards"][1].get("mana_cost"), None);
        assert_eq!(json["cards"][1].get("power"), None);
        assert_eq!(json["cards"][1].get("keywords"), None);
        assert_eq!(json["formats"][0].get("max_copies"), None);
        assert_eq!(json["formats"][0].get("max_deck_size"), None);
        assert_eq!(json["formats"][0]["min_deck_size"], 0);
        // The additive #394 flags are default-elided: a non-commander format writes
        // neither, so the frame stays byte-for-byte the pre-#394 shape.
        assert_eq!(json["formats"][0].get("requires_commander"), None);
        assert_eq!(json["formats"][0].get("enforce_color_identity"), None);
        let back: CatalogView = serde_json::from_value(json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn issue_394_catalog_format_advertises_commander_deck_rules() {
        // A commander format advertises both deck-rule facts as `true`, and they ride
        // the wire so a client learns the requirement from metadata, not a format name.
        let format = CatalogFormat {
            game_setup: "commander".into(),
            min_deck_size: 100,
            max_deck_size: Some(100),
            max_copies: Some(1),
            basic_land_exempt: true,
            requires_commander: true,
            enforce_color_identity: true,
            min_seats: 2,
            max_seats: 4,
        };
        let json = serde_json::to_value(&format).unwrap();
        assert_eq!(json["requires_commander"], true);
        assert_eq!(json["enforce_color_identity"], true);
        let back: CatalogFormat = serde_json::from_value(json).unwrap();
        assert_eq!(back, format);

        // An older frame that omits both flags still deserializes, defaulting each to
        // `false` (backward compatibility).
        let legacy = serde_json::json!({
            "game_setup": "standard_2p",
            "min_deck_size": 0,
            "basic_land_exempt": true,
            "min_seats": 2,
            "max_seats": 8,
        });
        let parsed: CatalogFormat = serde_json::from_value(legacy).unwrap();
        assert!(!parsed.requires_commander);
        assert!(!parsed.enforce_color_identity);
    }
}
