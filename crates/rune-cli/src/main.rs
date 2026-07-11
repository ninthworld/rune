//! RUNE CLI client (dev sequence step 3, `docs/brief.md`).
//!
//! Connects to a running `rune-server` over WebSocket, renders each personalized
//! [`GameView`](rune_protocol::GameView) as a numbered list of `valid_actions`,
//! reads a choice from stdin, and sends the matching `ChooseAction` — proving the
//! two-message protocol end to end without a UI. All the logic lives in the
//! [`rune_cli`] library so it can be unit-tested and driven over any transport;
//! this binary only wires it to real stdin/stdout and the network.
//!
//! ## Usage
//! ```text
//! rune-cli [--addr <host:port | ws://… >]
//! ```
//! The server address is taken from `--addr`/`-a`, else the `RUNE_SERVER_ADDR`
//! environment variable, else the default `127.0.0.1:9000`. A bare `host:port`
//! is dialed as `ws://host:port`.

use std::process::ExitCode;

use rune_cli::{connect, run_session, CliConfig};
use tokio::io::BufReader;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match CliConfig::from_env_and_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("rune-cli: {error}");
            return ExitCode::FAILURE;
        }
    };

    match run(&config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rune-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Connect to the configured server and drive the interactive session over real
/// stdin/stdout until the server closes or stdin reaches EOF.
async fn run(config: &CliConfig) -> Result<(), rune_cli::SessionError> {
    eprintln!("rune-cli: connecting to {} ...", config.ws_url());
    let ws = connect(config).await?;
    eprintln!("rune-cli: connected. Waiting for the first game view.");
    let input = BufReader::new(tokio::io::stdin());
    let output = tokio::io::stdout();
    run_session(ws, input, output).await
}
