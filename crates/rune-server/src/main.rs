//! RUNE server — layers 1 (lobby) and 2 (rooms) per docs/brief.md.
//! Owns networking, sessions, and timers. Never owns rules — that is rune-engine.
//!
//! This binary wires the [`rune_server`] transport to the process: it reads the
//! listen address from the environment/CLI, initialises logging, binds the
//! WebSocket listener, and serves until Ctrl-C.

use std::process::ExitCode;

use rune_server::{Config, Lobby, Server};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let config = match Config::from_env_and_args() {
        Ok(config) => config,
        Err(error) => {
            error!(%error, "invalid configuration");
            return ExitCode::FAILURE;
        }
    };

    // The lobby owns the room registry and the card database every room is built
    // from. A snapshot that fails to parse means we cannot host games at all. A
    // pinned `--rng-seed` (ADR 0014) and/or `--starting-life` make every game
    // reproducible and short for the deterministic e2e suite; normal play leaves
    // both unset.
    if let Some(seed) = config.rng_seed {
        info!(seed, "pinning a fixed engine shuffle seed for every game");
    }
    if let Some(life) = config.starting_life {
        info!(life, "pinning a fixed starting life total for every game");
    }
    let lobby = match Lobby::bundled_with_overrides(
        Lobby::DEFAULT_MAX_ROOMS,
        config.rng_seed,
        config.starting_life,
    ) {
        Ok(lobby) => lobby,
        Err(error) => {
            error!(%error, "failed to load bundled card database");
            return ExitCode::FAILURE;
        }
    };

    let server = match Server::bind(&config).await {
        Ok(server) => server,
        Err(error) => {
            error!(%error, addr = %config.addr, "failed to bind listener");
            return ExitCode::FAILURE;
        }
    };
    info!(addr = %server.local_addr(), "rune-server listening");

    // Graceful shutdown on Ctrl-C; a signal-registration failure just means we
    // never trigger shutdown, so log it and keep serving.
    let shutdown = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => info!("received Ctrl-C"),
            Err(error) => error!(%error, "failed to listen for Ctrl-C"),
        }
    };

    if let Err(error) = server.run(lobby, shutdown).await {
        error!(%error, "server exited with an error");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Initialise `tracing` output, honouring `RUST_LOG` and defaulting to `info`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // `try_init` fails only if a subscriber is already set; ignore that.
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
