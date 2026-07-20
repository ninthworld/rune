//! Building an initial [`GameState`] from real decklists (CR 103, "Starting the
//! Game").
//!
//! [`GameSetup`] is pure configuration — per-player decklists as card-database
//! ids plus the starting life total and hand size — and [`GameState::new`] turns
//! it into a ready-to-play state: instances minted for every card, each library
//! shuffled deterministically from the injected seed (CR 103.3), and opening
//! hands drawn (CR 103.5). The resulting state opens in the London mulligan
//! decision phase (CR 103.5, [`crate::mulligan`]) — turn 1 does not begin until
//! every player has kept. No I/O and no OS entropy is involved; the only source
//! of randomness is the seed carried in [`GameSetup::rng_seed`], consumed through
//! [`crate::rng`].
//!
//! Out of scope here: deck-legality checks (deck size, singleton/limit rules) and
//! the set/printing model (ADR 0013). Unknown card ids are the one input error
//! this constructor rejects.

use std::fmt;

use crate::id::{CardId, PlayerId};
use crate::mulligan::MulliganState;
use crate::phase::Step;
use crate::player::{Player, STARTING_LIFE};
use crate::rng::SplitMix64;
use crate::state::GameState;

/// Default starting life total, 20 (CR 103.4 / CR 119.1).
pub const DEFAULT_STARTING_LIFE: i32 = STARTING_LIFE;

/// Default opening-hand size, seven cards (CR 103.5).
pub const DEFAULT_STARTING_HAND_SIZE: usize = 7;

/// One player's contribution to a [`GameSetup`]: the deck they bring, listed as
/// card-database ids.
///
/// The list is the full, already-expanded decklist (one entry per physical card,
/// so four copies of a card appear four times), in any order — it is shuffled
/// during construction. Deck legality (size, copy limits) is not checked here;
/// that is a separate concern (out of scope for issue #109).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSetup {
    /// The player's deck as card-database ids, one entry per card.
    pub decklist: Vec<CardId>,
    /// The card this player designates as their commander (CR 903.3), or `None`
    /// for a player without one. It must be one of the [`decklist`](Self::decklist)
    /// entries; at setup that physical copy is placed in the command zone (CR
    /// 903.6) and the *rest* of the deck is shuffled into the library. Which
    /// physical instance becomes the commander is chosen here by setup — the first
    /// decklist entry matching this card. Deck legality (singleton, color
    /// identity) is not checked; that stays server-side (ADR 0013 §4).
    pub commander: Option<CardId>,
}

impl PlayerSetup {
    /// A setup for a player bringing `decklist` (one [`CardId`] per physical card)
    /// and **no** commander — behaves exactly as before commanders existed.
    #[must_use]
    pub fn new(decklist: Vec<CardId>) -> Self {
        Self {
            decklist,
            commander: None,
        }
    }

    /// A setup for a player bringing `decklist` and designating `commander` (which
    /// must be one of the decklist entries, CR 903.3). At construction the matching
    /// physical copy is set aside into the command zone.
    #[must_use]
    pub fn with_commander(decklist: Vec<CardId>, commander: CardId) -> Self {
        Self {
            decklist,
            commander: Some(commander),
        }
    }
}

/// A pure, replayable description of a game about to start: who is playing with
/// which deck, the starting life and hand size, and the seed that fixes every
/// shuffle.
///
/// Feeding the same `GameSetup` (identical decklists, life, hand size, and
/// [`rng_seed`](Self::rng_seed)) to [`GameState::new`] always yields the exact
/// same initial [`GameState`], which is what makes games replayable from setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameSetup {
    /// Each seat's deck, in seating (turn) order. Seat `n` becomes
    /// [`PlayerId(n)`](crate::PlayerId).
    pub players: Vec<PlayerSetup>,
    /// Life total every player starts at (CR 103.4). Defaults to
    /// [`DEFAULT_STARTING_LIFE`].
    pub starting_life: i32,
    /// Number of cards in each opening hand (CR 103.5). Defaults to
    /// [`DEFAULT_STARTING_HAND_SIZE`].
    pub starting_hand_size: usize,
    /// Seed for the deterministic library shuffle (CR 103.3). The same seed and
    /// decklists produce identical library orders; different seeds differ. This
    /// is the sole source of randomness — no OS entropy, no wall-clock. It is
    /// stored into [`GameState::rng_seed`](crate::GameState::rng_seed) (advanced
    /// past the setup shuffles) so the rest of the game keeps drawing from the
    /// same stream.
    pub rng_seed: u64,
}

impl GameSetup {
    /// A setup with the CR-default starting life (20) and hand size (7), seeded
    /// with `rng_seed`. Seats are taken from `players` in order.
    #[must_use]
    pub fn new(players: Vec<PlayerSetup>, rng_seed: u64) -> Self {
        Self {
            players,
            starting_life: DEFAULT_STARTING_LIFE,
            starting_hand_size: DEFAULT_STARTING_HAND_SIZE,
            rng_seed,
        }
    }

    /// Convenience for the common two-player game: `deck0` versus `deck1` with CR
    /// defaults, seeded with `rng_seed`.
    #[must_use]
    pub fn two_player(deck0: Vec<CardId>, deck1: Vec<CardId>, rng_seed: u64) -> Self {
        Self::new(
            vec![PlayerSetup::new(deck0), PlayerSetup::new(deck1)],
            rng_seed,
        )
    }
}

/// Why building a [`GameState`] from a [`GameSetup`] failed.
///
/// Deliberately narrow: this constructor validates only that every card id
/// resolves in the database. Deck legality and player-count rules are enforced
/// elsewhere (out of scope for issue #109).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupError {
    /// A decklist referenced a card id absent from the [`CardDatabase`]. Carries
    /// the offending seat and id so the caller can report which deck is at fault.
    ///
    /// [`CardDatabase`]: crate::CardDatabase
    UnknownCard {
        /// The seat whose decklist held the bad id.
        player: PlayerId,
        /// The card id that did not resolve.
        card: CardId,
    },
    /// A player designated a commander (CR 903.3) that is not one of their
    /// decklist entries. Carries the offending seat and the designated card so
    /// the caller can report which deck is at fault.
    CommanderNotInDeck {
        /// The seat whose commander is not in their deck.
        player: PlayerId,
        /// The designated commander card that the decklist does not contain.
        card: CardId,
    },
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCard { player, card } => write!(
                f,
                "player {}'s decklist references unknown card id {}",
                player.0, card.0
            ),
            Self::CommanderNotInDeck { player, card } => write!(
                f,
                "player {}'s designated commander (card id {}) is not in their deck",
                player.0, card.0
            ),
        }
    }
}

impl std::error::Error for SetupError {}

impl GameState {
    /// Build an initial [`GameState`] from `setup` and the card database `db`
    /// (CR 103, "Starting the Game").
    ///
    /// The result is a pure function of its inputs: for every seat an instance is
    /// minted per decklist entry into the library, each library is shuffled
    /// deterministically from [`GameSetup::rng_seed`] (CR 103.3), and the top
    /// [`GameSetup::starting_hand_size`] cards become that player's opening hand
    /// (CR 103.5). The state then opens in the London mulligan phase
    /// ([`GameState::mulligan`](crate::GameState::mulligan) is `Some`), with seat 0
    /// deciding first; turn 1 (seat 0 at [`Step::Untap`]) begins only once every
    /// player has kept. Libraries populate [`Player::library`] so their counts flow
    /// through `GameView` unchanged. The seed is stored back into
    /// [`GameState::rng_seed`](crate::GameState::rng_seed) advanced past the setup
    /// shuffles, so later randomness (including mulligan reshuffles) continues the
    /// stream.
    ///
    /// Shuffling consumes the seeded stream one seat at a time in seating order,
    /// so a seat's library order depends on both the seed and the seats before it
    /// — replay-stable, never on wall-clock or OS entropy.
    ///
    /// # Errors
    /// Returns [`SetupError::UnknownCard`] if any decklist references a card id
    /// not present in `db`. No state is built when validation fails.
    pub fn new(setup: &GameSetup, db: &crate::CardDatabase) -> Result<Self, SetupError> {
        // Validate every id up front so a bad decklist yields an error, not a
        // half-built state.
        for (seat, player) in setup.players.iter().enumerate() {
            for &card in &player.decklist {
                if db.card(card).is_none() {
                    return Err(SetupError::UnknownCard {
                        player: PlayerId(seat),
                        card,
                    });
                }
            }
            // A designated commander must be one of the deck's cards (CR 903.3).
            // Its id already resolved above (it is a decklist entry), so this is
            // the only extra validation the command zone introduces.
            if let Some(commander) = player.commander {
                if !player.decklist.contains(&commander) {
                    return Err(SetupError::CommanderNotInDeck {
                        player: PlayerId(seat),
                        card: commander,
                    });
                }
            }
        }

        let mut state = Self {
            turn: 1,
            active_player: PlayerId(0),
            priority: PlayerId(0),
            consecutive_passes: 0,
            step: Step::Untap,
            players: Vec::with_capacity(setup.players.len()),
            battlefield: Vec::new(),
            stack: Vec::new(),
            static_effects: Vec::new(),
            next_object_id: 1,
            land_played: false,
            attackers_declared: false,
            blockers_declared: false,
            damage_orders: Vec::new(),
            blockers_declared_by: Vec::new(),
            deathtouch_struck: Vec::new(),
            commander_damage: Vec::new(),
            extra_turns: Vec::new(),
            extra_steps: Vec::new(),
            rng_seed: setup.rng_seed,
            // Populated below once seat count is known: the game opens in the
            // London mulligan decision phase (CR 103.5), not on turn 1.
            mulligan: None,
            log: Vec::new(),
            next_log_sequence: 1,
        };

        let mut rng = SplitMix64::new(setup.rng_seed);
        for player_setup in &setup.players {
            let mut player = Player {
                life: setup.starting_life,
                ..Player::default()
            };
            // Mint a distinct instance per decklist entry. A designated commander's
            // matching copy is set aside into the command zone (CR 903.6) instead of
            // the library, and the designation is recorded (CR 903.3); every other
            // card goes to the library, which is then shuffled (CR 103.3) and the
            // opening hand drawn off the top (CR 103.5). With no commander this is
            // byte-for-byte the pre-commander behavior.
            let mut commander_taken = false;
            for &card in &player_setup.decklist {
                let instance = state.new_instance(card);
                if !commander_taken && player_setup.commander == Some(card) {
                    commander_taken = true;
                    player.commander =
                        Some(crate::commander::CommanderState::new(card, instance.id));
                    player.command.push(instance);
                } else {
                    player.library.push(instance);
                }
            }
            rng.shuffle(&mut player.library);
            let draw = setup.starting_hand_size.min(player.library.len());
            for _ in 0..draw {
                if let Some(card) = player.library.pop() {
                    player.hand.push(card);
                }
            }
            state.players.push(player);
        }
        // Persist the advanced generator state so subsequent randomness (future
        // in-game shuffles, etc.) continues the same deterministic stream.
        state.rng_seed = rng.state();

        // Enter the London mulligan decision phase (CR 103.5): turn 1 does not
        // begin until every player has kept. Seat 0 decides first; priority is
        // already seated there.
        state.mulligan = Some(MulliganState::new(
            state.players.len(),
            setup.starting_hand_size,
        ));

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::card::CardDatabase;
    use crate::fixtures::fixture;

    /// The bundled database used across these tests.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A 40-card decklist cycling through six bundled cards.
    ///
    /// Named by authored identity, not by handle: a `CardId` is interned from the
    /// catalog's sort order (ADR 0018 §3), so `CardId(1)` means a different card the
    /// moment one is authored ahead of it.
    fn sample_decklist() -> Vec<CardId> {
        const CARDS: [&str; 6] = [
            "onakke_ogre",
            "fire_elemental",
            "snapping_drake",
            "giant_spider",
            "forest",
            "llanowar_elves",
        ];
        (0..40).map(|i| fixture(CARDS[i % 6])).collect()
    }

    #[test]
    fn cr_103_builds_shuffled_libraries_and_opening_hands() {
        // CR 103.5: each player draws a hand of the starting size; CR 103.3: the
        // remaining deck forms the (shuffled) library.
        let setup = GameSetup::two_player(sample_decklist(), sample_decklist(), 42);
        let state = GameState::new(&setup, &db()).unwrap();

        assert_eq!(state.turn, 1);
        assert_eq!(state.active_player, PlayerId(0));
        assert_eq!(state.step, Step::Untap);
        assert_eq!(state.players.len(), 2);

        for player in &state.players {
            assert_eq!(player.life, DEFAULT_STARTING_LIFE);
            assert_eq!(player.hand.len(), DEFAULT_STARTING_HAND_SIZE);
            // 40 cards minus the seven drawn (CR 103.5).
            assert_eq!(player.library.len(), 40 - DEFAULT_STARTING_HAND_SIZE);
            assert!(player.graveyard.is_empty());
            assert!(player.exile.is_empty());
        }
    }

    #[test]
    fn every_instance_id_is_unique_across_seats() {
        // Instances are minted from the shared monotonic counter, so no two
        // physical cards — even across libraries and hands — share an id.
        let setup = GameSetup::two_player(sample_decklist(), sample_decklist(), 7);
        let state = GameState::new(&setup, &db()).unwrap();

        let mut ids: Vec<_> = state
            .players
            .iter()
            .flat_map(|p| p.library.iter().chain(p.hand.iter()).map(|c| c.id))
            .collect();
        let total = ids.len();
        assert_eq!(total, 80); // two 40-card decks
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    #[test]
    fn cr_103_3_same_seed_and_decklists_are_identical() {
        // Determinism: identical inputs (including seed) reproduce the game
        // exactly, library order included.
        let setup = GameSetup::two_player(sample_decklist(), sample_decklist(), 0xABCD);
        let a = GameState::new(&setup, &db()).unwrap();
        let b = GameState::new(&setup, &db()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn cr_103_3_different_seeds_shuffle_differently() {
        // Different seeds must produce different library orders (the shuffle is
        // genuinely seed-driven, not a fixed permutation).
        let deck = sample_decklist();
        let one =
            GameState::new(&GameSetup::two_player(deck.clone(), deck.clone(), 1), &db()).unwrap();
        let two =
            GameState::new(&GameSetup::two_player(deck.clone(), deck.clone(), 2), &db()).unwrap();
        let lib = |s: &GameState| -> Vec<CardId> {
            s.players[0].library.iter().map(|c| c.card).collect()
        };
        assert_ne!(lib(&one), lib(&two));
    }

    #[test]
    fn shuffle_preserves_the_multiset_of_cards() {
        // Shuffling reorders but neither adds, drops, nor duplicates cards: the
        // library-plus-hand multiset equals the decklist.
        let decklist = sample_decklist();
        let setup = GameSetup::two_player(decklist.clone(), sample_decklist(), 99);
        let state = GameState::new(&setup, &db()).unwrap();

        let mut got: Vec<u64> = state.players[0]
            .library
            .iter()
            .chain(state.players[0].hand.iter())
            .map(|c| c.card.0)
            .collect();
        got.sort_unstable();
        let mut want: Vec<u64> = decklist.iter().map(|c| c.0).collect();
        want.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn unknown_card_id_is_rejected() {
        // The one validation this constructor performs: unknown ids (CardId(9999)
        // is absent from the bundled database) fail with the offending seat/id.
        let bad = vec![fixture("onakke_ogre"), CardId(9999)];
        let setup = GameSetup::two_player(bad, sample_decklist(), 0);
        let err = GameState::new(&setup, &db()).unwrap_err();
        assert_eq!(
            err,
            SetupError::UnknownCard {
                player: PlayerId(0),
                card: CardId(9999),
            }
        );
    }

    #[test]
    fn seed_is_advanced_past_the_setup_shuffles() {
        // After consuming randomness the stored seed differs from the input seed,
        // so later draws continue the stream rather than replaying the shuffle.
        let setup = GameSetup::two_player(sample_decklist(), sample_decklist(), 12345);
        let state = GameState::new(&setup, &db()).unwrap();
        assert_ne!(state.rng_seed, 12345);
    }

    #[test]
    fn short_deck_draws_only_what_is_available() {
        // A deck smaller than the opening-hand size draws the whole library and
        // leaves it empty rather than erroring (deck-size legality is out of
        // scope; #111 handles mulligans).
        let setup = GameSetup::two_player(
            vec![fixture("onakke_ogre"), fixture("fire_elemental")],
            vec![fixture("onakke_ogre")],
            3,
        );
        let state = GameState::new(&setup, &db()).unwrap();
        assert_eq!(state.players[0].hand.len(), 2);
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn cr_903_6_designated_commander_starts_in_the_command_zone() {
        // CR 903.6: a designated commander begins the game in the command zone, and
        // the rest of the deck is shuffled into the library. Library size is the
        // deck minus the commander minus the opening hand.
        let db = db();
        let commander = fixture("llanowar_elves"); // appears in sample_decklist
        let setup = GameSetup::new(
            vec![
                PlayerSetup::with_commander(sample_decklist(), commander),
                PlayerSetup::new(sample_decklist()),
            ],
            77,
        );
        let state = GameState::new(&setup, &db).unwrap();

        // Seat 0's commander sits in the command zone, designated with no tax yet.
        assert_eq!(state.players[0].command.len(), 1);
        assert_eq!(state.players[0].command[0].card, commander);
        let designation = state.players[0].commander.unwrap();
        assert_eq!(designation.card, commander);
        assert_eq!(designation.instance, state.players[0].command[0].id);
        assert_eq!(designation.casts, 0);
        assert!(!designation.return_pending);

        // The library is the deck minus the commander minus the opening hand.
        assert_eq!(state.players[0].hand.len(), DEFAULT_STARTING_HAND_SIZE);
        assert_eq!(
            state.players[0].library.len(),
            40 - 1 - DEFAULT_STARTING_HAND_SIZE
        );
        // The commander instance is not in any other zone.
        let commander_instance = designation.instance;
        assert!(!state.players[0]
            .library
            .iter()
            .chain(&state.players[0].hand)
            .any(|c| c.id == commander_instance));

        // Seat 1 designated nothing: its command zone is empty and it has no
        // designation — exactly as before commanders existed.
        assert!(state.players[1].command.is_empty());
        assert!(state.players[1].commander.is_none());
        assert_eq!(
            state.players[1].library.len(),
            40 - DEFAULT_STARTING_HAND_SIZE
        );
    }

    #[test]
    fn issue_370_no_designation_is_identical_to_the_pre_commander_setup() {
        // A setup with no commander must behave exactly as today: every card in the
        // library/hand, empty command zones, no designation.
        let db = db();
        let setup = GameSetup::two_player(sample_decklist(), sample_decklist(), 5);
        let state = GameState::new(&setup, &db).unwrap();
        for player in &state.players {
            assert!(player.command.is_empty());
            assert!(player.commander.is_none());
            assert_eq!(player.library.len(), 40 - DEFAULT_STARTING_HAND_SIZE);
            assert_eq!(player.hand.len(), DEFAULT_STARTING_HAND_SIZE);
        }
    }

    #[test]
    fn commander_not_in_deck_is_rejected() {
        // A designated commander must be one of the deck's cards (CR 903.3).
        let db = db();
        // `island` is not one of the six cards `sample_decklist` cycles through.
        let absent = fixture("island");
        let setup = GameSetup::new(
            vec![PlayerSetup::with_commander(sample_decklist(), absent)],
            0,
        );
        let err = GameState::new(&setup, &db).unwrap_err();
        assert_eq!(
            err,
            SetupError::CommanderNotInDeck {
                player: PlayerId(0),
                card: absent,
            }
        );
    }

    #[test]
    fn custom_life_and_hand_size_are_honored() {
        // Non-default starting values flow straight through.
        let setup = GameSetup {
            players: vec![PlayerSetup::new(sample_decklist())],
            starting_life: 40,
            starting_hand_size: 5,
            rng_seed: 8,
        };
        let state = GameState::new(&setup, &db()).unwrap();
        assert_eq!(state.players[0].life, 40);
        assert_eq!(state.players[0].hand.len(), 5);
        assert_eq!(state.players[0].library.len(), 35);
    }
}
