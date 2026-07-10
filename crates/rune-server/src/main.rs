//! RUNE server — layers 1 (lobby) and 2 (rooms) per docs/brief.md.
//! Owns networking, sessions, and timers. Never owns rules — that is rune-engine.

fn main() {
    let state = rune_engine::GameState::default();
    println!("rune-server scaffold (engine state: turn {})", state.turn);
}
