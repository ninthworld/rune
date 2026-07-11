//! RUNE server transport — layer 1 (lobby) accept loop.
//!
//! This crate owns networking, sessions, and timers; it never owns rules — that
//! is [`rune_engine`]. This module is the layer-1 skeleton: it runs a Tokio
//! runtime, accepts WebSocket client connections, logs their lifecycle, and
//! shuts down gracefully. It contains **no game logic** — the per-connection
//! handler here only echoes frames to prove the transport. The rooms layer
//! (layer 2) that drives the engine and pushes `GameView`s lands in a later
//! milestone (see `docs/agents/backlog.md` #11).
//!
//! See `docs/decisions/0008-tokio-websocket-server.md` for the dependency
//! choices behind this module.

use std::future::Future;
use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

/// Listen address the server binds to when no override is supplied.
///
/// Overridable via the `--addr <host:port>` CLI flag or the `RUNE_SERVER_ADDR`
/// environment variable (in that order of precedence).
pub const DEFAULT_ADDR: &str = "127.0.0.1:9000";

/// Environment variable read for the listen address.
pub const ADDR_ENV_VAR: &str = "RUNE_SERVER_ADDR";

/// Runtime configuration for the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `host:port` the listener binds to. Resolved by the OS at bind time, so a
    /// hostname (e.g. `localhost:9000`) or an IP literal both work.
    pub addr: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: DEFAULT_ADDR.to_string(),
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
}

/// Error building a [`Config`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// `--addr`/`-a` was supplied without a following value.
    MissingAddrValue,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAddrValue => {
                write!(f, "--addr requires a value, e.g. --addr {DEFAULT_ADDR}")
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
    /// task-per-client model from `docs/brief.md`). A failed WebSocket handshake
    /// or accept is logged and skipped; it never takes the listener down.
    ///
    /// # Errors
    /// Currently infallible in practice, but returns [`std::io::Result`] so the
    /// signature is stable as the loop grows.
    pub async fn run<F>(self, shutdown: F) -> std::io::Result<()>
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
                        connections.spawn(handle_connection(stream, peer, close_rx));
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

/// Handle a single client: complete the WebSocket handshake, then echo frames
/// until the peer closes, an error occurs, or shutdown is requested.
async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    mut close_rx: watch::Receiver<bool>,
) {
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(error) => {
            warn!(%peer, %error, "websocket handshake failed");
            return;
        }
    };
    info!(%peer, "client connected");

    let (mut write, mut read) = ws.split();
    loop {
        tokio::select! {
            incoming = read.next() => {
                let message = match incoming {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => {
                        warn!(%peer, %error, "websocket read error");
                        break;
                    }
                    None => break,
                };
                match message {
                    // Echo application frames — enough to prove the transport.
                    Message::Text(_) | Message::Binary(_) => {
                        if let Err(error) = write.send(message).await {
                            warn!(%peer, %error, "websocket write error");
                            break;
                        }
                    }
                    Message::Ping(payload) => {
                        if let Err(error) = write.send(Message::Pong(payload)).await {
                            warn!(%peer, %error, "failed to answer ping");
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Pong(_) | Message::Frame(_) => {}
                }
            }
            changed = close_rx.changed() => {
                // `changed()` only errs if the sender dropped, which means the
                // server is stopping regardless — close either way.
                if changed.is_ok() && *close_rx.borrow() {
                    let _ = write.send(Message::Close(None)).await;
                }
                break;
            }
        }
    }

    let _ = write.close().await;
    info!(%peer, "client disconnected");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn default_config_uses_documented_default() {
        assert_eq!(Config::default().addr, DEFAULT_ADDR);
        assert_eq!(DEFAULT_ADDR, "127.0.0.1:9000");
    }

    #[test]
    fn flag_overrides_env_and_default() {
        let cfg = Config::resolve(["--addr".to_string(), "0.0.0.0:1234".to_string()], |_| {
            Some("10.0.0.1:9999".to_string())
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
}
