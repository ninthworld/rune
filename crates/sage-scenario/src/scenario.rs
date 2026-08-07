//! The scenario file: the declarative description of a position, exactly as authored.
//!
//! This module is the *shape* of a scenario and nothing else — it parses, it does not
//! judge. Every field here is a value a contributor (or an agent writing on their behalf)
//! typed into a TOML file; whether the combination is a position the engine can actually
//! run is [`build`](crate::build)'s question, and it answers it with a named field and a
//! reason rather than a half-built game.
//!
//! Two decisions are worth naming, because they are what keep an authored file readable:
//!
//! - **Cards are named by `functional_id`** (ADR 0008 §3), the same identity a decklist
//!   and the wire use. A `CardId` is interned from the catalog's sort order and would
//!   silently come to mean a different card the moment one is authored ahead of it, so it
//!   is never a thing a file may say.
//! - **Nothing here names an internal id.** A permanent is addressed by an optional
//!   [`label`](PermanentScenario::label) the author picks; object and physical-card
//!   identities are minted by the builder from the engine's own monotonic counter. An
//!   author who never writes an attachment never writes a label either.
//!
//! Every table sets `deny_unknown_fields`, so a misspelled key is a parse error naming the
//! key rather than a setting that silently did nothing.

use std::collections::BTreeMap;

use sage_engine::{CounterKind, Face, Step};
use serde::Deserialize;

/// One authored position: who is playing, where the turn is, and what is where.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// A human label for the position, shown when the runner starts. Presentation only.
    pub name: Option<String>,
    /// A one-line note about what the position is for. Presentation only.
    pub note: Option<String>,
    /// The seed every piece of randomness in the run draws from: the engine's own stream
    /// (ADR 0006) and each AI seat's policy. The same file and the same seed replay
    /// identically. Defaults to `0` so a file that omits it is still deterministic.
    #[serde(default)]
    pub seed: u64,
    /// The turn number the position sits on (1-based). It is what summoning sickness and
    /// every "this turn" allowance are measured against, so a position wanting
    /// established permanents wants a turn past the first.
    #[serde(default = "default_turn")]
    pub turn: u32,
    /// The step the turn is in. Defaults to the precombat main phase, which is where a
    /// position that wants to cast something belongs.
    #[serde(default)]
    pub step: StepName,
    /// The seat whose turn it is, as an index into [`Scenario::players`].
    #[serde(default)]
    pub active_player: usize,
    /// The seat holding priority. Defaults to the active player.
    pub priority: Option<usize>,
    /// Every seat, in seating (turn) order. Seat `n` is `PlayerId(n)`; seat 0 is the one
    /// the browser is handed.
    pub players: Vec<PlayerScenario>,
}

/// One seat: who plays it, and what it has.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerScenario {
    /// The display name this seat is labelled with in every `GameView`.
    pub name: Option<String>,
    /// The server-side AI that plays this seat (`"random"` today), or absent for a seat a
    /// person plays. Only seat 0 is bridged to the browser, so every other seat wants one.
    pub ai: Option<String>,
    /// This seat's life total.
    #[serde(default = "default_life")]
    pub life: i32,
    /// The cards in hand, in the order they appear there.
    #[serde(default)]
    pub hand: Vec<String>,
    /// The library, **top card first** — the next card this seat would draw is the first
    /// entry. Not shuffled: a scenario's library is authored, not dealt.
    #[serde(default)]
    pub library: Vec<String>,
    /// The graveyard, oldest first.
    #[serde(default)]
    pub graveyard: Vec<String>,
    /// The cards this seat owns in exile.
    #[serde(default)]
    pub exile: Vec<String>,
    /// The cards in this seat's command zone (CR 408). Designating a commander is not
    /// part of this vocabulary yet — see `docs/scenarios.md`.
    #[serde(default)]
    pub command: Vec<String>,
    /// Mana already in this seat's pool (CR 106.4).
    #[serde(default)]
    pub mana: ManaScenario,
    /// The permanents this seat owns. Control may be handed elsewhere per permanent with
    /// [`PermanentScenario::controller`]; the seat a permanent is listed under always owns
    /// it, which is where it goes when it leaves the battlefield (CR 400.7).
    #[serde(default)]
    pub battlefield: Vec<PermanentScenario>,
}

/// One permanent on the battlefield.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentScenario {
    /// The card's `functional_id`.
    pub card: String,
    /// A name **this file** uses to point at this permanent, for
    /// [`attached_to`](Self::attached_to). Never an engine id, and never on the wire.
    pub label: Option<String>,
    /// The seat that controls it, when that is not the seat that owns it — a stolen
    /// creature. Modelled the way the engine models one: a CR 613 layer 2 control-changing
    /// effect, so every rule that asks who controls it reads the same computed answer.
    pub controller: Option<usize>,
    /// Whether it is tapped.
    #[serde(default)]
    pub tapped: bool,
    /// Whether it is **summoning sick** (CR 302.6) — it came under its controller's
    /// control this turn. Defaults to `false`: a scenario's permanents are established
    /// unless the position is about one that is not.
    #[serde(default)]
    pub summoning_sick: bool,
    /// Damage already marked on it this turn (CR 120.3).
    #[serde(default)]
    pub damage: u32,
    /// Counters on it, keyed by the engine's own counter names
    /// (`plus_one_plus_one`, `minus_one_minus_one`, `loyalty`, `charge`, …).
    #[serde(default)]
    pub counters: BTreeMap<CounterKind, u32>,
    /// The [`label`](Self::label) of the permanent this one is attached to — an Aura on
    /// its host, an Equipment on the creature it equips (CR 303.4 / 301.5).
    pub attached_to: Option<String>,
    /// Which face is up (CR 712.4). Defaults to the front.
    #[serde(default)]
    pub face: FaceName,
}

/// Mana sitting in a pool, by colour (CR 106.1). Restricted mana (CR 106.6) is not part of
/// this vocabulary yet.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaScenario {
    /// `{W}`.
    #[serde(default)]
    pub white: u8,
    /// `{U}`.
    #[serde(default)]
    pub blue: u8,
    /// `{B}`.
    #[serde(default)]
    pub black: u8,
    /// `{R}`.
    #[serde(default)]
    pub red: u8,
    /// `{G}`.
    #[serde(default)]
    pub green: u8,
    /// `{C}`.
    #[serde(default)]
    pub colorless: u8,
}

impl ManaScenario {
    /// Whether this pool is empty, which is what almost every seat's is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.white == 0
            && self.blue == 0
            && self.black == 0
            && self.red == 0
            && self.green == 0
            && self.colorless == 0
    }
}

/// The step a scenario sits in, spelled the way a file spells it.
///
/// A mirror of the engine's [`Step`] rather than a `Deserialize` on it: the engine derives
/// serde only for compile-time card data (ADR 0002), and the vocabulary a file uses is this
/// crate's contract to keep stable, not the engine's.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepName {
    /// CR 502.
    Untap,
    /// CR 503.
    Upkeep,
    /// CR 504.
    Draw,
    /// CR 505 — the default, and where a position that wants to cast something belongs.
    #[default]
    PrecombatMain,
    /// CR 507.
    BeginCombat,
    /// CR 508.
    DeclareAttackers,
    /// CR 509.
    DeclareBlockers,
    /// CR 510.
    CombatDamage,
    /// CR 511.
    EndCombat,
    /// CR 505, after combat.
    PostcombatMain,
    /// CR 513.
    End,
    /// CR 514.
    Cleanup,
}

impl From<StepName> for Step {
    fn from(step: StepName) -> Self {
        match step {
            StepName::Untap => Step::Untap,
            StepName::Upkeep => Step::Upkeep,
            StepName::Draw => Step::Draw,
            StepName::PrecombatMain => Step::PrecombatMain,
            StepName::BeginCombat => Step::BeginCombat,
            StepName::DeclareAttackers => Step::DeclareAttackers,
            StepName::DeclareBlockers => Step::DeclareBlockers,
            StepName::CombatDamage => Step::CombatDamage,
            StepName::EndCombat => Step::EndCombat,
            StepName::PostcombatMain => Step::PostcombatMain,
            StepName::End => Step::End,
            StepName::Cleanup => Step::Cleanup,
        }
    }
}

/// Which face of a card is up, spelled the way a file spells it — the mirror of
/// [`Face`], and here for the same reason [`StepName`] is.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaceName {
    /// The front face (CR 712.4a).
    #[default]
    Front,
    /// The back face, for a permanent that has transformed (CR 712.2a).
    Back,
}

impl From<FaceName> for Face {
    fn from(face: FaceName) -> Self {
        match face {
            FaceName::Front => Face::Front,
            FaceName::Back => Face::Back,
        }
    }
}

/// A scenario that says nothing about the turn is on turn 3: far enough in that a
/// permanent can be established and a land drop has been available, near enough that the
/// number reads as a real position.
fn default_turn() -> u32 {
    3
}

/// A seat that says nothing about its life total starts where the rules do (CR 103.4).
fn default_life() -> i32 {
    sage_engine::DEFAULT_STARTING_LIFE
}

/// Parse a scenario from the text of a TOML file.
///
/// # Errors
/// Returns the parser's own error, which names the line, the column, and the key — the
/// diagnostic an author needs and one this crate has no way to improve on.
pub fn parse(text: &str) -> Result<Scenario, toml::de::Error> {
    toml::from_str(text)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn a_minimal_scenario_fills_in_every_default() {
        let scenario = parse(
            r#"
            [[players]]
            [[players]]
            ai = "random"
            "#,
        )
        .expect("two bare seats are a scenario");
        assert_eq!(scenario.turn, default_turn());
        assert_eq!(scenario.step, StepName::PrecombatMain);
        assert_eq!(scenario.seed, 0);
        assert_eq!(scenario.active_player, 0);
        assert_eq!(scenario.priority, None);
        assert_eq!(scenario.players.len(), 2);
        assert_eq!(scenario.players[0].life, default_life());
        assert!(scenario.players[0].ai.is_none());
        assert_eq!(scenario.players[1].ai.as_deref(), Some("random"));
        assert!(scenario.players[0].mana.is_empty());
    }

    #[test]
    fn a_misspelled_key_is_a_parse_error_naming_it() {
        // The whole point of `deny_unknown_fields`: a setting that silently did nothing is
        // exactly the "subtly broken game" this tool exists not to open.
        let error = parse(
            r#"
            [[players]]
            lfie = 3
            "#,
        )
        .expect_err("an unknown key is refused");
        assert!(
            error.to_string().contains("lfie"),
            "the error names the key: {error}"
        );
    }

    #[test]
    fn counters_are_named_with_the_engines_own_vocabulary() {
        let scenario = parse(
            r#"
            [[players]]
            [[players.battlefield]]
            card = "colossal_dreadmaw"
            counters = { plus_one_plus_one = 2, minus_one_minus_one = 1 }
            "#,
        )
        .expect("counter names parse");
        let counters = &scenario.players[0].battlefield[0].counters;
        assert_eq!(counters.get(&CounterKind::PlusOnePlusOne), Some(&2));
        assert_eq!(counters.get(&CounterKind::MinusOneMinusOne), Some(&1));
    }

    #[test]
    fn every_step_name_maps_onto_an_engine_step() {
        // The mirror is only useful while it is complete: parse each name and check it
        // lands on the step it is named for.
        for (name, step) in [
            ("untap", Step::Untap),
            ("upkeep", Step::Upkeep),
            ("draw", Step::Draw),
            ("precombat_main", Step::PrecombatMain),
            ("begin_combat", Step::BeginCombat),
            ("declare_attackers", Step::DeclareAttackers),
            ("declare_blockers", Step::DeclareBlockers),
            ("combat_damage", Step::CombatDamage),
            ("end_combat", Step::EndCombat),
            ("postcombat_main", Step::PostcombatMain),
            ("end", Step::End),
            ("cleanup", Step::Cleanup),
        ] {
            let scenario = parse(&format!(
                "step = \"{name}\"\n[[players]]\n[[players]]\nai = \"random\"\n"
            ))
            .unwrap_or_else(|error| panic!("`{name}` is a step name: {error}"));
            assert_eq!(Step::from(scenario.step), step);
        }
    }

    #[test]
    fn a_face_is_named_front_or_back() {
        let scenario = parse(
            r#"
            [[players]]
            [[players.battlefield]]
            card = "colossal_dreadmaw"
            face = "back"
            "#,
        )
        .expect("a face parses");
        assert_eq!(
            Face::from(scenario.players[0].battlefield[0].face),
            Face::Back
        );
    }
}
