//! The server-side format registry and deck-legality policy.
//!
//! Game configuration splits into two layers so the pure engine holds **no format
//! policy and no I/O**:
//!
//! - the engine's [`GameSetup`] is a pure value type carrying only the
//!   rules-affecting parameters a game needs to run (player count, starting life,
//!   starting hand size); and
//! - the **format** — a named `game_setup` identifier ([`GameSetupId`]) mapped to a
//!   concrete engine [`GameSetup`] **plus deck-legality rules** (minimum/maximum
//!   deck size, per-card copy limit, basic-land exemption).
//!
//! **Deck legality is validated here, server-side — never by the engine.** It is
//! matchmaking/format policy, not a rule of an in-progress game, and keeping it out
//! of the engine preserves the engine's purity and its freedom from format churn
//! (the engine's `setup.rs` deliberately scoped deck legality out of issue #109).
//! The one engine input this module borrows is the *structured*
//! [`Supertype::Basic`] flag on a card, read through the [`CardDatabase`] — the
//! basic-land **policy** (that basics are exempt from the copy limit) lives here,
//! only the datum lives in the engine.

use std::collections::{HashMap, HashSet};

use sage_engine::{
    abilities_of, parse_mana_cost, Ability, CardData, CardDatabase, CardId, CardType, Color,
    Effect, GameSetup, PlayerSetup, Supertype,
};
use sage_protocol::GameSetupId;

/// The life total each player begins a **commander** game with (CR 903.7): 40.
/// This is engine *setup data* the server drives, not a rule the engine knows —
/// it flows through [`GameSetup::starting_life`] like any other format's life
/// total, so the engine stays free of format policy.
mod color_identity;
mod errors;

pub(crate) use color_identity::*;
pub(crate) use errors::*;

pub(crate) const COMMANDER_STARTING_LIFE: i32 = 40;

/// The deck-legality rules of a format: the server policy a submitted decklist is
/// validated against in the pre-game gate. None of this is an engine
/// rule — it is format/matchmaking policy the engine never sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeckRules {
    /// Fewest cards a legal deck may contain (inclusive).
    pub(crate) min_size: usize,
    /// Most cards a legal deck may contain (inclusive), or `None` for no upper
    /// bound.
    pub(crate) max_size: Option<usize>,
    /// The most copies of any single card (by oracle [`CardId`]) a deck may hold,
    /// unless the card is exempt (see [`DeckRules::basic_land_exempt`]).
    pub(crate) max_copies: usize,
    /// Whether basic lands (cards with the [`Supertype::Basic`] supertype) are
    /// exempt from [`max_copies`](DeckRules::max_copies), the usual Magic rule
    /// (CR 100.2a lets a deck hold any number of basic lands).
    pub(crate) basic_land_exempt: bool,
    /// Whether a legal deck must designate a **commander** that is a legendary
    /// creature (CR 903.3, 903.5a). Commander-style formats set this; every other
    /// format leaves it `false` and ignores the designation entirely, so a
    /// non-commander deck is validated exactly as before (issue #372).
    pub(crate) require_commander: bool,
    /// Whether every card's **color identity** must be contained in the
    /// commander's (CR 903.4 / 903.5c), computed from structured card data only
    /// (see [`color_identity`]). Meaningful only alongside
    /// [`require_commander`](DeckRules::require_commander); `false` for
    /// non-commander formats.
    pub(crate) enforce_color_identity: bool,
}

/// A registered format: the engine [`GameSetup`] parameters a room starts its game
/// with, plus the [`DeckRules`] its decklists are validated against.
///
/// This is the value the server's format registry maps a `game_setup` identifier
/// to. The engine-setup half is pure game configuration (starting life, hand size);
/// the [`DeckRules`] half is server-only deck-legality policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Format {
    /// Life total every player starts at (feeds [`GameSetup::starting_life`]).
    pub(crate) starting_life: i32,
    /// Opening-hand size (feeds [`GameSetup::starting_hand_size`]).
    pub(crate) starting_hand_size: usize,
    /// The inclusive seat-count range a room using this format may be created with
    /// (issue #349). Room creation rejects a seat count outside it, so a format
    /// controls how many players its games seat — a two-player format refuses a
    /// free-for-all seat count and vice versa. Always within the lobby's overall
    /// `2..=8` bound.
    pub(crate) seats: std::ops::RangeInclusive<u8>,
    /// The deck-legality rules submitted decks are validated against.
    pub(crate) deck_rules: DeckRules,
}

impl Format {
    /// The seeded starter format: a 40-card minimum, at most four copies of any
    /// non-basic card, basic lands exempt, with the engine's default starting life
    /// and hand size ("starter-1v1").
    fn starter() -> Self {
        Self {
            starting_life: sage_engine::DEFAULT_STARTING_LIFE,
            starting_hand_size: sage_engine::DEFAULT_STARTING_HAND_SIZE,
            // The starter format is a 1v1 duel.
            seats: 2..=2,
            deck_rules: DeckRules {
                min_size: 40,
                max_size: None,
                max_copies: 4,
                basic_land_exempt: true,
                require_commander: false,
                enforce_color_identity: false,
            },
        }
    }

    /// The **commander** format (`commander`, issue #372): a 100-card singleton
    /// deck (exactly 100 cards, at most one copy of any non-basic, basics exempt),
    /// a required commander that must be a legendary creature (CR 903.3/903.5a),
    /// color-identity containment (CR 903.4), 40 starting life (CR 903.7), seating
    /// 2–4. Deck legality is server policy; the engine only receives
    /// the designated commander in setup and the 40-life `GameSetup`.
    fn commander() -> Self {
        Self {
            // CR 903.7: each player begins with 40 life. The engine is told the
            // starting life through `GameSetup`; 40 is setup data, not a rule the
            // engine knows about.
            starting_life: COMMANDER_STARTING_LIFE,
            starting_hand_size: sage_engine::DEFAULT_STARTING_HAND_SIZE,
            // A commander game seats 2–4 (multiplayer or a duel); partner,
            // Two-Headed Giant, and >4 seats are out of scope (issue #372).
            seats: 2..=4,
            deck_rules: DeckRules {
                // Exactly 100 cards (CR 903.5a), expressed as a closed size range.
                min_size: 100,
                max_size: Some(100),
                // Singleton: at most one of each non-basic card (CR 903.5b).
                max_copies: 1,
                basic_land_exempt: true,
                require_commander: true,
                enforce_color_identity: true,
            },
        }
    }

    /// The permissive **multiplayer** catch-all (`standard_multiplayer`): any decklist
    /// of resolvable cards is legal, preserving the pre-format-registry behavior (ADR
    /// 0012, where `submit_deck` only checked that every identity resolved), across the
    /// lobby's full 2–8 seat plumbing range. Named competitive formats like
    /// `starter-1v1` are where size/copy limits bite; the catch-all deliberately imposes
    /// none so casual and test games play with any deck.
    ///
    /// **Its name says how many it seats, because its seat range is the widest one**
    /// (issue #707). This was `standard_2p` until a format called a duel was found to
    /// advertise eight seats; the duel keeps that id and this one took a neutral name,
    /// so no registered format's identity promises a game it does not seat.
    fn open() -> Self {
        Self {
            starting_life: sage_engine::DEFAULT_STARTING_LIFE,
            starting_hand_size: sage_engine::DEFAULT_STARTING_HAND_SIZE,
            // The permissive catch-all keeps the lobby's full 2–8 seat plumbing range;
            // every named format narrows it to the game its name describes.
            seats: 2..=8,
            deck_rules: DeckRules {
                min_size: 0,
                max_size: None,
                max_copies: usize::MAX,
                basic_land_exempt: true,
                require_commander: false,
                enforce_color_identity: false,
            },
        }
    }

    /// The permissive **duel** format (`standard_2p`, and the web client's `1v1`): the
    /// same no-deck-rules openness as [`Self::open`], seating exactly two (issue #707).
    ///
    /// A format named for a duel seats a duel. Before #707 both ids resolved to
    /// [`Self::open`] and would open an eight-seat room, which is the contradiction that
    /// issue names: the identifiers, the comments, the protocol examples, and the
    /// client's label all said two.
    fn duel() -> Self {
        Self {
            seats: 2..=2,
            ..Self::open()
        }
    }

    /// A permissive **free-for-all** format (`standard_ffa`, issue #349): the same
    /// no-deck-rules openness as [`Self::open`], seating 3–4 players. This is the
    /// format that starts real multiplayer games on the engine's multiplayer rules
    /// (#341/#342/#344); a room created with it and 3 or 4 seats runs a free-for-all.
    fn open_ffa() -> Self {
        Self {
            seats: 3..=4,
            ..Self::open()
        }
    }

    /// Build the engine [`GameSetup`] this format starts a game with, from each
    /// seat's already-validated `players` decklists and a server-generated
    /// `rng_seed`. The format supplies the rules-affecting parameters (starting
    /// life and hand size); the engine owns everything past construction.
    pub(crate) fn game_setup(&self, players: Vec<PlayerSetup>, rng_seed: u64) -> GameSetup {
        GameSetup {
            players,
            starting_life: self.starting_life,
            starting_hand_size: self.starting_hand_size,
            rng_seed,
        }
    }

    /// Validate a resolved decklist against this format's [`DeckRules`], reading the
    /// basic-land supertype through `db` for the copy-limit exemption.
    ///
    /// `deck` is the fully expanded decklist (one [`CardId`] per physical card, so
    /// four copies of a card appear four times), already resolved against the card
    /// database by the caller; `commander` is the seat's designated commander (CR
    /// 903.3), or `None` if it designated none. Validation is server policy only
    ///: it checks deck size; the per-oracle copy limit (basics exempt
    /// when [`DeckRules::basic_land_exempt`] is set); and, for a commander format
    /// ([`DeckRules::require_commander`]), that the designation is one of the deck's
    /// cards and a **legendary creature** (CR 903.5a) and — when
    /// [`DeckRules::enforce_color_identity`] is set — that every card's color
    /// identity is contained in the commander's (CR 903.4). Everything is read from
    /// structured card data through `db`; nothing parses generated display text.
    ///
    /// # Errors
    /// Returns the first [`DeckError`] the deck violates, in this order: size, copy
    /// limit, commander legality (missing / not in deck / not a legendary creature),
    /// then color identity. A non-commander format ignores `commander` entirely.
    pub(crate) fn validate_deck(
        &self,
        deck: &[CardId],
        commander: Option<CardId>,
        db: &CardDatabase,
    ) -> Result<(), DeckError> {
        let rules = &self.deck_rules;
        if deck.len() < rules.min_size {
            return Err(DeckError::BelowMinimum {
                have: deck.len(),
                min: rules.min_size,
            });
        }
        if let Some(max) = rules.max_size {
            if deck.len() > max {
                return Err(DeckError::AboveMaximum {
                    have: deck.len(),
                    max,
                });
            }
        }
        // Tally copies per oracle id, then flag the first non-exempt card over the
        // limit. A stable scan (deck order) makes the reported card deterministic.
        let mut counts: HashMap<CardId, usize> = HashMap::new();
        for &card in deck {
            *counts.entry(card).or_insert(0) += 1;
        }
        for &card in deck {
            let count = counts.get(&card).copied().unwrap_or(0);
            if count > rules.max_copies && !(rules.basic_land_exempt && is_basic(db, card)) {
                return Err(DeckError::CopyLimit {
                    card,
                    count,
                    limit: rules.max_copies,
                });
            }
        }
        // Commander-specific legality (CR 903), only for a format that asks for it.
        if rules.require_commander {
            let commander = commander.ok_or(DeckError::MissingCommander)?;
            // The commander is one of the deck's 100 cards (CR 903.3).
            if !deck.contains(&commander) {
                return Err(DeckError::CommanderNotInDeck { card: commander });
            }
            // It must be a legendary creature (CR 903.5a).
            if !is_legendary_creature(db, commander) {
                return Err(DeckError::CommanderNotLegendaryCreature { card: commander });
            }
            // Color-identity containment (CR 903.4): every card's identity ⊆ the
            // commander's, computed from structured data. First offender in deck
            // order is reported for a deterministic message.
            if rules.enforce_color_identity {
                let allowed = color_identity(db, commander);
                for &card in deck {
                    if !color_identity(db, card).is_subset(&allowed) {
                        return Err(DeckError::OutOfIdentity { card });
                    }
                }
            }
        }
        Ok(())
    }
}

/// The server's registry mapping each `game_setup` [`GameSetupId`] to its
/// [`Format`]. A `CreateRoom` naming an id absent from the registry
/// is rejected before a room is opened; a room's submitted decks are validated
/// against the [`DeckRules`] of the format its id resolves to.
#[derive(Clone, Debug)]
pub(crate) struct FormatRegistry {
    /// The registered formats, keyed by their `game_setup` identifier.
    formats: HashMap<GameSetupId, Format>,
}

impl FormatRegistry {
    /// The identifier of the default two-player format, carried in the protocol's
    /// `RoomConfig` examples (`docs/protocol.md`). Seats exactly two (issue #707).
    const DEFAULT_ID: &'static str = "standard_2p";

    /// The identifier of the permissive multiplayer catch-all (issue #707): no deck
    /// rules, the lobby's full 2–8 seats, and a name that says so. This is where a room
    /// wanting more than four seats is made now that `standard_2p` seats a duel.
    const MULTIPLAYER_ID: &'static str = "standard_multiplayer";

    /// The identifier of the seeded starter format.
    const STARTER_ID: &'static str = "starter-1v1";

    /// The identifier of the free-for-all format (issue #349): 3–4 seats.
    const FFA_ID: &'static str = "standard_ffa";

    /// The identifier of the commander format (issue #372): 100-card singleton,
    /// color identity, 40 life, seats 2–4.
    const COMMANDER_ID: &'static str = "commander";

    /// Build the registry seeded with the competitive starter format
    /// (`starter-1v1`: 40-card minimum, four copies per non-basic, basics exempt)
    /// and the permissive default two-player format (`standard_2p`: no size or copy
    /// limits, the pre-registry behavior). Deck-legality rules are the point of
    /// difference — the default catch-all imposes none so any resolvable deck plays.
    ///
    /// **Every registered format's name and its seat range describe the same game**
    /// (issue #707), which is checked over the whole registry in `format::tests`.
    pub(crate) fn with_defaults() -> Self {
        let mut formats = HashMap::new();
        // The competitive starter format enforces deck legality (size + copy limits).
        formats.insert(Self::STARTER_ID.to_string(), Format::starter());
        // Permissive duel formats: the CLI's/protocol's `standard_2p` default and the
        // web client's `1v1` (LobbyScreen). No deck rules, exactly two seats.
        for id in [Self::DEFAULT_ID, "1v1"] {
            formats.insert(id.to_string(), Format::duel());
        }
        // The permissive multiplayer catch-all, which is where the lobby's full 2–8 seat
        // range is reachable from. Neutrally named, because it is the one format that
        // seats anything from a duel to eight.
        formats.insert(Self::MULTIPLAYER_ID.to_string(), Format::open());
        // Permissive free-for-all formats seating 3–4 players (issue #349): the web
        // client's `ffa-4` and the named `standard_ffa`. These start real multiplayer
        // games on the engine's multiplayer rules; an id absent here is still rejected
        // by `create_room`.
        for id in [Self::FFA_ID, "ffa-4"] {
            formats.insert(id.to_string(), Format::open_ffa());
        }
        // The commander format (issue #372): 100-card singleton with color-identity
        // containment, a required legendary-creature commander, 40 starting life, and
        // 2–4 seats. Deck legality is enforced entirely here.
        formats.insert(Self::COMMANDER_ID.to_string(), Format::commander());
        Self { formats }
    }

    /// Resolve a `game_setup` identifier to its [`Format`], or `None` if the id
    /// names no registered format.
    pub(crate) fn get(&self, game_setup: &str) -> Option<&Format> {
        self.formats.get(game_setup)
    }

    /// Iterate every registered format with its `game_setup` identifier, for the
    /// lobby catalog projection (issue #367). Unordered — the catalog builder sorts
    /// by id for a deterministic wire order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&GameSetupId, &Format)> {
        self.formats.iter()
    }
}

#[cfg(test)]
mod tests;
