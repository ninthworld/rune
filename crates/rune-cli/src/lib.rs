//! RUNE terminal client library — the human-driven session loop, kept separate
//! from the [binary](../main.rs) so it can be unit-tested and driven over any
//! transport.
//!
//! The client is a **dumb renderer** (`AGENTS.md` hard rule): it prints the
//! [`GameView`] the server sent, offers its `valid_actions` as a numbered menu,
//! reads a number from the operator, and echoes back the matching `action_id` in a
//! [`ClientMessage::ChooseAction`]. It computes no legality, cost, or effect, and
//! carries no state across messages — every frame rebuilds the whole display from
//! scratch, exactly as reconnect/resync require (`docs/protocol.md`).
//!
//! The wire protocol is the server's: it consumes the personalized [`GameView`]
//! frames the room task pushes and replies with the same `choose_action` message
//! shape the room accepts (see `rune-server`'s `room.rs`/`view.rs`).

use futures_util::{SinkExt, StreamExt};
use rune_protocol::{CardView, ChooseAction, ClientMessage, GameView};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

mod agent;

pub use agent::{
    is_offered, request_payload, run_agent_session, safe_default, Agent, AgentConfig, AgentError,
    PassPriorityAgent, AGENT_TIMEOUT_ENV_VAR, DEFAULT_AGENT_DEADLINE,
};

/// Address the CLI connects to when nothing overrides it. Matches the server's
/// own default listen address (`rune_server::DEFAULT_ADDR`).
pub const DEFAULT_ADDR: &str = "127.0.0.1:9000";

/// Environment variable read for the server address. Shared with the server so a
/// single `RUNE_SERVER_ADDR` points both halves at the same endpoint.
pub const ADDR_ENV_VAR: &str = "RUNE_SERVER_ADDR";

/// Runtime configuration for the CLI client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliConfig {
    /// The server endpoint. Either a `host:port` (turned into a `ws://` URL) or a
    /// full `ws://`/`wss://` URL, in which case it is used verbatim.
    pub addr: String,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            addr: DEFAULT_ADDR.to_string(),
        }
    }
}

impl CliConfig {
    /// Build a [`CliConfig`] from process arguments and environment, applying
    /// precedence: `--addr`/`-a` flag > [`ADDR_ENV_VAR`] > [`DEFAULT_ADDR`].
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the address flag is supplied without a value.
    pub fn from_env_and_args() -> Result<Self, ConfigError> {
        Self::resolve(std::env::args().skip(1), |key| std::env::var(key).ok())
    }

    /// Core of [`CliConfig::from_env_and_args`], with arguments and environment
    /// injected so it can be unit-tested without touching process globals.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if `--addr`/`-a` is given without a following value.
    pub fn resolve<A, E>(args: A, env: E) -> Result<Self, ConfigError>
    where
        A: IntoIterator<Item = String>,
        E: Fn(&str) -> Option<String>,
    {
        let mut addr: Option<String> = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if let Some(value) = arg.strip_prefix("--addr=") {
                addr = Some(value.to_string());
            } else if arg == "--addr" || arg == "-a" {
                addr = Some(args.next().ok_or(ConfigError::MissingAddrValue)?);
            }
        }

        let addr = addr
            .or_else(|| env(ADDR_ENV_VAR))
            .unwrap_or_else(|| DEFAULT_ADDR.to_string());
        Ok(Self { addr })
    }

    /// The WebSocket URL to dial. A bare `host:port` becomes `ws://host:port`; an
    /// address already carrying a `ws://`/`wss://` scheme is returned unchanged.
    #[must_use]
    pub fn ws_url(&self) -> String {
        if self.addr.starts_with("ws://") || self.addr.starts_with("wss://") {
            self.addr.clone()
        } else {
            format!("ws://{}", self.addr)
        }
    }
}

/// Error building a [`CliConfig`] or [`AgentConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// `--addr`/`-a` was supplied without a following value.
    MissingAddrValue,
    /// `--agent-timeout` was supplied without a following value.
    MissingAgentTimeoutValue,
    /// `--agent-timeout` (or [`AGENT_TIMEOUT_ENV_VAR`]) was not a positive,
    /// finite number of seconds. Carries the offending value.
    InvalidAgentTimeout(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAddrValue => {
                write!(f, "--addr requires a value, e.g. --addr {DEFAULT_ADDR}")
            }
            Self::MissingAgentTimeoutValue => {
                write!(
                    f,
                    "--agent-timeout requires a value in seconds, e.g. --agent-timeout 5"
                )
            }
            Self::InvalidAgentTimeout(value) => {
                write!(
                    f,
                    "--agent-timeout must be a positive number of seconds, got {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Something that ended a client [`run_session`] loop abnormally.
#[derive(Debug)]
pub enum SessionError {
    /// The WebSocket transport failed (connect, read, or write).
    WebSocket(tokio_tungstenite::tungstenite::Error),
    /// A stdin/stdout I/O error occurred.
    Io(std::io::Error),
    /// A chosen action could not be serialized to the wire message.
    Encode(serde_json::Error),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WebSocket(error) => write!(f, "websocket error: {error}"),
            Self::Io(error) => write!(f, "i/o error: {error}"),
            Self::Encode(error) => write!(f, "failed to encode action: {error}"),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WebSocket(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Encode(error) => Some(error),
        }
    }
}

/// Dial the server named by `config` and complete the WebSocket handshake.
///
/// # Errors
/// Returns [`SessionError::WebSocket`] if the connection or handshake fails.
pub async fn connect(
    config: &CliConfig,
) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, SessionError>
{
    let url = config.ws_url();
    let (ws, _response) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(SessionError::WebSocket)?;
    Ok(ws)
}

/// Run the interactive session to completion over an already-connected socket.
///
/// The loop is: receive one [`GameView`], render it (summary + numbered menu of
/// `valid_actions`), and — only when the view offers actions — prompt for a menu
/// number and send the matching `action_id` as a [`ClientMessage::ChooseAction`].
/// A view with no actions is displayed and the loop simply waits for the next one.
///
/// `input` is the operator's line source (stdin in the binary) and `output` is
/// where the rendered display and prompts are written (stdout). Both are injected
/// so the loop can be driven by a test fixture.
///
/// The loop exits cleanly — returning `Ok(())` — when the server closes the
/// connection or when `input` reaches EOF; it never panics on either. It returns
/// an error only if the transport or a local write fails mid-session.
///
/// # Errors
/// Returns a [`SessionError`] if a WebSocket read/write, a stdout write, or the
/// encoding of a chosen action fails.
pub async fn run_session<S, R, W>(
    ws: WebSocketStream<S>,
    mut input: R,
    mut output: W,
) -> Result<(), SessionError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    let mut line = String::new();

    'session: loop {
        // 1. Receive the next personalized view. The entire display is rebuilt
        //    from this single message — nothing is carried across frames.
        let view = loop {
            match read.next().await {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<GameView>(text.as_str()) {
                        Ok(view) => break view,
                        Err(error) => {
                            let note = format!("! ignoring undecodable server message: {error}\n");
                            write_str(&mut output, &note).await?;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    write_str(&mut output, "\nServer closed the connection. Goodbye.\n").await?;
                    break 'session;
                }
                // Ping/pong/binary/raw frames carry no protocol message; ignore.
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(SessionError::WebSocket(error)),
            }
        };

        write_str(&mut output, &render(&view)).await?;

        // 2. No actions offered (we do not hold priority): await the next view.
        if view.valid_actions.is_empty() {
            continue;
        }

        // 3. Prompt until a valid menu number is entered, or stdin hits EOF.
        let action_id = loop {
            write_str(&mut output, &prompt(view.valid_actions.len())).await?;
            output.flush().await.map_err(SessionError::Io)?;

            line.clear();
            let read_bytes = input.read_line(&mut line).await.map_err(SessionError::Io)?;
            if read_bytes == 0 {
                // EOF on stdin: leave cleanly, telling the server we are done.
                write_str(&mut output, "\nEnd of input. Goodbye.\n").await?;
                let _ = write.send(Message::Close(None)).await;
                return Ok(());
            }

            match select_action(&view, &line) {
                Some(id) => break id.to_string(),
                None => {
                    let note = format!(
                        "  '{}' is not a listed choice — enter a number from the menu.\n",
                        line.trim()
                    );
                    write_str(&mut output, &note).await?;
                }
            }
        };

        // 4. Echo the chosen action id back; the server decides what happens next.
        let choose = ClientMessage::ChooseAction(ChooseAction { action_id });
        let json = serde_json::to_string(&choose).map_err(SessionError::Encode)?;
        write
            .send(Message::Text(json))
            .await
            .map_err(SessionError::WebSocket)?;
    }

    let _ = write.send(Message::Close(None)).await;
    Ok(())
}

/// Map an operator's raw menu entry to the offered `action_id`, or `None` if it
/// is not a number naming a listed action.
///
/// The menu is 1-based: `"1"` selects `valid_actions[0]`. Anything that is not a
/// positive integer within range (blank, non-numeric, `0`, out-of-range) returns
/// `None`, so the caller can re-prompt. This performs **no** game logic — it only
/// indexes into the actions the server already offered.
#[must_use]
pub fn select_action<'a>(view: &'a GameView, input: &str) -> Option<&'a str> {
    let choice: usize = input.trim().parse().ok()?;
    let index = choice.checked_sub(1)?;
    view.valid_actions
        .get(index)
        .map(|action| action.id.as_str())
}

/// Render the whole display for one [`GameView`]: a plain-text summary of the
/// public and owned state followed by the numbered `valid_actions` menu.
///
/// This is a pure projection of the view — it shows only what the server sent and
/// derives nothing. The output is deterministic for a given view, which is what
/// lets a fresh frame fully reconstruct the display.
#[must_use]
pub fn render(view: &GameView) -> String {
    let mut out = String::new();
    out.push_str("\n========================================\n");
    out.push_str(&format!("Phase: {:?}\n", view.phase));
    match &view.priority_player {
        Some(player) => out.push_str(&format!("Priority: {player}\n")),
        None => out.push_str("Priority: (none)\n"),
    }
    if !view.mana_pool.is_empty() {
        out.push_str(&format!("Mana pool: {}\n", view.mana_pool.join(" ")));
    }

    out.push_str(&format!("Your hand ({}):\n", view.my_hand.len()));
    if view.my_hand.is_empty() {
        out.push_str("  (empty)\n");
    } else {
        for card in &view.my_hand {
            out.push_str(&format!("  - {}\n", card_line(card)));
        }
    }

    for opponent in &view.opponents {
        out.push_str(&format!(
            "Opponent {}: life {}, hand {}, library {}, graveyard {}\n",
            opponent.player_id,
            opponent.life,
            opponent.hand_size,
            opponent.library_size,
            opponent.graveyard_size,
        ));
    }

    if !view.battlefield.is_empty() {
        out.push_str("Battlefield:\n");
        for perm in &view.battlefield {
            let tapped = if perm.tapped { " (tapped)" } else { "" };
            out.push_str(&format!(
                "  - {} [{}]{}\n",
                perm.card.name, perm.controller, tapped
            ));
        }
    }

    if !view.stack.is_empty() {
        out.push_str("Stack (top last):\n");
        for item in &view.stack {
            out.push_str(&format!("  - {} [{}]\n", item.description, item.controller));
        }
    }

    for pile in &view.graveyards {
        out.push_str(&format!(
            "Graveyard {}: {} card(s)\n",
            pile.player_id,
            pile.cards.len()
        ));
    }
    for pile in &view.exile {
        out.push_str(&format!(
            "Exile {}: {} card(s)\n",
            pile.player_id,
            pile.cards.len()
        ));
    }

    if view.valid_actions.is_empty() {
        out.push_str("\nNo actions available — waiting for the other player...\n");
    } else {
        out.push_str("\nActions:\n");
        for (index, action) in view.valid_actions.iter().enumerate() {
            out.push_str(&format!("  {}) {}\n", index + 1, action.label));
        }
    }

    out
}

/// One line describing a card the viewer may see: name, cost, type, and P/T.
fn card_line(card: &CardView) -> String {
    let mut line = card.name.clone();
    if let Some(cost) = &card.mana_cost {
        line.push(' ');
        line.push_str(cost);
    }
    if !card.type_line.is_empty() {
        line.push_str(" — ");
        line.push_str(&card.type_line);
    }
    if let (Some(power), Some(toughness)) = (&card.power, &card.toughness) {
        line.push_str(&format!(" ({power}/{toughness})"));
    }
    line
}

/// The prompt shown before reading a menu choice.
fn prompt(count: usize) -> String {
    format!("Choose an action [1-{count}] (Ctrl-D to quit): ")
}

/// Write a whole string to `output`, mapping any I/O failure to [`SessionError`].
async fn write_str<W: AsyncWrite + Unpin>(output: &mut W, text: &str) -> Result<(), SessionError> {
    output
        .write_all(text.as_bytes())
        .await
        .map_err(SessionError::Io)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_protocol::{Phase, ValidAction};

    fn view_with_actions(actions: Vec<ValidAction>) -> GameView {
        GameView {
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::PrecombatMain,
            mana_pool: vec![],
            priority_player: Some("p0".into()),
            valid_actions: actions,
            action_deadline: None,
        }
    }

    fn pass() -> ValidAction {
        ValidAction {
            id: "a0".into(),
            kind: "pass_priority".into(),
            label: "Pass priority".into(),
            subject: vec![],
        }
    }

    fn play_land() -> ValidAction {
        ValidAction {
            id: "a1".into(),
            kind: "play_land".into(),
            label: "Play Forest".into(),
            subject: vec!["card_5".into()],
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
}
