//! The client seam: what a frame renders as, and what one line of input selects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use sage_protocol::{Phase, ValidAction};

fn view_with_actions(actions: Vec<ValidAction>) -> GameView {
    // `GameView` derives `Default` (issue #553), so a harness view names only the
    // fields it exercises and never has to restate the additive rest.
    GameView {
        you: "p0".into(),
        phase: Phase::PrecombatMain,
        turn: 1,
        active_player: "p0".into(),
        priority_player: Some("p0".into()),
        valid_actions: actions,
        ..Default::default()
    }
}

fn pass() -> ValidAction {
    ValidAction {
        id: "a0".into(),
        kind: "pass_priority".into(),
        label: "Pass priority".into(),
        subject: vec![],
        ..Default::default()
    }
}

fn play_land() -> ValidAction {
    ValidAction {
        id: "a1".into(),
        kind: "play_land".into(),
        label: "Play Forest".into(),
        subject: vec!["card_5".into()],
        ..Default::default()
    }
}

#[test]
fn ws_url_adds_scheme_for_bare_host_port() {
    let config = CliConfig {
        addr: "127.0.0.1:9000".into(),
    };
    assert_eq!(config.ws_url(), "ws://127.0.0.1:9000");
}

#[test]
fn ws_url_preserves_an_explicit_scheme() {
    let config = CliConfig {
        addr: "wss://example.test/game".into(),
    };
    assert_eq!(config.ws_url(), "wss://example.test/game");
}

#[test]
fn config_precedence_flag_over_env_over_default() {
    let flag = CliConfig::resolve(["--addr".to_string(), "host:1".to_string()], |_| {
        Some("host:2".to_string())
    })
    .unwrap();
    assert_eq!(flag.addr, "host:1");

    let env = CliConfig::resolve(Vec::<String>::new(), |k| {
        (k == ADDR_ENV_VAR).then(|| "host:2".to_string())
    })
    .unwrap();
    assert_eq!(env.addr, "host:2");

    let default = CliConfig::resolve(Vec::<String>::new(), |_| None).unwrap();
    assert_eq!(default.addr, DEFAULT_ADDR);
}

#[test]
fn config_flag_without_value_is_an_error() {
    let err = CliConfig::resolve(["--addr".to_string()], |_| None).unwrap_err();
    assert_eq!(err, ConfigError::MissingAddrValue);
}

#[test]
fn select_action_maps_one_based_menu_to_offered_ids() {
    let view = view_with_actions(vec![pass(), play_land()]);
    assert_eq!(select_action(&view, "1"), Some("a0"));
    assert_eq!(select_action(&view, "2"), Some("a1"));
    // Whitespace around the number is tolerated.
    assert_eq!(select_action(&view, "  2\n"), Some("a1"));
}

#[test]
fn select_action_rejects_invalid_choices() {
    let view = view_with_actions(vec![pass(), play_land()]);
    // Zero, out of range, non-numeric, and empty all fail — caller re-prompts.
    assert_eq!(select_action(&view, "0"), None);
    assert_eq!(select_action(&view, "3"), None);
    assert_eq!(select_action(&view, "banana"), None);
    assert_eq!(select_action(&view, ""), None);
    assert_eq!(select_action(&view, "-1"), None);
}

#[test]
fn render_numbers_actions_and_shows_labels() {
    let view = view_with_actions(vec![pass(), play_land()]);
    let text = render(&view);
    assert!(text.contains("1) Pass priority"));
    assert!(text.contains("2) Play Forest"));
    assert!(text.contains("Priority: p0"));
}

#[test]
fn render_reports_when_no_actions_are_available() {
    let view = view_with_actions(vec![]);
    let text = render(&view);
    assert!(text.contains("No actions available"));
    assert!(!text.contains("Actions:"));
}

#[test]
fn issue_255_render_shows_the_receivers_own_life_and_library() {
    // The terminal client shows the player their own life and library size, the
    // same public numbers it already prints for each opponent.
    let mut view = view_with_actions(vec![]);
    view.me = sage_protocol::SelfView {
        life: 15,
        library_size: 30,
        ..Default::default()
    };
    let text = render(&view);
    assert!(
        text.contains("You (p0): life 15, library 30"),
        "own stats missing from:\n{text}"
    );
}
