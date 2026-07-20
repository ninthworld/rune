//! Client → server in-game messages (docs/protocol.md).

use serde::{Deserialize, Serialize};

use crate::{EntityId, Phase};

/// The client's chosen action, answered atomically: the `id` of one issued
/// [`ValidAction`], its content-binding [`token`](ChooseAction::token), and the
/// full set of [`targets`](ChooseAction::targets) filling that action's
/// requirement slots. The server validates the id, verifies the token against the
/// action it currently offers, and checks each chosen target against that slot's
/// freshly computed legal set; anything else is rejected and the current
/// `GameView` is re-sent (docs/decisions/0009-targeting-model.md §Protocol).
///
/// `Default` yields the minimal no-choice answer (empty token and targets), so a
/// caller answering a plain action can set only `action_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseAction {
    /// The `id` of the chosen [`ValidAction`].
    pub action_id: String,
    /// The chosen action's [`ValidAction::token`], echoed verbatim. Binds this
    /// answer to the exact action content the client saw, closing the stale-`id`
    /// rebinding hole. Omitted (`""`) only for a legacy unbound action; a real
    /// server rejects an answer whose token does not match.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// One entry per [`ValidAction::requirements`] slot, carrying the entity ids
    /// the player selected. Submitted all at once (never a multi-message
    /// handshake); empty for an action with no requirements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetChoice>,
}

/// The player's answer to one choice slot — a [`TargetRequirement`] **or** a
/// [`Prompt`] — keyed back to the slot by `slot`. The same shape answers every slot
/// kind: `chosen` carries the selected ids (a target id, a [`PromptOption::id`], the
/// picked zone ids, or a full ordering). Each id must be one of that slot's
/// advertised candidates/options/items, or the server treats the action as a no-op.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetChoice {
    /// The [`TargetRequirement::slot`] or [`Prompt`] slot this answers.
    pub slot: String,
    /// The entity ids chosen for this slot: one for a single-target slot; the
    /// chosen [`PromptOption::id`] for an [`Prompt::Option`]; the selected ids for a
    /// [`Prompt::SelectFromZone`]; or the full ordering for a [`Prompt::Order`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chosen: Vec<EntityId>,
}

/// Set (or replace) this connection's **priority-stop preferences** (issue #264,
/// ADR 0020): the steps at which the seat wants priority even when it has no
/// meaningful action, so basic auto-pass does not skip it there. Server-authoritative
/// and reconnect-durable — the room stores the set per seat (like a display name) and
/// reflects it back in [`GameView::stops`]. An unparseable message is ignored and the
/// current view re-sent (the non-fatal pattern); the empty set means "stop nowhere".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetStops {
    /// The steps to stop at, as [`Phase`] values. Replaces the seat's current set
    /// wholesale (not additive). Empty (and omitted from the wire) to clear all stops.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<Phase>,
}

/// Everything a client can send about the game. Serializes with a `type`
/// discriminator (`{"type":"choose_action", ...}`) so the wire stays
/// self-describing and open to future message types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// The player chose one of the issued valid actions.
    ChooseAction(ChooseAction),
    /// The player set their priority-stop preferences (issue #264).
    SetStops(SetStops),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn choose_action_is_just_an_id() {
        let msg = ChooseAction {
            action_id: "a2".into(),
            token: String::new(),
            targets: vec![],
        };
        assert_eq!(msg.action_id, "a2");
    }

    #[test]
    fn client_message_uses_documented_wire_shape() {
        // A no-choice action: empty token and targets elide, so the minimal
        // `{type, action_id}` wire shape is preserved for backward compatibility.
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a2".into(),
            token: String::new(),
            targets: vec![],
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
    fn choose_action_carries_token_and_targets() {
        // A real targeted answer: id + content-binding token + the atomically
        // submitted selection, keyed per requirement slot.
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a3".into(),
            token: "h:9f2c".into(),
            targets: vec![TargetChoice {
                slot: "t0".into(),
                chosen: vec!["perm_bear".into()],
            }],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "choose_action",
                "action_id": "a3",
                "token": "h:9f2c",
                "targets": [{ "slot": "t0", "chosen": ["perm_bear"] }]
            })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_264_set_stops_message_uses_documented_wire_shape() {
        // The stops-preference message rides the same tagged `ClientMessage` envelope
        // as `choose_action`, carrying the stop phases as snake_case `Phase` names.
        let msg = ClientMessage::SetStops(SetStops {
            stops: vec![Phase::Upkeep, Phase::End],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "set_stops", "stops": ["upkeep", "end"] })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_264_empty_set_stops_elides_the_list() {
        // Clearing all stops sends an empty list, which elides — the minimal wire shape.
        let msg = ClientMessage::SetStops(SetStops { stops: vec![] });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "set_stops" }));
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }
}
