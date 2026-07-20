//! The server-side format registry and deck-legality policy (ADR 0013 §4).
//!
//! ADR 0013 §4 splits game configuration into two layers so the pure engine holds
//! **no format policy and no I/O**:
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
//! (ADR 0013 §4; the engine's `setup.rs` deliberately scoped deck legality out of
//! issue #109). The one engine input this module borrows is the *structured*
//! [`Supertype::Basic`] flag on a card, read through the [`CardDatabase`] — the
//! basic-land **policy** (that basics are exempt from the copy limit) lives here,
//! only the datum lives in the engine.

use std::collections::HashMap;

use rune_engine::{CardDatabase, CardId, GameSetup, PlayerSetup, Supertype};
use rune_protocol::GameSetupId;

/// The deck-legality rules of a format: the server policy a submitted decklist is
/// validated against in the pre-game gate (ADR 0013 §4). None of this is an engine
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
}

/// A registered format: the engine [`GameSetup`] parameters a room starts its game
/// with, plus the [`DeckRules`] its decklists are validated against (ADR 0013 §4).
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
    /// and hand size (ADR 0013 §4, "starter-1v1").
    fn starter() -> Self {
        Self {
            starting_life: rune_engine::DEFAULT_STARTING_LIFE,
            starting_hand_size: rune_engine::DEFAULT_STARTING_HAND_SIZE,
            // The starter format is a 1v1 duel.
            seats: 2..=2,
            deck_rules: DeckRules {
                min_size: 40,
                max_size: None,
                max_copies: 4,
                basic_land_exempt: true,
            },
        }
    }

    /// The permissive default two-player format (`standard_2p`): any decklist of
    /// resolvable cards is legal, preserving the pre-format-registry behavior (ADR
    /// 0012, where `submit_deck` only checked that every identity resolved). Named
    /// competitive formats like `starter-1v1` are where size/copy limits bite; the
    /// catch-all default deliberately imposes none so casual and test games play
    /// with any deck.
    fn open() -> Self {
        Self {
            starting_life: rune_engine::DEFAULT_STARTING_LIFE,
            starting_hand_size: rune_engine::DEFAULT_STARTING_HAND_SIZE,
            // The permissive catch-all keeps the lobby's full 2–8 seat plumbing range
            // (ADR 0012); named formats like the free-for-all narrow it.
            seats: 2..=8,
            deck_rules: DeckRules {
                min_size: 0,
                max_size: None,
                max_copies: usize::MAX,
                basic_land_exempt: true,
            },
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
    /// database by the caller. Validation is server policy only (ADR 0013 §4): it
    /// checks deck size and, per oracle [`CardId`], the copy limit — basics exempt
    /// when [`DeckRules::basic_land_exempt`] is set.
    ///
    /// # Errors
    /// Returns the first [`DeckError`] the deck violates: size before copy limits.
    pub(crate) fn validate_deck(
        &self,
        deck: &[CardId],
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
        Ok(())
    }
}

/// Why a submitted decklist is illegal for a format (ADR 0013 §4). Distinct from
/// an *unknown card*, which the lobby rejects before legality is even considered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DeckError {
    /// The deck holds fewer than the format's minimum number of cards.
    BelowMinimum {
        /// How many cards the deck holds.
        have: usize,
        /// The format's minimum deck size.
        min: usize,
    },
    /// The deck holds more than the format's maximum number of cards.
    AboveMaximum {
        /// How many cards the deck holds.
        have: usize,
        /// The format's maximum deck size.
        max: usize,
    },
    /// A single non-exempt card appears more times than the format's copy limit.
    CopyLimit {
        /// The offending card.
        card: CardId,
        /// How many copies the deck holds.
        count: usize,
        /// The format's per-card copy limit.
        limit: usize,
    },
}

impl std::fmt::Display for DeckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BelowMinimum { have, min } => {
                write!(f, "deck has {have} cards, below the {min}-card minimum")
            }
            Self::AboveMaximum { have, max } => {
                write!(f, "deck has {have} cards, above the {max}-card maximum")
            }
            Self::CopyLimit { card, count, limit } => write!(
                f,
                "card {} appears {count} times, above the {limit}-copy limit",
                card.0
            ),
        }
    }
}

impl std::error::Error for DeckError {}

/// Whether `card` is a basic land — carries the engine's structured
/// [`Supertype::Basic`] — read through `db`. The basic-land *policy* (exemption
/// from the copy limit) lives in [`Format::validate_deck`]; only this datum is the
/// engine's (ADR 0013 §4). An unknown id is treated as non-basic; the lobby has
/// already rejected unknown ids before legality is checked.
fn is_basic(db: &CardDatabase, card: CardId) -> bool {
    db.card(card)
        .is_some_and(|data| data.supertypes.contains(&Supertype::Basic))
}

/// The server's registry mapping each `game_setup` [`GameSetupId`] to its
/// [`Format`] (ADR 0013 §4). A `CreateRoom` naming an id absent from the registry
/// is rejected before a room is opened; a room's submitted decks are validated
/// against the [`DeckRules`] of the format its id resolves to.
#[derive(Clone, Debug)]
pub(crate) struct FormatRegistry {
    /// The registered formats, keyed by their `game_setup` identifier.
    formats: HashMap<GameSetupId, Format>,
}

impl FormatRegistry {
    /// The identifier of the default two-player format, carried in the protocol's
    /// `RoomConfig` examples (`docs/protocol.md`).
    const DEFAULT_ID: &'static str = "standard_2p";

    /// The identifier of the seeded starter format (ADR 0013 §4).
    const STARTER_ID: &'static str = "starter-1v1";

    /// The identifier of the free-for-all format (issue #349): 3–4 seats.
    const FFA_ID: &'static str = "standard_ffa";

    /// Build the registry seeded with the competitive starter format
    /// (`starter-1v1`: 40-card minimum, four copies per non-basic, basics exempt)
    /// and the permissive default two-player format (`standard_2p`: no size or copy
    /// limits, the pre-registry behavior). Deck-legality rules are the point of
    /// difference — the default catch-all imposes none so any resolvable deck plays.
    pub(crate) fn with_defaults() -> Self {
        let mut formats = HashMap::new();
        // The competitive starter format enforces deck legality (size + copy limits).
        formats.insert(Self::STARTER_ID.to_string(), Format::starter());
        // Permissive two-player catch-all formats: the CLI's/protocol's `standard_2p`
        // default and the web client's `1v1` (LobbyScreen). No deck rules (ADR 0012).
        for id in [Self::DEFAULT_ID, "1v1"] {
            formats.insert(id.to_string(), Format::open());
        }
        // Permissive free-for-all formats seating 3–4 players (issue #349): the web
        // client's `ffa-4` and the named `standard_ffa`. These start real multiplayer
        // games on the engine's multiplayer rules; an id absent here is still rejected
        // by `create_room` (ADR 0013 §4).
        for id in [Self::FFA_ID, "ffa-4"] {
            formats.insert(id.to_string(), Format::open_ffa());
        }
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;

    /// The bundled database. Forest is its only basic land.
    fn db() -> CardDatabase {
        CardDatabase::bundled().expect("bundled cards")
    }

    /// The five non-basic cards these deck tests build with.
    const NON_BASICS: [&str; 5] = [
        "onakke_ogre",
        "snapping_drake",
        "fire_elemental",
        "giant_spider",
        "walking_corpse",
    ];

    /// A legal 40-card starter deck: four copies each of the five non-basics plus
    /// twenty basic Forests.
    fn legal_deck() -> Vec<CardId> {
        let mut deck = Vec::new();
        for slug in NON_BASICS {
            for _ in 0..4 {
                deck.push(fixture(slug));
            }
        }
        for _ in 0..20 {
            deck.push(fixture("forest"));
        }
        deck
    }

    #[test]
    fn registry_resolves_seeded_ids_and_rejects_unknown() {
        let registry = FormatRegistry::with_defaults();
        assert!(registry.get("starter-1v1").is_some());
        assert!(registry.get("standard_2p").is_some());
        // The ids the web client's create form actually sends must resolve, or
        // `create_room` would reject every real room (regression guard for the
        // real-server e2e).
        assert!(registry.get("1v1").is_some());
        assert!(registry.get("ffa-4").is_some());
        assert!(registry.get("no-such-format").is_none());
    }

    #[test]
    fn issue_349_ffa_format_seats_three_to_four_and_duels_seat_two() {
        let registry = FormatRegistry::with_defaults();
        // The free-for-all format seats 3–4 players; a duel format seats exactly two.
        let ffa = registry
            .get("standard_ffa")
            .expect("standard_ffa is registered");
        assert_eq!(ffa.seats, 3..=4);
        assert!(!ffa.seats.contains(&2) && ffa.seats.contains(&3) && ffa.seats.contains(&4));
        // The FFA format imposes no deck rules (permissive, like the open default).
        assert_eq!(ffa.deck_rules, Format::open().deck_rules);
        // The seeded competitive starter is a 1v1 duel.
        assert_eq!(registry.get("starter-1v1").unwrap().seats, 2..=2);
    }

    #[test]
    fn seeded_format_yields_a_game_setup_with_its_parameters() {
        let format = FormatRegistry::with_defaults()
            .get("starter-1v1")
            .unwrap()
            .clone();
        let setup = format.game_setup(vec![PlayerSetup::new(legal_deck())], 7);
        assert_eq!(setup.starting_life, rune_engine::DEFAULT_STARTING_LIFE);
        assert_eq!(
            setup.starting_hand_size,
            rune_engine::DEFAULT_STARTING_HAND_SIZE
        );
        assert_eq!(setup.rng_seed, 7);
        assert_eq!(setup.players.len(), 1);
    }

    #[test]
    fn a_legal_deck_including_many_basics_is_accepted() {
        // Twenty basic Forests far exceed the four-copy limit, yet are exempt.
        let format = Format::starter();
        assert_eq!(format.validate_deck(&legal_deck(), &db()), Ok(()));
    }

    #[test]
    fn a_deck_under_the_minimum_size_is_rejected() {
        let format = Format::starter();
        let small = vec![fixture("forest"); 39];
        assert_eq!(
            format.validate_deck(&small, &db()),
            Err(DeckError::BelowMinimum { have: 39, min: 40 }),
        );
    }

    #[test]
    fn over_the_copy_limit_for_a_non_basic_is_rejected() {
        // Five copies of one non-basic with an otherwise legal 40-card deck.
        let mut deck = vec![fixture("onakke_ogre"); 5];
        for slug in &NON_BASICS[1..] {
            for _ in 0..4 {
                deck.push(fixture(slug));
            }
        }
        for _ in 0..19 {
            deck.push(fixture("forest"));
        }
        assert_eq!(deck.len(), 40);
        assert_eq!(
            Format::starter().validate_deck(&deck, &db()),
            Err(DeckError::CopyLimit {
                card: fixture("onakke_ogre"),
                count: 5,
                limit: 4,
            }),
        );
    }

    #[test]
    fn basics_are_only_exempt_when_the_rule_says_so() {
        // Same twenty Forests, but a format that does not exempt basics rejects them.
        let strict = Format {
            starting_life: 20,
            starting_hand_size: 7,
            seats: 2..=2,
            deck_rules: DeckRules {
                min_size: 40,
                max_size: None,
                max_copies: 4,
                basic_land_exempt: false,
            },
        };
        assert_eq!(
            strict.validate_deck(&legal_deck(), &db()),
            Err(DeckError::CopyLimit {
                card: fixture("forest"),
                count: 20,
                limit: 4,
            }),
        );
    }

    #[test]
    fn a_deck_over_the_maximum_size_is_rejected() {
        let capped = Format {
            starting_life: 20,
            starting_hand_size: 7,
            seats: 2..=2,
            deck_rules: DeckRules {
                min_size: 40,
                max_size: Some(60),
                max_copies: 4,
                basic_land_exempt: true,
            },
        };
        let big = vec![fixture("forest"); 61];
        assert_eq!(
            capped.validate_deck(&big, &db()),
            Err(DeckError::AboveMaximum { have: 61, max: 60 }),
        );
    }
}
