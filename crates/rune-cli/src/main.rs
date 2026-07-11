//! RUNE CLI client (dev sequence steps 3–4, `docs/brief.md`).
//!
//! Connects to a running `rune-server` over WebSocket and drives one of two
//! session loops over the same two-message protocol:
//!
//! - **Interactive** (default): renders each personalized
//!   [`GameView`](rune_protocol::GameView) as a numbered list of `valid_actions`,
//!   reads a choice from stdin, and sends the matching `ChooseAction`.
//! - **Agent** (`--agent`): hands the view to an [`Agent`](rune_cli::Agent) and
//!   sends the id it picks, with a deadline and a safe fallback (pass priority) so
//!   a slow or broken model never stalls the game — proving AI opponents work.
//!
//! All the logic lives in the [`rune_cli`] library so it can be unit-tested and
//! driven over any transport; this binary only wires it to real stdin/stdout,
//! stderr, and the network.
//!
//! ## Usage
//! ```text
//! rune-cli [--addr <host:port | ws://…>] [--agent] [--agent-timeout <seconds>]
//! ```
//! The server address is taken from `--addr`/`-a`, else `RUNE_SERVER_ADDR`, else
//! the default `127.0.0.1:9000`. In agent mode the decision deadline comes from
//! `--agent-timeout`, else `RUNE_AGENT_TIMEOUT` (seconds), else 5s. The built-in
//! agent is deterministic and needs no network or secrets; see the crate docs for
//! wiring a real model provider.

use std::process::ExitCode;

use rune_cli::{
    connect, run_agent_session, run_session, AgentConfig, CliConfig, PassPriorityAgent,
    SessionError,
};
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
    let agent = match AgentConfig::from_env_and_args() {
        Ok(agent) => agent,
        Err(error) => {
            eprintln!("rune-cli: {error}");
            return ExitCode::FAILURE;
        }
    };

    let result = if agent.enabled {
        run_agent(&config, &agent).await
    } else {
        run_interactive(&config).await
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rune-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Connect and drive the interactive session over real stdin/stdout until the
/// server closes or stdin reaches EOF.
async fn run_interactive(config: &CliConfig) -> Result<(), SessionError> {
    eprintln!("rune-cli: connecting to {} ...", config.ws_url());
    let ws = connect(config).await?;
    eprintln!("rune-cli: connected. Waiting for the first game view.");
    let input = BufReader::new(tokio::io::stdin());
    let output = tokio::io::stdout();
    run_session(ws, input, output).await
}

/// Connect and drive the non-interactive agent session, logging each decision to
/// stderr, until the server closes the connection.
async fn run_agent(config: &CliConfig, agent: &AgentConfig) -> Result<(), SessionError> {
    eprintln!(
        "rune-cli: connecting to {} (agent mode) ...",
        config.ws_url()
    );
    let ws = connect(config).await?;
    eprintln!(
        "rune-cli: connected. Playing with the built-in pass-priority agent (deadline {:?}).",
        agent.deadline
    );
    let log = tokio::io::stderr();
    run_agent_session(ws, &PassPriorityAgent, agent.deadline, log).await
}
