//! RUNE protocol — the entire client/server contract.
//!
//! Two message types (docs/protocol.md):
//! - Server -> client: a personalized [`GameView`]
//! - Client -> server: a [`ClientMessage`] (only variant: [`ChooseAction`])
//!
//! Everything here serializes to the JSON documented in `docs/protocol.md`. Any
//! change to these shapes must update that document in the same PR. Clients and
//! server tolerate unknown fields (serde ignores them) so the wire format can
//! grow without breaking older clients — see the forward-compat test below.

use serde::{Deserialize, Serialize};

/// Opaque player identity (server-assigned).
pub type PlayerId = String;

/// Opaque per-game entity id: a card, permanent, or stack object.
pub type EntityId = String;

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
    /// Rules text as displayed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub oracle_text: String,
    /// Displayed power (a string so `*` and other non-numeric values round-trip).
    /// Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CardView::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub tapped: bool,
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

/// One entry of [`GameView::valid_actions`]. The client renders these; it never
/// invents its own. `subject` names the entities this action belongs to so the
/// client can put the action ON the card rather than in a global bar
/// (docs/decisions/0004-subject-owned-actions.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidAction {
    /// Opaque id the client echoes back in [`ChooseAction`] to take this action.
    pub id: String,
    /// Coarse action category (e.g. `"pass_priority"`, `"activate_ability"`).
    /// A free-form string, not an enum, so new action kinds do not break older
    /// clients that only key off `subject` and `label`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Human-readable label to render for this action.
    pub label: String,
    /// Entity ids this action belongs to; empty for global actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject: Vec<String>,
}

/// The personalized state the server sends after every change (docs/protocol.md).
/// Hidden information is redacted server-side before this is built. A client must
/// be able to fully reconstruct its UI from a single `GameView` — no client state
/// is load-bearing across messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameView {
    /// Full card objects for the receiving player only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub my_hand: Vec<CardView>,
    /// Redacted views of every other player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opponents: Vec<OpponentView>,
    /// All permanents in play.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battlefield: Vec<Permanent>,
    /// The stack, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackItem>,
    /// Each player's graveyard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyards: Vec<ZonePile>,
    /// Each player's exile zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<ZonePile>,
    /// The current turn step.
    pub phase: Phase,
    /// The receiving player's unspent mana, as pip strings (e.g. `["{G}", "{G}"]`).
    /// Server-computed; the client only displays it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_pool: Vec<String>,
    /// The player who currently holds priority, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    /// The only source of interactivity: what the receiving player may do now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_actions: Vec<ValidAction>,
    /// Seconds remaining for the pending decision, if a clock is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_deadline: Option<f64>,
}

/// The client's chosen action: just the `id` of one issued `valid_actions` entry.
/// The server validates it against the actions it issued; anything else is
/// rejected and the current `GameView` is re-sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseAction {
    /// The `id` of the chosen [`ValidAction`].
    pub action_id: String,
}

/// Everything a client can send about the game. Serializes with a `type`
/// discriminator (`{"type":"choose_action", ...}`) so the wire stays
/// self-describing and open to future message types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// The player chose one of the issued valid actions.
    ChooseAction(ChooseAction),
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // panics are the failure signal in tests
mod tests {
    use super::*;

    #[test]
    fn choose_action_is_just_an_id() {
        let msg = ChooseAction {
            action_id: "a2".into(),
        };
        assert_eq!(msg.action_id, "a2");
    }

    #[test]
    fn client_message_uses_documented_wire_shape() {
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a2".into(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "choose_action", "action_id": "a2" })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn valid_action_serializes_type_and_omits_empty_subject() {
        let pass = ValidAction {
            id: "a1".into(),
            kind: "pass_priority".into(),
            label: "Pass".into(),
            subject: vec![],
        };
        let json = serde_json::to_value(&pass).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "a1", "type": "pass_priority", "label": "Pass" })
        );
    }

    #[test]
    fn game_view_round_trips_through_json() {
        let view = GameView {
            my_hand: vec![CardView {
                id: "c1".into(),
                name: "Llanowar Elves".into(),
                type_line: "Creature — Elf Druid".into(),
                mana_cost: Some("{G}".into()),
                oracle_text: "{T}: Add {G}.".into(),
                power: Some("1".into()),
                toughness: Some("1".into()),
            }],
            opponents: vec![OpponentView {
                player_id: "p2".into(),
                hand_size: 7,
                life: 20,
                library_size: 53,
                graveyard_size: 0,
                statuses: vec!["monarch".into()],
            }],
            battlefield: vec![Permanent {
                id: "perm_xyz".into(),
                controller: "p1".into(),
                owner: "p1".into(),
                card: CardView {
                    id: "perm_xyz".into(),
                    name: "Grizzly Bears".into(),
                    type_line: "Creature — Bear".into(),
                    mana_cost: Some("{1}{G}".into()),
                    oracle_text: String::new(),
                    power: Some("2".into()),
                    toughness: Some("2".into()),
                },
                tapped: true,
                counters: vec![Counter {
                    kind: "+1/+1".into(),
                    count: 2,
                }],
            }],
            stack: vec![StackItem {
                id: "s1".into(),
                controller: "p2".into(),
                description: "Lightning Bolt".into(),
                source: None,
            }],
            graveyards: vec![ZonePile {
                player_id: "p1".into(),
                cards: vec![],
            }],
            exile: vec![],
            phase: Phase::PrecombatMain,
            mana_pool: vec!["{G}".into()],
            priority_player: Some("p1".into()),
            valid_actions: vec![ValidAction {
                id: "a2".into(),
                kind: "activate_ability".into(),
                label: "Tap for mana".into(),
                subject: vec!["perm_xyz".into()],
            }],
            action_deadline: Some(12.5),
        };

        let json = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn empty_game_view_round_trips() {
        let view = GameView {
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::Upkeep,
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
        };
        let json = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn mana_pool_is_omitted_when_empty_and_round_trips_when_present() {
        let mut view = GameView {
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::PrecombatMain,
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
        };
        // Empty pool is elided from the wire.
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("mana_pool").is_none());

        // A non-empty pool round-trips as a list of pip strings.
        view.mana_pool = vec!["{G}".into(), "{G}".into()];
        let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(back.mana_pool, vec!["{G}".to_string(), "{G}".to_string()]);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // Forward-compat invariant (docs/protocol.md): a newer server may add
        // fields; older clients must still deserialize the message.
        let json = r#"{ "phase": "draw", "some_future_field": 42 }"#;
        let view: GameView = serde_json::from_str(json).unwrap();
        assert_eq!(view.phase, Phase::Draw);
        assert!(view.my_hand.is_empty());
    }
}
