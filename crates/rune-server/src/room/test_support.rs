//! Shared `#[cfg(test)]` harness for the room submodules (issue #427): the outbox
//! channel helpers, view-awaiting drivers, and the game-state fixtures every
//! submodule's `#[cfg(test)]` block builds on. `pub(crate)` so each sibling test
//! module can import it; compiled only under `cfg(test)`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_engine::Step;
use rune_protocol::{ChooseAction, ClientMessage};

use super::*;
use crate::test_support::fixture;

pub(crate) fn db() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// A fresh per-seat outbox pair mirroring what a connection hands the room.
pub(crate) fn view_channel() -> (
    watch::Sender<Option<GameView>>,
    watch::Receiver<Option<GameView>>,
) {
    watch::channel(None)
}

/// Receive the next (latest) view, awaiting the room task rather than
/// busy-polling. Marks the value seen so a later [`watch::Receiver::has_changed`]
/// reflects only views pushed after this call.
pub(crate) async fn wait_for_view(rx: &mut watch::Receiver<Option<GameView>>) -> GameView {
    rx.changed().await.expect("room should push a view");
    rx.borrow_and_update()
        .clone()
        .expect("pushed view is never the initial empty slot")
}

/// Receive the next (latest) spectator view, awaiting the room task.
pub(crate) async fn wait_for_spectator_view(
    rx: &mut watch::Receiver<Option<SpectatorView>>,
) -> SpectatorView {
    rx.changed()
        .await
        .expect("room should push a spectator view");
    rx.borrow_and_update().clone().expect("a pushed view")
}

/// A two-player game in the precombat main phase where player 0 holds a Forest
/// and a creature and player 1 holds a single card. Enough to exercise
/// hidden-zone redaction and a real (non-pass) action.
pub(crate) fn dealt_state() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let p0_hand = vec![
        state.new_instance(fixture("forest")),
        state.new_instance(fixture("walking_corpse")),
    ];
    let p0_lib = vec![state.new_instance(fixture("onakke_ogre"))];
    let p1_hand = vec![state.new_instance(fixture("onakke_ogre"))];
    let p1_lib = vec![
        state.new_instance(fixture("onakke_ogre")),
        state.new_instance(fixture("onakke_ogre")),
    ];
    state.players[0].hand = p0_hand;
    state.players[0].library = p0_lib;
    state.players[1].hand = p1_hand;
    state.players[1].library = p1_lib;
    state
}

/// A two-player game whose player 1 sits at 0 life. `apply_action` always runs
/// state-based actions, so the next applied action (even a pass) marks player 1
/// as having lost — driving the room to a terminal state.
pub(crate) fn near_terminal_state() -> GameState {
    let mut state = GameState::new_two_player();
    state.players[1].life = 0;
    state
}

/// A [`dealt_state`] whose player 1 sits at 0 life, so the very next applied
/// action (a timeout's default pass) runs state-based actions that end the game —
/// giving the auto-advancing test clock a terminal state to stop at.
pub(crate) fn near_terminal_dealt_state() -> GameState {
    let mut state = dealt_state();
    state.players[1].life = 0;
    state
}

/// A two-player game where neither seat can ever take a meaningful action: empty
/// hands and boards, and libraries of uncastable creatures (drawn cards can never
/// be cast — no lands, no mana — so a seat stays idle every turn without decking).
/// Starts at seat 0's upkeep so a full turn's worth of priority windows is ahead.
pub(crate) fn spell_less_state() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::Upkeep;
    for seat in 0..2 {
        let lib: Vec<_> = (0..12)
            .map(|_| state.new_instance(fixture("onakke_ogre")))
            .collect();
        state.players[seat].library = lib;
    }
    state
}

/// Choose this seat's move from a view: pass if offered, else the sole forced
/// choice (an empty combat declaration), with no targets. A minimal inline driver
/// (no rule-based agent, to avoid a crate cycle) that stands in for a human's clicks.
pub(crate) fn forced_move(view: &GameView) -> ChooseAction {
    let action = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "pass_priority")
        .or_else(|| view.valid_actions.iter().find(|a| a.kind != "concede"))
        .expect("an actionable view offers a move");
    ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: Vec::new(),
    }
}

/// Drive the room with [`forced_move`] until either seat's latest view reaches
/// `until_turn`, returning how many messages the driver had to send — a proxy for
/// how many manual clicks the turn cost.
pub(crate) async fn count_clicks_until_turn(
    handle: &RoomHandle,
    rx0: &mut watch::Receiver<Option<GameView>>,
    rx1: &mut watch::Receiver<Option<GameView>>,
    until_turn: u32,
) -> usize {
    let mut clicks = 0usize;
    for _ in 0..1000usize {
        let v0 = rx0.borrow_and_update().clone();
        let v1 = rx1.borrow_and_update().clone();
        if v0
            .as_ref()
            .or(v1.as_ref())
            .is_some_and(|v| v.turn >= until_turn)
        {
            return clicks;
        }
        let actor = if v0.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
            v0.map(|v| (0usize, v))
        } else if v1.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
            v1.map(|v| (1usize, v))
        } else {
            None
        };
        match actor {
            Some((seat, view)) => {
                clicks += 1;
                handle.send(RoomInput::Message {
                    seat,
                    message: ClientMessage::ChooseAction(forced_move(&view)),
                });
                tokio::select! {
                    _ = rx0.changed() => {}
                    _ = rx1.changed() => {}
                }
            }
            None => {
                tokio::select! {
                    r0 = rx0.changed() => { if r0.is_err() { break; } }
                    r1 = rx1.changed() => { if r1.is_err() { break; } }
                }
            }
        }
    }
    clicks
}
