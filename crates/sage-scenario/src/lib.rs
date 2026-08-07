//! **`sage-scenario` — the live scenario runner (issue #777).** Development only.
//!
//! One command opens the shipping web client on an exact, disposable game position:
//!
//! ```text
//! cargo run -p sage-scenario -- scenarios/murder-the-dreadmaw.toml
//! # Ready: http://127.0.0.1:4173/?server=ws://127.0.0.1:9010
//! ```
//!
//! The maintainer opens the URL, plays the position through the real UI, and throws the
//! game away. The format and the vocabulary are documented in `docs/scenarios.md`.
//!
//! # Why this exists
//! The pieces were all here and none of them was this. Engine and server tests construct
//! arbitrary states but have no browser; the client can replay a `GameView` fixture but has
//! no engine behind it, so nothing a click does is *legal* in any sense the rules define;
//! and the lobby starts real games, but only from turn one. So testing a mechanic by hand
//! meant writing throwaway Rust for every request. This crate is that throwaway Rust,
//! written once, with a file where the bespoke part used to be.
//!
//! # What is real, and what is not
//! **Real:** the engine generates the legal actions and applies them; the server projects
//! each seat's view, redacts hidden information, binds actions, paces the game, and drives
//! the AI; the client only renders a `GameView` and returns an advertised `action_id`. A
//! scenario changes exactly one thing — the state the game starts from.
//!
//! **Not real, deliberately:** there is no deck, no format, no mulligan, and no legality
//! check on what a scenario puts in a library. A scenario is a *position*, and the format
//! registry judges decklists for games that begin at turn one.
//!
//! # The boundaries this holds
//! - **No I/O reaches the engine.** File reading, parsing, process lifecycle, and
//!   networking live here; the engine is handed a finished [`GameState`](sage_engine::GameState).
//! - **No game logic reaches the client.** The client is the built bundle, unmodified,
//!   pointed at a socket with `?server=`.
//! - **No protocol command is added.** Nothing on the wire can inject authoritative state.
//!   The position comes from a file this process read before it bound anything, and
//!   [`serve`] refuses to bind an address that is not loopback.
//! - **Nothing ships from here.** `sage-server`, `sage-cli`, and `sage-engine` do not
//!   depend on this crate, and it is `publish = false`.

pub mod build;
pub mod error;
pub mod scenario;
pub mod serve;

pub use build::{build, Position, SeatPlan};
pub use error::{ScenarioError, Site};
pub use scenario::{parse, Scenario};
pub use serve::{accept, start, Options, Running, ServeError, DEFAULT_ADDR, DEFAULT_CLIENT_ADDR};
