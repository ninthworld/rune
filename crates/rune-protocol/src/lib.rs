//! RUNE protocol — the entire client/server contract.
//!
//! Two message types (docs/protocol.md):
//! - Server -> client: a personalized `GameView`
//! - Client -> server: `ChooseAction { action_id }`
//!
//! Any change to these shapes must update docs/protocol.md in the same PR.

/// One entry of `GameView::valid_actions`. The client renders these; it never
/// invents its own. `subject` names the entities this action belongs to so the
/// client can put the action ON the card rather than in a global bar
/// (docs/decisions/0004-subject-owned-actions.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidAction {
    pub id: String,
    pub label: String,
    pub subject: Vec<String>,
}

/// The only message a client ever sends about the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChooseAction {
    pub action_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_action_is_just_an_id() {
        let msg = ChooseAction { action_id: "a2".into() };
        assert_eq!(msg.action_id, "a2");
    }
}
