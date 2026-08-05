//! SAGE terminal client library — the human-driven session loop, kept separate
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
//! shape the room accepts (see `sage-server`'s `room.rs`/`view.rs`).

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use sage_protocol::{
    CardView, ChooseAction, ClientMessage, GameView, Prompt, PromptOption, TargetChoice,
    TargetRequirement, ValidAction,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

mod agent;
mod lobby;
mod render;
mod session;

pub use agent::{
    choose_action, fill_answers, is_offered, request_payload, run_agent_lobby_session,
    run_agent_session, safe_default, Agent, AgentConfig, AgentError, PassPriorityAgent,
    RuleBasedAgent, AGENT_TIMEOUT_ENV_VAR, DEFAULT_AGENT_DEADLINE,
};
pub use lobby::{render_lobby, LobbyConfig, RoomAction};
pub use render::*;
pub use session::*;

/// The write half of a split WebSocket, shared by the game and lobby loops so the
/// lobby phase can hand the same socket to the game phase without reconnecting.
pub(crate) type WsWrite<S> = SplitSink<WebSocketStream<S>, Message>;
/// The read half of a split WebSocket (see [`WsWrite`]).
pub(crate) type WsRead<S> = SplitStream<WebSocketStream<S>>;

/// Address the CLI connects to when nothing overrides it. Matches the server's
/// own default listen address (`sage_server::DEFAULT_ADDR`).
pub const DEFAULT_ADDR: &str = "127.0.0.1:9000";

/// Environment variable read for the server address. Shared with the server so a
/// single `SAGE_SERVER_ADDR` points both halves at the same endpoint.
pub const ADDR_ENV_VAR: &str = "SAGE_SERVER_ADDR";

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
    /// `--room` was supplied without a following room id.
    MissingRoomValue,
    /// `--seats` was supplied without a following value.
    MissingSeatsValue,
    /// `--seats` was not a valid seat count. Carries the offending value.
    InvalidSeats(String),
    /// `--game-setup` was supplied without a following value.
    MissingGameSetupValue,
    /// `--deck` was supplied without a following value.
    MissingDeckValue,
    /// Both `--create` and `--room` were supplied; a connection either creates a
    /// room or joins one, never both.
    ConflictingRoomAction,
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
            Self::MissingRoomValue => write!(f, "--room requires a room id, e.g. --room r0"),
            Self::MissingSeatsValue => {
                write!(f, "--seats requires a value in 2..=8, e.g. --seats 2")
            }
            Self::InvalidSeats(value) => {
                write!(f, "--seats must be a whole number of seats, got {value:?}")
            }
            Self::MissingGameSetupValue => {
                write!(
                    f,
                    "--game-setup requires a value, e.g. --game-setup standard_2p"
                )
            }
            Self::MissingDeckValue => write!(
                f,
                "--deck requires a comma-separated list of card identities, e.g. --deck 1,1,2,2"
            ),
            Self::ConflictingRoomAction => {
                write!(
                    f,
                    "--create and --room are mutually exclusive: create a room or join one"
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

#[cfg(test)]
mod tests;
