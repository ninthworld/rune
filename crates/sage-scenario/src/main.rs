//! The `sage-scenario` binary: read a file, refuse it or serve it, print the URL.
//!
//! The order matters and is the whole shape of this file. Everything that can be wrong is
//! decided *before* anything is bound or launched: the file parses, the position builds, the
//! address is loopback. Only then is a socket opened and a client started. A runner that
//! bound a port and opened a browser before finding out the scenario was nonsense would be
//! the "subtly broken game" this tool exists to make impossible.

use std::path::PathBuf;
use std::process::ExitCode;

use sage_engine::CardDatabase;
use sage_scenario::{accept, build, parse, start, Options, DEFAULT_ADDR, DEFAULT_CLIENT_ADDR};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

/// Where the client lives relative to this crate, for the default `--client-dir`.
///
/// A compile-time path, which is honest for a tool that only ever runs from a checkout: a
/// contributor invokes it as `cargo run -p sage-scenario`, so the source tree it was built
/// from is the source tree they are in. `--client-dir` is there for when it is not.
const CLIENT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../clients/web");

const USAGE: &str = "\
sage-scenario — open the web client on an exact, disposable game position (development only)

USAGE:
    cargo run -p sage-scenario -- <scenario.toml> [OPTIONS]

OPTIONS:
    --addr <host:port>          Loopback address the game socket binds [default: 127.0.0.1:9010]
    --client-addr <host:port>   Address the built client is served on  [default: 127.0.0.1:4173]
    --client-dir <path>         Where clients/web is [default: alongside this crate]
    --no-client                 Do not start a client; just serve the game and print the URL
    -h, --help                  Print this

The scenario format is documented in docs/scenarios.md.";

#[tokio::main]
async fn main() -> ExitCode {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    // Everything that can be refused is refused here, before a port is touched.
    let text = match std::fs::read_to_string(&args.scenario) {
        Ok(text) => text,
        Err(error) => {
            error!(path = %args.scenario.display(), %error, "could not read the scenario");
            return ExitCode::FAILURE;
        }
    };
    let scenario = match parse(&text) {
        Ok(scenario) => scenario,
        Err(error) => {
            error!(path = %args.scenario.display(), "{error}");
            return ExitCode::FAILURE;
        }
    };
    let db = match CardDatabase::bundled() {
        Ok(db) => db,
        Err(error) => {
            error!(%error, "failed to load the bundled card database");
            return ExitCode::FAILURE;
        }
    };
    let position = match build(&scenario, &db) {
        Ok(position) => position,
        Err(error) => {
            error!(path = %args.scenario.display(), "{error}");
            return ExitCode::FAILURE;
        }
    };

    let options = Options {
        addr: args.addr,
        client_addr: args.client_addr,
        client_dir: (!args.no_client).then_some(args.client_dir),
    };
    let running = match start(&position, db, &options).await {
        Ok(running) => running,
        Err(error) => {
            error!("{error}");
            return ExitCode::FAILURE;
        }
    };

    if let Some(name) = &position.name {
        info!(scenario = %name, "scenario loaded");
    }
    if let Some(note) = &position.note {
        info!("{note}");
    }
    // The one line a person is actually waiting for, on stdout so it can be piped.
    println!("Ready: {}", running.url());
    if args.no_client {
        println!("(no client started — open the address above against one you are running)");
    }

    accept(running, async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => info!("received Ctrl-C; discarding the scenario"),
            Err(error) => error!(%error, "failed to listen for Ctrl-C"),
        }
    })
    .await;
    ExitCode::SUCCESS
}

/// The parsed command line.
struct Args {
    scenario: PathBuf,
    addr: String,
    client_addr: String,
    client_dir: PathBuf,
    no_client: bool,
}

impl Args {
    /// Parse arguments. `Ok(None)` means help was asked for; `Err` carries a message to
    /// print above the usage.
    fn parse<A: IntoIterator<Item = String>>(args: A) -> Result<Option<Self>, String> {
        let mut scenario: Option<PathBuf> = None;
        let mut addr = DEFAULT_ADDR.to_string();
        let mut client_addr = DEFAULT_CLIENT_ADDR.to_string();
        let mut client_dir = PathBuf::from(CLIENT_DIR);
        let mut no_client = false;

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |flag: &str| -> Result<String, String> {
                args.next()
                    .ok_or_else(|| format!("{flag} requires a value"))
            };
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--no-client" => no_client = true,
                "--addr" => addr = value("--addr")?,
                "--client-addr" => client_addr = value("--client-addr")?,
                "--client-dir" => client_dir = PathBuf::from(value("--client-dir")?),
                other if other.starts_with("--") => {
                    if let Some((flag, given)) = other.split_once('=') {
                        match flag {
                            "--addr" => addr = given.to_string(),
                            "--client-addr" => client_addr = given.to_string(),
                            "--client-dir" => client_dir = PathBuf::from(given),
                            _ => return Err(format!("unknown option {flag}")),
                        }
                    } else {
                        return Err(format!("unknown option {other}"));
                    }
                }
                path if scenario.is_none() => scenario = Some(PathBuf::from(path)),
                extra => return Err(format!("unexpected argument {extra:?}")),
            }
        }

        let scenario = scenario.ok_or_else(|| "a scenario file is required".to_string())?;
        Ok(Some(Self {
            scenario,
            addr,
            client_addr,
            client_dir,
            no_client,
        }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn parse_args(args: &[&str]) -> Result<Option<Args>, String> {
        Args::parse(args.iter().map(|arg| (*arg).to_string()))
    }

    #[test]
    fn a_bare_path_takes_every_default() {
        let args = parse_args(&["scenarios/x.toml"]).unwrap().unwrap();
        assert_eq!(args.scenario, PathBuf::from("scenarios/x.toml"));
        assert_eq!(args.addr, DEFAULT_ADDR);
        assert_eq!(args.client_addr, DEFAULT_CLIENT_ADDR);
        assert!(!args.no_client);
    }

    #[test]
    fn options_are_accepted_in_both_spellings() {
        let spaced = parse_args(&["x.toml", "--addr", "127.0.0.1:1", "--no-client"])
            .unwrap()
            .unwrap();
        assert_eq!(spaced.addr, "127.0.0.1:1");
        assert!(spaced.no_client);

        let equals = parse_args(&["x.toml", "--client-addr=127.0.0.1:2"])
            .unwrap()
            .unwrap();
        assert_eq!(equals.client_addr, "127.0.0.1:2");
    }

    #[test]
    fn a_missing_path_a_missing_value_and_an_unknown_flag_are_all_refused() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["x.toml", "--addr"]).is_err());
        assert!(parse_args(&["x.toml", "--nope"]).is_err());
        assert!(parse_args(&["x.toml", "y.toml"]).is_err());
    }

    #[test]
    fn help_is_not_an_error() {
        assert!(parse_args(&["--help"]).unwrap().is_none());
        assert!(parse_args(&["-h"]).unwrap().is_none());
    }
}
