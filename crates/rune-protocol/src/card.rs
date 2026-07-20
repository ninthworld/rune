//! In-game card, board, and zone views — the public and self-visible pieces a
//! [`GameView`](crate::GameView) is assembled from.

use serde::{Deserialize, Serialize};

use crate::{EntityId, PlayerId};

/// A card object, shown only to a player entitled to see it (`my_hand`, public
/// zones, revealed cards). Characteristics are server-computed; the client never
/// derives them. Grows alongside the card database (backlog: engine card loader).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardView {
    /// Entity id of this card instance.
    pub id: EntityId,
    /// Display name.
    pub name: String,
    /// e.g. `"Creature — Elf Warrior"`.
    pub type_line: String,
    /// Displayed mana cost string, e.g. `"{1}{G}"`. `None` for cards without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// The card's rules text, **generated** by the server from the card's ability IR
    /// (ADR 0018 §7) — never a stored or upstream string, and never exact Oracle text.
    /// Written to be semantically complete for play; matching official wording is not
    /// a goal. Empty (and omitted from the wire) for a card with no rules.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rules_text: String,
    /// The card's stable **presentation identity**: the `functional_id` of the card
    /// definition this object is a copy of (ADR 0018 §3, §8).
    ///
    /// Unlike [`CardView::id`], which is a per-game entity handle, this is the same
    /// string for every copy of a card in every game, and it survives a server rebuild.
    /// It exists so a future *client-local* cache can look a card up by identity to
    /// enrich its presentation; the server neither has nor requires such a cache, and a
    /// client that ignores this field renders the card completely from `rules_text`.
    /// Empty only for a card the server cannot resolve (a defensive placeholder).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub functional_id: String,
    /// Displayed power (a string so `*` and other non-numeric values round-trip).
    /// Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CardView::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    /// The card's keyword abilities as lowercase wire names (e.g. `"flying"`,
    /// `"first_strike"`), server-computed for display; the client renders badges
    /// and never derives them. Omitted from the wire when the card has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// What the receiving player is allowed to know about an opponent: hidden zones
/// are reduced to counts, public state is exact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpponentView {
    /// Which opponent this describes.
    pub player_id: PlayerId,
    /// Number of cards in hand (contents hidden).
    pub hand_size: u32,
    /// Current life total.
    pub life: i32,
    /// Number of cards left in library.
    pub library_size: u32,
    /// Number of cards in the graveyard.
    pub graveyard_size: u32,
    /// Free-form status labels (e.g. `"monarch"`, `"hexproof"`) for display only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
    /// Whether this opponent has been eliminated — they lost while the game
    /// continued and left it (CR 800.4a, issue #342/#345). Additive: omitted (and
    /// defaults to `false`) so a two-player view is unchanged; the client shows an
    /// eliminated opponent as out of the game. Server-computed from the player's
    /// stored loss state; never derived by the client.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub eliminated: bool,
}

/// The receiver's own public stats — the self-counterpart of [`OpponentView`].
///
/// A player is entitled to see their own public state, but [`GameView`] historically
/// carried none of it: hand *contents* ride in `my_hand` and unspent mana in
/// `mana_pool`, yet the two public numbers every opponent already sees about this
/// player — life total and library size — had no home, so a player could see everyone's
/// life but their own. This is that home; it exposes no hidden information (a player's
/// own life and library size are public).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfView {
    /// The receiver's current life total.
    pub life: i32,
    /// Number of cards left in the receiver's library.
    pub library_size: u32,
}

/// A permanent on the battlefield with its server-computed characteristics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permanent {
    /// Entity id of this permanent.
    pub id: EntityId,
    /// Player who currently controls it.
    pub controller: PlayerId,
    /// Player who owns it (matters when control changes).
    pub owner: PlayerId,
    /// The permanent's current (computed) card face.
    pub card: CardView,
    /// Whether the permanent is tapped.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub tapped: bool,
    /// Whether this permanent is currently attacking — declared as an attacker
    /// this combat (CR 508). Server-computed; the client displays it and never
    /// derives it. Omitted from the wire when `false`.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub attacking: bool,
    /// The defending player this permanent is attacking (CR 508.1a), as their
    /// entity id — the multiplayer generalization of [`Self::attacking`] (issue
    /// #341/#345). Additive: omitted (and defaults to `None`) when the permanent is
    /// not attacking, and in a two-player game a client may ignore it since the sole
    /// opponent is the only possible defender; with more seats it names *whom* the
    /// attacker attacks so the board can render split attacks. Follows the
    /// `blocking`/`attached_to` precedent of projecting one object's reference to
    /// another. Server-computed; never derived by the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacking_player: Option<EntityId>,
    /// The permanent this one is blocking, if it was declared as a blocker this
    /// combat (CR 509): the attacker's entity id. `None`/omitted when it is not
    /// blocking. Several blockers may name the same attacker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking: Option<EntityId>,
    /// Damage marked on this permanent this turn (CR 120.3), the value the
    /// lethal-damage state-based action compares against toughness (CR 704.5g).
    /// Server-computed; the client displays it and never derives it. Cleared at
    /// cleanup (CR 514.2). `0`/omitted when no damage is marked.
    #[serde(default, skip_serializing_if = "crate::is_zero")]
    pub damage: u32,
    /// The permanent this one is attached to, if any (CR 303.4): an Aura on the
    /// battlefield names the object it enchants, as that host's entity id — the
    /// same `PermanentId`→`EntityId` projection [`blocking`](Self::blocking) uses.
    /// `None`/omitted for an unattached permanent. Server-computed; the client
    /// clusters the attachment with its host and derives no rules from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<EntityId>,
    /// Named counters and their quantities, e.g. `{"+1/+1": 2}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<Counter>,
}

/// A named counter on a permanent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counter {
    /// Counter name, e.g. `"+1/+1"` or `"loyalty"`.
    pub kind: String,
    /// How many of this counter are present.
    pub count: u32,
}

/// One object on the stack — a spell or an ability. Ability entries carry their
/// source permanent so the client can point back at it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackItem {
    /// Entity id of this stack object.
    pub id: EntityId,
    /// Player who controls it (chooses targets/resolution).
    pub controller: PlayerId,
    /// Spell name or ability text as it should be displayed.
    pub description: String,
    /// Source permanent for an ability; `None` for a spell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<EntityId>,
}

/// A public, ordered pile owned by one player (graveyard or exile).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZonePile {
    /// Player who owns the pile.
    pub player_id: PlayerId,
    /// Cards in zone order (top last).
    pub cards: Vec<CardView>,
}

/// The current turn step. The full sequence lives in the engine's phase FSM
/// (backlog); the protocol carries the current step for overview/focus rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Untap step.
    Untap,
    /// Upkeep step.
    Upkeep,
    /// Draw step.
    Draw,
    /// Precombat main phase.
    PrecombatMain,
    /// Beginning of combat step.
    BeginCombat,
    /// Declare attackers step.
    DeclareAttackers,
    /// Declare blockers step.
    DeclareBlockers,
    /// Combat damage step.
    CombatDamage,
    /// End of combat step.
    EndCombat,
    /// Postcombat main phase.
    PostcombatMain,
    /// End step.
    End,
    /// Cleanup step.
    Cleanup,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn issue_255_self_view_round_trips_and_defaults_when_omitted() {
        // The receiver's own public stats round-trip on their own...
        let me = SelfView {
            life: 15,
            library_size: 40,
        };
        let back: SelfView = serde_json::from_str(&serde_json::to_string(&me).unwrap()).unwrap();
        assert_eq!(back, me);

        // ...and a GameView from an older server that omits `me` still deserializes,
        // defaulting to a zero placeholder rather than failing (the `you`-field pattern).
        let view: GameView =
            serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
        assert_eq!(view.me, SelfView::default());
        assert_eq!(view.me.life, 0);
    }

    #[test]
    fn permanent_combat_state_round_trips_and_elides_when_absent() {
        // Attack/block state (issue #117) and marked damage (issue #118):
        // `attacking`, `blocking`, and `damage` round-trip when present, and all
        // elide from the wire in the common not-in-combat, undamaged case so the
        // serialized shape is unchanged for non-combat permanents.
        let base = Permanent {
            id: "perm_1".into(),
            controller: "p0".into(),
            owner: "p0".into(),
            card: CardView {
                id: "perm_1".into(),
                name: "Grizzly Bears".into(),
                type_line: "Creature — Bear".into(),
                mana_cost: Some("{1}{G}".into()),
                rules_text: String::new(),
                functional_id: String::new(),
                power: Some("2".into()),
                toughness: Some("2".into()),
                keywords: vec![],
            },
            tapped: false,
            attacking: false,
            attacking_player: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            counters: vec![],
        };

        // Not in combat and undamaged: all three fields elide from the JSON.
        let json = serde_json::to_value(&base).unwrap();
        assert!(json.get("attacking").is_none());
        assert!(json.get("blocking").is_none());
        assert!(json.get("damage").is_none());

        // An attacker and its blocker both round-trip with their state present.
        let attacker = Permanent {
            attacking: true,
            attacking_player: None,
            ..base.clone()
        };
        let blocker = Permanent {
            blocking: Some("perm_1".into()),
            ..base.clone()
        };
        let attacker_json = serde_json::to_value(&attacker).unwrap();
        assert_eq!(
            attacker_json.get("attacking"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            serde_json::from_value::<Permanent>(attacker_json).unwrap(),
            attacker
        );
        let blocker_json = serde_json::to_value(&blocker).unwrap();
        assert_eq!(
            blocker_json.get("blocking"),
            Some(&serde_json::json!("perm_1"))
        );
        assert_eq!(
            serde_json::from_value::<Permanent>(blocker_json).unwrap(),
            blocker
        );

        // Marked damage round-trips when non-zero and serializes as a number.
        let damaged = Permanent {
            damage: 2,
            ..base.clone()
        };
        let damaged_json = serde_json::to_value(&damaged).unwrap();
        assert_eq!(damaged_json.get("damage"), Some(&serde_json::json!(2)));
        assert_eq!(
            serde_json::from_value::<Permanent>(damaged_json).unwrap(),
            damaged
        );
    }

    #[test]
    fn permanent_attachment_round_trips_and_elides_when_absent() {
        // Aura attachment (issue #333, CR 303.4): `attached_to` names the host's
        // entity id when the permanent is attached, round-trips through the wire,
        // and elides entirely for an unattached permanent so the common non-Aura
        // shape is unchanged.
        let base = Permanent {
            id: "perm_1".into(),
            controller: "p0".into(),
            owner: "p0".into(),
            card: CardView {
                id: "perm_1".into(),
                name: "Ironbark Aegis".into(),
                type_line: "Enchantment — Aura".into(),
                mana_cost: Some("{1}{G}".into()),
                rules_text: "Enchant creature".into(),
                functional_id: String::new(),
                power: None,
                toughness: None,
                keywords: vec![],
            },
            tapped: false,
            attacking: false,
            attacking_player: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            counters: vec![],
        };

        // Unattached: the field elides from the JSON.
        let json = serde_json::to_value(&base).unwrap();
        assert!(json.get("attached_to").is_none());

        // Attached: the host id round-trips and serializes as a string.
        let attached = Permanent {
            attached_to: Some("perm_9".into()),
            ..base.clone()
        };
        let attached_json = serde_json::to_value(&attached).unwrap();
        assert_eq!(
            attached_json.get("attached_to"),
            Some(&serde_json::json!("perm_9"))
        );
        assert_eq!(
            serde_json::from_value::<Permanent>(attached_json).unwrap(),
            attached
        );
    }

    #[test]
    fn issue_153_card_keywords_round_trip_and_elide_when_absent() {
        // Keyword abilities (issue #153) surface on a CardView as lowercase wire
        // names for display; the list round-trips when present and elides from the
        // JSON when the card has none, so a keyword-less card keeps its terse shape.
        let base = CardView {
            id: "c1".into(),
            name: "Snapping Drake".into(),
            type_line: "Creature — Drake".into(),
            mana_cost: Some("{3}{U}".into()),
            rules_text: "Flying".into(),
            functional_id: "snapping_drake".into(),
            power: Some("3".into()),
            toughness: Some("2".into()),
            keywords: vec!["flying".into()],
        };
        let json = serde_json::to_value(&base).unwrap();
        assert_eq!(json.get("keywords"), Some(&serde_json::json!(["flying"])));
        assert_eq!(serde_json::from_value::<CardView>(json).unwrap(), base);

        // A card with no keywords omits the field entirely.
        let vanilla = CardView {
            keywords: vec![],
            ..base.clone()
        };
        let vanilla_json = serde_json::to_value(&vanilla).unwrap();
        assert!(vanilla_json.get("keywords").is_none());
    }
}
