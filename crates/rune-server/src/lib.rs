//! RUNE server transport — layers 1 (lobby) and 2 (rooms).
//!
//! This crate owns networking, sessions, and timers; it never owns rules — that
//! is [`rune_engine`]. Layer 1 (this module) runs a Tokio runtime, accepts
//! WebSocket client connections, logs their lifecycle, and shuts down gracefully.
//! An accepted connection lands in the [`lobby`] module's **pre-game phase**: it is
//! issued a session token and a [`rune_protocol::LobbyView`], and it creates or
//! joins a room by id with [`rune_protocol::LobbyCommand`]s (ADR 0012). No engine
//! game is constructed and no `GameView` is sent until the ready gate passes
//! (issue #112).
//!
//! Layer 2 is the [`room`] module: one async task per room owns a single engine
//! game, applies chosen actions through the engine, and pushes each connected seat
//! its own personalized [`rune_protocol::GameView`]. Redacting hidden zones and
//! naming entities for the wire is the pure [`view`] shim.
//!
//! The server holds **no game logic**: the lobby only issues sessions and routes
//! create/join, and the room routes an `action_id` back to the engine's own
//! `valid_actions`/`apply_action` rather than deciding legality itself.
//!
//! See `docs/decisions/0008-tokio-websocket-server.md` for the dependency
//! choices behind this crate.

mod format;
mod lobby;
mod room;
mod rules_text;
#[cfg(test)]
mod test_support;
mod view;

pub use lobby::{serve_lobby_connection, Lobby};
pub use room::{serve_connection, Room, RoomHandle, RoomInput, Seat};

use std::future::Future;
use std::net::SocketAddr;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{info, warn};

/// Listen address the server binds to when no override is supplied.
///
/// Overridable via the `--addr <host:port>` CLI flag or the `RUNE_SERVER_ADDR`
/// environment variable (in that order of precedence).
pub const DEFAULT_ADDR: &str = "127.0.0.1:9000";

/// Environment variable read for the listen address.
pub const ADDR_ENV_VAR: &str = "RUNE_SERVER_ADDR";

/// Environment variable read for a fixed engine shuffle seed (see
/// [`Config::rng_seed`]).
pub const RNG_SEED_ENV_VAR: &str = "RUNE_RNG_SEED";

/// Environment variable read for a fixed starting life total (see
/// [`Config::starting_life`]).
pub const STARTING_LIFE_ENV_VAR: &str = "RUNE_STARTING_LIFE";

/// Runtime configuration for the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `host:port` the listener binds to. Resolved by the OS at bind time, so a
    /// hostname (e.g. `localhost:9000`) or an IP literal both work.
    pub addr: String,
    /// A fixed engine shuffle seed to build **every** game from, when set. The
    /// engine's only randomness is this seed (ADR 0014), and per that ADR the seed
    /// is *server-side* state that is never projected to a client — so pinning it
    /// is a server/operator concern, deliberately **not** a client-settable
    /// `RoomConfig`/protocol field (which would let a player predict their own
    /// shuffle). Left `None` for normal play, where each game gets a distinct,
    /// server-generated seed. Its purpose is a fully deterministic, reproducible
    /// game for the end-to-end suite (issue #145): the harness starts the server
    /// with a pinned seed and fixed decklists so a scripted game replays exactly.
    /// Overridable via `--rng-seed <u64>` or [`RNG_SEED_ENV_VAR`].
    pub rng_seed: Option<u64>,
    /// A fixed starting life total to build **every** game from, when set,
    /// overriding the room format's default (ADR 0013 §4). Like [`Config::rng_seed`]
    /// this is a server/operator concern, not a client-settable field: it exists so
    /// the end-to-end suite (issue #145) can run a *short*, deterministic game —
    /// a low life total means only a few combat turns are needed to reach the lethal
    /// `LifeZero` result, keeping the browser-driven game inside the CI budget.
    /// `None` for normal play (each format's own starting life stands). Overridable
    /// via `--starting-life <i32>` or [`STARTING_LIFE_ENV_VAR`].
    pub starting_life: Option<i32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: DEFAULT_ADDR.to_string(),
            rng_seed: None,
            starting_life: None,
        }
    }
}

impl Config {
    /// Build a [`Config`] from process arguments and environment, applying
    /// precedence: `--addr`/`-a` flag > [`ADDR_ENV_VAR`] > [`DEFAULT_ADDR`].
    ///
    /// # Errors
    /// Returns [`ConfigError`] if the address flag is given without a value.
    pub fn from_env_and_args() -> Result<Self, ConfigError> {
        Self::resolve(std::env::args().skip(1), |key| std::env::var(key).ok())
    }

    /// Core of [`Config::from_env_and_args`], with arguments and environment
    /// injected so it can be unit-tested without touching process globals.
    fn resolve<A, E>(args: A, env: E) -> Result<Self, ConfigError>
    where
        A: IntoIterator<Item = String>,
        E: Fn(&str) -> Option<String>,
    {
        let mut addr: Option<String> = None;
        let mut rng_seed: Option<String> = None;
        let mut starting_life: Option<String> = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if let Some(value) = arg.strip_prefix("--addr=") {
                addr = Some(value.to_string());
            } else if arg == "--addr" || arg == "-a" {
                addr = Some(args.next().ok_or(ConfigError::MissingAddrValue)?);
            } else if let Some(value) = arg.strip_prefix("--rng-seed=") {
                rng_seed = Some(value.to_string());
            } else if arg == "--rng-seed" {
                rng_seed = Some(args.next().ok_or(ConfigError::MissingRngSeedValue)?);
            } else if let Some(value) = arg.strip_prefix("--starting-life=") {
                starting_life = Some(value.to_string());
            } else if arg == "--starting-life" {
                starting_life = Some(args.next().ok_or(ConfigError::MissingStartingLifeValue)?);
            }
        }

        let addr = addr
            .or_else(|| env(ADDR_ENV_VAR))
            .unwrap_or_else(|| DEFAULT_ADDR.to_string());
        let rng_seed = match rng_seed.or_else(|| env(RNG_SEED_ENV_VAR)) {
            Some(raw) => Some(
                raw.trim()
                    .parse::<u64>()
                    .map_err(|_| ConfigError::InvalidRngSeed(raw))?,
            ),
            None => None,
        };
        let starting_life = match starting_life.or_else(|| env(STARTING_LIFE_ENV_VAR)) {
            Some(raw) => Some(
                raw.trim()
                    .parse::<i32>()
                    .map_err(|_| ConfigError::InvalidStartingLife(raw))?,
            ),
            None => None,
        };
        Ok(Self {
            addr,
            rng_seed,
            starting_life,
        })
    }
}

/// Error building a [`Config`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// `--addr`/`-a` was supplied without a following value.
    MissingAddrValue,
    /// `--rng-seed` was supplied without a following value.
    MissingRngSeedValue,
    /// `--rng-seed`/[`RNG_SEED_ENV_VAR`] held a value that is not a `u64`. Carries
    /// the offending raw value.
    InvalidRngSeed(String),
    /// `--starting-life` was supplied without a following value.
    MissingStartingLifeValue,
    /// `--starting-life`/[`STARTING_LIFE_ENV_VAR`] held a value that is not an
    /// `i32`. Carries the offending raw value.
    InvalidStartingLife(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAddrValue => {
                write!(f, "--addr requires a value, e.g. --addr {DEFAULT_ADDR}")
            }
            Self::MissingRngSeedValue => {
                write!(f, "--rng-seed requires a value, e.g. --rng-seed 42")
            }
            Self::InvalidRngSeed(raw) => {
                write!(f, "--rng-seed must be a u64, got {raw:?}")
            }
            Self::MissingStartingLifeValue => {
                write!(
                    f,
                    "--starting-life requires a value, e.g. --starting-life 20"
                )
            }
            Self::InvalidStartingLife(raw) => {
                write!(f, "--starting-life must be an i32, got {raw:?}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// A bound-but-not-yet-serving server. Split from [`Server::run`] so callers
/// (and tests) can learn the actual [`Server::local_addr`] — useful when binding
/// to port `0` — before the accept loop starts.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    local_addr: SocketAddr,
}

impl Server {
    /// Bind a TCP listener for the configured address.
    ///
    /// # Errors
    /// Returns the underlying [`std::io::Error`] if the address cannot be
    /// resolved or the socket cannot be bound (e.g. the port is in use).
    pub async fn bind(config: &Config) -> std::io::Result<Self> {
        let listener = TcpListener::bind(config.addr.as_str()).await?;
        let local_addr = listener.local_addr()?;
        Ok(Self {
            listener,
            local_addr,
        })
    }

    /// The address the listener is actually bound to (resolved, and with any
    /// ephemeral port filled in).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Run the accept loop until `shutdown` resolves, then close every live
    /// connection cleanly and wait for its task to finish.
    ///
    /// Each accepted connection is handled on its own Tokio task (the
    /// task-per-client model from `docs/brief.md`). Once its WebSocket handshake
    /// succeeds, the connection enters the `lobby`'s pre-game phase — issued a
    /// session and a `LobbyView`, creating or joining rooms by id — until the ready
    /// gate constructs a game (issue #112). A failed handshake or accept is logged
    /// and skipped; it never takes the listener down.
    ///
    /// # Errors
    /// Currently infallible in practice, but returns [`std::io::Result`] so the
    /// signature is stable as the loop grows.
    pub async fn run<F>(self, lobby: Lobby, shutdown: F) -> std::io::Result<()>
    where
        F: Future<Output = ()>,
    {
        // Broadcast channel telling live connections to close on shutdown.
        let (close_tx, _close_rx) = watch::channel(false);
        let mut connections = JoinSet::new();

        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let close_rx = close_tx.subscribe();
                        connections.spawn(handle_connection(stream, peer, lobby.clone(), close_rx));
                    }
                    Err(error) => {
                        // Transient accept errors (e.g. fd exhaustion) must not
                        // kill the listener.
                        warn!(%error, "failed to accept connection");
                    }
                },
                () = &mut shutdown => {
                    info!("shutdown requested; no longer accepting connections");
                    break;
                }
            }
        }

        // Ask live connections to close, then drain their tasks.
        let _ = close_tx.send(true);
        while let Some(joined) = connections.join_next().await {
            if let Err(error) = joined {
                warn!(%error, "connection task did not exit cleanly");
            }
        }
        info!("all connections closed; server stopped");
        Ok(())
    }
}

/// Handle a single client: complete the WebSocket handshake, then drive it through
/// the `lobby`'s pre-game phase until the peer closes or shutdown is requested.
///
/// This is layer 1's connective tissue: it registers a session and routes lobby
/// commands, holding **no game logic** of its own. No engine game is constructed and
/// no `GameView` is sent here — that begins at the ready gate (issue #112). On exit
/// the session is disconnected, vacating any seat it held.
async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    lobby: Lobby,
    close_rx: watch::Receiver<bool>,
) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(error) => {
            warn!(%peer, %error, "websocket handshake failed");
            return;
        }
    };
    info!(%peer, "client connected");

    // Drive the connection through the lobby's pre-game phase, ending it politely
    // when the server signals shutdown over the watch channel.
    serve_lobby_connection(lobby, ws, wait_for_shutdown(close_rx)).await;

    info!(%peer, "client disconnected");
}

/// Resolve once the server has signalled shutdown on the watch channel (or the
/// sender was dropped, which also means the server is stopping).
async fn wait_for_shutdown(mut close_rx: watch::Receiver<bool>) {
    while close_rx.changed().await.is_ok() {
        if *close_rx.borrow() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn default_config_uses_documented_default() {
        assert_eq!(Config::default().addr, DEFAULT_ADDR);
        assert_eq!(DEFAULT_ADDR, "127.0.0.1:9000");
        // Normal play sources its own per-game entropy; no seed is pinned.
        assert_eq!(Config::default().rng_seed, None);
    }

    #[test]
    fn flag_overrides_env_and_default() {
        let cfg = Config::resolve(["--addr".to_string(), "0.0.0.0:1234".to_string()], |k| {
            (k == ADDR_ENV_VAR).then(|| "10.0.0.1:9999".to_string())
        })
        .unwrap();
        assert_eq!(cfg.addr, "0.0.0.0:1234");
    }

    #[test]
    fn flag_accepts_equals_form() {
        let cfg = Config::resolve(["--addr=0.0.0.0:1234".to_string()], |_| None).unwrap();
        assert_eq!(cfg.addr, "0.0.0.0:1234");
    }

    #[test]
    fn env_used_when_no_flag() {
        let cfg = Config::resolve(Vec::<String>::new(), |k| {
            (k == ADDR_ENV_VAR).then(|| "127.0.0.1:5555".to_string())
        })
        .unwrap();
        assert_eq!(cfg.addr, "127.0.0.1:5555");
    }

    #[test]
    fn default_when_nothing_supplied() {
        let cfg = Config::resolve(Vec::<String>::new(), |_| None).unwrap();
        assert_eq!(cfg.addr, DEFAULT_ADDR);
    }

    #[test]
    fn missing_flag_value_is_an_error() {
        let err = Config::resolve(["--addr".to_string()], |_| None).unwrap_err();
        assert_eq!(err, ConfigError::MissingAddrValue);
    }

    #[test]
    fn rng_seed_defaults_to_none() {
        let cfg = Config::resolve(Vec::<String>::new(), |_| None).unwrap();
        assert_eq!(cfg.rng_seed, None);
    }

    #[test]
    fn rng_seed_from_flag_and_equals_and_env() {
        let flag = Config::resolve(["--rng-seed".to_string(), "42".to_string()], |_| None).unwrap();
        assert_eq!(flag.rng_seed, Some(42));

        let eq = Config::resolve(["--rng-seed=7".to_string()], |_| None).unwrap();
        assert_eq!(eq.rng_seed, Some(7));

        let from_env = Config::resolve(Vec::<String>::new(), |k| {
            (k == RNG_SEED_ENV_VAR).then(|| "123".to_string())
        })
        .unwrap();
        assert_eq!(from_env.rng_seed, Some(123));

        // The flag wins over the environment, mirroring `--addr` precedence.
        let flag_over_env =
            Config::resolve(["--rng-seed=1".to_string()], |_| Some("999".to_string())).unwrap();
        assert_eq!(flag_over_env.rng_seed, Some(1));
    }

    #[test]
    fn rng_seed_rejects_missing_or_non_numeric() {
        let missing = Config::resolve(["--rng-seed".to_string()], |_| None).unwrap_err();
        assert_eq!(missing, ConfigError::MissingRngSeedValue);

        let bad = Config::resolve(["--rng-seed=banana".to_string()], |_| None).unwrap_err();
        assert_eq!(bad, ConfigError::InvalidRngSeed("banana".to_string()));
    }

    #[test]
    fn starting_life_from_flag_env_and_rejects_bad_values() {
        assert_eq!(
            Config::resolve(Vec::<String>::new(), |_| None)
                .unwrap()
                .starting_life,
            None
        );

        let flag =
            Config::resolve(["--starting-life".to_string(), "5".to_string()], |_| None).unwrap();
        assert_eq!(flag.starting_life, Some(5));

        let eq = Config::resolve(["--starting-life=8".to_string()], |_| None).unwrap();
        assert_eq!(eq.starting_life, Some(8));

        let from_env = Config::resolve(Vec::<String>::new(), |k| {
            (k == STARTING_LIFE_ENV_VAR).then(|| "3".to_string())
        })
        .unwrap();
        assert_eq!(from_env.starting_life, Some(3));

        let missing = Config::resolve(["--starting-life".to_string()], |_| None).unwrap_err();
        assert_eq!(missing, ConfigError::MissingStartingLifeValue);

        let bad = Config::resolve(["--starting-life=lots".to_string()], |_| None).unwrap_err();
        assert_eq!(bad, ConfigError::InvalidStartingLife("lots".to_string()));
    }
}
