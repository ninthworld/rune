//! The direct-manipulation half of the action contract (issue #554): where an
//! action may be *taken to*, and the acknowledgement that closes the loop on a
//! submission.
//!
//! Both shapes exist for the same reason the rest of `valid_actions` does — so the
//! client never computes anything. [`ActionDestination`] answers "where may I drop
//! this?" with server-authoritative zones, entities, and players instead of a
//! client-side table of which card type belongs in which zone; [`ActionAck`] answers
//! "did my click land?" by naming the exact submission a view responds to, which
//! [`GameView::action_rejected`](crate::GameView) alone never could.

use serde::{Deserialize, Serialize};

use crate::PlayerId;

/// One server-authoritative **destination** an action may be taken to (issue #554):
/// the drop regions a direct-manipulation gesture is allowed to offer.
///
/// The client derives its drop regions from **exactly** this list and **fails
/// closed**: an action with no `destinations` has no drop target at all, and an
/// entry whose [`kind`](Self::kind) it does not recognize is ignored rather than
/// guessed at. That is the whole point — a client that decided "a land is dropped on
/// the battlefield, a spell on the stack" would be encoding rules, and would be
/// wrong the first time a card says otherwise.
///
/// Drag remains **optional input**. Every action reachable by a drop is reachable by
/// clicking the action, by keyboard, and by touch; destinations only tell a client
/// where a drag *may* release, never how an action must be taken.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDestination {
    /// What this destination names — `"zone"`, `"entity"`, or `"player"` today. A
    /// free-form string, not an enum, for the same reason [`ValidAction::kind`]
    /// is: new destination kinds must not break older clients, which ignore what
    /// they do not recognize (and so simply offer no drop region for it).
    #[serde(rename = "type")]
    pub kind: String,
    /// The destination itself: a zone name (`"battlefield"`, `"stack"`,
    /// `"command"`) for a `zone`, an [`EntityId`](crate::EntityId) for an `entity`,
    /// or a [`PlayerId`] for a `player`. Opaque — the client matches it against the
    /// surfaces it renders and never parses it.
    pub id: String,
    /// Whose copy of a per-player zone this is (a graveyard, a command zone).
    /// Omitted for a shared zone such as the battlefield or the stack, and for
    /// entity/player destinations, which name their own subject.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub owner: PlayerId,
    /// Human-readable label for the drop region, when the server has something more
    /// useful to say than the surface's own name. Omitted otherwise, in which case
    /// the client labels the region however it already labels that surface.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// The server's **acknowledgement** of one submitted action (issue #554), carried on
/// the [`GameView`](crate::GameView) that answers it.
///
/// Before this, a client sent a [`ChooseAction`](crate::ChooseAction) and watched for
/// *some* view to arrive. It could not tell that view apart from an unrelated
/// broadcast caused by another seat, so a pending indicator either cleared on the
/// wrong message or had to be timed out; and
/// [`action_rejected`](crate::GameView::action_rejected), the only feedback there
/// was, says *that* something was rejected, never *which* submission.
///
/// The correlation id is the client's own: it puts an opaque
/// [`ChooseAction::submission`](crate::ChooseAction::submission) on the message and
/// the server echoes it back here verbatim, never parsing or deriving it. A view that
/// answers no submission (an ordinary broadcast, a resync, a reconnect) carries no
/// ack at all, so the ack's presence is itself the signal. Transient and advisory
/// like `action_rejected`: the UI reconstructs fully without it, and it is delivered
/// exactly once, on the one view that answers the submission.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionAck {
    /// The [`ChooseAction::submission`](crate::ChooseAction::submission) this view
    /// answers, echoed verbatim.
    pub submission: String,
    /// Whether the server **applied** the submitted action. `false` means it was
    /// rejected and the game is unchanged — the same event
    /// [`GameView::action_rejected`](crate::GameView::action_rejected) flags, now
    /// tied to a specific submission. Always present (this type is itself optional),
    /// so a client never has to read an absence as a verdict.
    pub accepted: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

    #[test]
    fn issue_554_destination_uses_the_type_key_and_elides_its_optional_halves() {
        // A shared zone: no owner, no label to add.
        let stack = ActionDestination {
            kind: "zone".into(),
            id: "stack".into(),
            owner: String::new(),
            label: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&stack).unwrap(),
            serde_json::json!({ "type": "zone", "id": "stack" })
        );

        // A per-player zone names its owner, and may carry a label.
        let command = ActionDestination {
            kind: "zone".into(),
            id: "command".into(),
            owner: "p1".into(),
            label: "Command zone".into(),
        };
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "zone",
                "id": "command",
                "owner": "p1",
                "label": "Command zone"
            })
        );
        assert_eq!(
            serde_json::from_value::<ActionDestination>(json).unwrap(),
            command
        );

        // An unknown future kind still decodes — a client ignores what it cannot
        // render and therefore offers no drop region for it (fail closed).
        let future: ActionDestination =
            serde_json::from_str(r#"{"type":"quadrant","id":"north"}"#).unwrap();
        assert_eq!(future.kind, "quadrant");
        assert!(future.owner.is_empty());
    }

    #[test]
    fn issue_554_action_ack_round_trips_and_states_its_verdict_explicitly() {
        // `accepted` is always on the wire: the ack itself is the optional part, so a
        // client never has to read an absent field as "rejected".
        let ok = ActionAck {
            submission: "s7".into(),
            accepted: true,
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            serde_json::json!({ "submission": "s7", "accepted": true })
        );
        let no = ActionAck {
            submission: "s8".into(),
            accepted: false,
        };
        let json = serde_json::to_value(&no).unwrap();
        assert_eq!(json["accepted"], serde_json::json!(false));
        assert_eq!(serde_json::from_value::<ActionAck>(json).unwrap(), no);
    }
}
