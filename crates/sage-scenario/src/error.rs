//! What a scenario can be wrong about, said in terms of the file rather than the engine.
//!
//! Every variant names the field an author would go and change. That is the whole design
//! rule here: a runner that binds a socket and opens a browser onto a subtly broken
//! position costs more than one that refuses and says which line is at fault, so the
//! failures are typed and each carries the seat, the zone, and the value that caused it.

use std::fmt;

/// Where in a scenario a card was named, so an unresolvable one can be reported by the
/// place it was written rather than by its index alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    /// A seat's hand.
    Hand,
    /// A seat's library.
    Library,
    /// A seat's graveyard.
    Graveyard,
    /// A seat's exile.
    Exile,
    /// A seat's command zone.
    Command,
    /// A seat's battlefield.
    Battlefield,
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Hand => "hand",
            Self::Library => "library",
            Self::Graveyard => "graveyard",
            Self::Exile => "exile",
            Self::Command => "command",
            Self::Battlefield => "battlefield",
        };
        f.write_str(name)
    }
}

/// Why a scenario could not be turned into a game to serve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScenarioError {
    /// Fewer than two seats. A game needs an opponent, and the browser takes seat 0.
    NotEnoughPlayers(usize),
    /// `turn = 0`. Turns are 1-based (CR 500.1), and turn 0 is not a position.
    TurnIsZero,
    /// `active_player` or `priority` named a seat that does not exist.
    NoSuchSeat {
        /// The field that named it (`active_player`, `priority`, or `controller`).
        field: &'static str,
        /// The seat index it named.
        seat: usize,
        /// How many seats the scenario has.
        seats: usize,
    },
    /// A card name did not resolve to anything in the catalog.
    UnknownCard {
        /// The seat whose list held it.
        seat: usize,
        /// Which of that seat's lists.
        site: Site,
        /// The `functional_id` as written.
        card: String,
    },
    /// A seat named an AI kind the server does not have.
    UnknownAi {
        /// The seat that named it.
        seat: usize,
        /// The kind as written.
        kind: String,
        /// Every kind that would have worked.
        known: Vec<String>,
    },
    /// Two permanents claimed the same label, so an `attached_to` naming it would be
    /// ambiguous.
    DuplicateLabel {
        /// The label both used.
        label: String,
    },
    /// An `attached_to` named a label no permanent carries.
    UnknownLabel {
        /// The seat whose permanent named it.
        seat: usize,
        /// The label as written.
        label: String,
    },
    /// A permanent was attached to itself, which is not a state the rules have.
    AttachedToItself {
        /// The label it carries and named.
        label: String,
    },
    /// A permanent was asked not to be summoning sick under a seat that has not begun a
    /// turn yet, which no permanent under that seat can be (CR 302.6).
    CannotBeEstablished {
        /// The seat that controls it.
        seat: usize,
        /// The card, for the author to find the entry by.
        card: String,
    },
    /// A seat starts at zero or less life, so the first state-based check ends the game
    /// before anyone acts (CR 704.5a).
    LethalLife {
        /// The seat at issue.
        seat: usize,
        /// The life total it was given.
        life: i32,
    },
    /// The engine offers the seat holding priority no legal action at all, so serving the
    /// position would open a browser onto a board that cannot move.
    NoLegalAction,
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughPlayers(seats) => write!(
                f,
                "a scenario needs at least two `[[players]]` seats; this one has {seats}"
            ),
            Self::TurnIsZero => write!(f, "`turn` is 1-based; turn 0 is not a position"),
            Self::NoSuchSeat { field, seat, seats } => write!(
                f,
                "`{field}` names seat {seat}, but this scenario has {seats} seats (0..{})",
                seats.saturating_sub(1)
            ),
            Self::UnknownCard { seat, site, card } => write!(
                f,
                "seat {seat}'s {site} names `{card}`, which is not a card in this build's \
                 catalog — cards are named by their `functional_id`"
            ),
            Self::UnknownAi { seat, kind, known } => write!(
                f,
                "seat {seat} asks for the `{kind}` AI, which this server does not have; \
                 it offers: {}",
                known.join(", ")
            ),
            Self::DuplicateLabel { label } => write!(
                f,
                "two permanents are labelled `{label}`; a label points at exactly one"
            ),
            Self::UnknownLabel { seat, label } => write!(
                f,
                "seat {seat} attaches a permanent to `{label}`, which no permanent is \
                 labelled — add `label = \"{label}\"` to the one it should attach to"
            ),
            Self::AttachedToItself { label } => {
                write!(f, "the permanent labelled `{label}` is attached to itself")
            }
            Self::CannotBeEstablished { seat, card } => write!(
                f,
                "seat {seat}'s `{card}` is not summoning sick, but seat {seat} has not \
                 begun a turn yet at this `turn` — raise `turn`, or set \
                 `summoning_sick = true`"
            ),
            Self::LethalLife { seat, life } => write!(
                f,
                "seat {seat} starts at {life} life, so the game ends on the first \
                 state-based check before anyone can act (CR 704.5a)"
            ),
            Self::NoLegalAction => write!(
                f,
                "the engine offers the seat holding priority no legal action in this \
                 position; check `step`, `priority`, and whether that seat is still in \
                 the game"
            ),
        }
    }
}

impl std::error::Error for ScenarioError {}
