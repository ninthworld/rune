//! Client → server in-game messages (docs/protocol.md).

use serde::{Deserialize, Serialize};

use crate::{EntityId, Phase};

/// The client's chosen action, answered atomically: the `id` of one issued
/// [`ValidAction`], its content-binding [`token`](ChooseAction::token), and the
/// full set of [`targets`](ChooseAction::targets) filling that action's
/// requirement slots. The server validates the id, verifies the token against the
/// action it currently offers, and checks each chosen target against that slot's
/// freshly computed legal set; anything else is rejected and the current
/// `GameView` is re-sent (docs/decisions/0004-targeting-model.md §Protocol).
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
    /// An opaque, client-generated **correlation id** for this submission (issue
    /// #554). The server echoes it verbatim in
    /// [`ActionAck::submission`](crate::ActionAck) on the one view that answers this
    /// message, so a pending indicator clears on *its own* answer rather than on
    /// whichever broadcast happens to arrive next (another seat's action produces one
    /// too). Never parsed, never interpreted, and never part of the content
    /// [`token`](ValidAction::token) — it identifies the *message*, not the action,
    /// so resubmitting the same action with a new id is a new submission.
    ///
    /// Optional and omitted when empty: a client that does not correlate sends
    /// exactly the message it sent before, and the server simply issues no ack.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub submission: String,
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
/// ADR 0010): the steps at which the seat wants priority even when it has no
/// meaningful action, so basic auto-pass does not skip it there. Server-authoritative
/// and reconnect-durable — the room stores the set per seat (like a display name) and
/// reflects it back in [`GameView::stops`]/[`GameView::own_turn_stops`]. An
/// unparseable message is ignored and the current view re-sent (the non-fatal
/// pattern); two empty sets mean "stop nowhere".
///
/// **The message is authoritative for both lists at once.** It replaces the seat's
/// whole preference, which is what lets a player *clear* the human default stops
/// issue #455 seeds — a bare `{"type":"set_stops"}` means "stop nowhere", not
/// "leave my defaults alone".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetStops {
    /// The steps to stop at on **any** player's turn, as [`Phase`] values. Replaces
    /// the seat's current set wholesale (not additive). Empty (and omitted from the
    /// wire) to clear them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<Phase>,
    /// The steps to stop at **only while this seat is the active player** (issue
    /// #455). The narrower half of the preference: a stop at your own main phase is
    /// how a human keeps the turn from fast-forwarding out from under them, while
    /// the same step on an opponent's turn stays auto-passed because there is
    /// nothing there to decide.
    ///
    /// A step listed in both is stopped at on every turn — [`Self::stops`] is the
    /// wider claim and wins. Additive: an older client sends only `stops` and gets
    /// exactly the behavior it always got.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub own_turn: Vec<Phase>,
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
    /// **Undo**: restore the game to the state before the last accepted transition
    /// (issue #648). Serializes as the bare tag `{"type":"undo"}`.
    ///
    /// It carries nothing, and that is the whole shape of the feature. *Which* state to
    /// restore is never the client's to name — the server holds the checkpoints, so a
    /// message that named one would be a client asserting a game state. The sender is
    /// the connection's own seat, and the only thing being said is "take the last one
    /// back".
    ///
    /// A separate message rather than a [`ValidAction`](crate::ValidAction) because an
    /// undo is not a play: it is not offered by the rules, it takes no priority, and it
    /// is legal for a seat that is not being asked anything. Availability still rides
    /// the view — [`GameView::undo`](crate::GameView::undo) — so the client renders the
    /// control from what the server stated and computes no legality, exactly as it does
    /// for `set_stops`.
    ///
    /// Rejected — the table does not allow undo, or nothing earlier is left to restore —
    /// the server changes nothing and re-sends the sender's current `GameView` with
    /// [`action_rejected`](crate::GameView::action_rejected) set, the same non-fatal
    /// pattern a stale `choose_action` gets.
    Undo,
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
            ..Default::default()
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
            ..Default::default()
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
            submission: String::new(),
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
            ..Default::default()
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
        let msg = ClientMessage::SetStops(SetStops::default());
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "set_stops" }));
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_648_undo_is_a_bare_tag_that_names_no_state() {
        // The whole message: a type and nothing else. Which state to restore is the
        // server's to know, and the sender is the connection's own seat, so there is
        // nothing left for the client to say.
        let msg = ClientMessage::Undo;
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "undo" }));
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_455_set_stops_carries_the_own_turn_half_and_stays_additive() {
        // The own-turn half rides its own list, so the two halves of the preference
        // are never confused for one another on the wire.
        let msg = ClientMessage::SetStops(SetStops {
            stops: vec![Phase::End],
            own_turn: vec![Phase::PrecombatMain, Phase::PostcombatMain],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "set_stops",
                "stops": ["end"],
                "own_turn": ["precombat_main", "postcombat_main"],
            })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);

        // An older client that never learned the field sends exactly what it always
        // sent, and the absent list reads as empty.
        let older: ClientMessage =
            serde_json::from_value(serde_json::json!({ "type": "set_stops", "stops": ["upkeep"] }))
                .unwrap();
        assert_eq!(
            older,
            ClientMessage::SetStops(SetStops {
                stops: vec![Phase::Upkeep],
                own_turn: Vec::new(),
            })
        );
    }
}
