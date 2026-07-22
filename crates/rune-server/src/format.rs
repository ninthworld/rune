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

use std::collections::{HashMap, HashSet};

use rune_engine::{
    abilities_of, parse_mana_cost, Ability, CardDatabase, CardId, CardType, Color, Effect,
    GameSetup, PlayerSetup, Supertype,
};
use rune_protocol::GameSetupId;

/// The life total each player begins a **commander** game with (CR 903.7): 40.
/// This is engine *setup data* the server drives, not a rule the engine knows —
/// it flows through [`GameSetup::starting_life`] like any other format's life
/// total, so the engine stays free of format policy (ADR 0013 §4).
pub(crate) const COMMANDER_STARTING_LIFE: i32 = 40;

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
                require_commander: false,
                enforce_color_identity: false,
            },
        }
    }

    /// The **commander** format (`commander`, issue #372): a 100-card singleton
    /// deck (exactly 100 cards, at most one copy of any non-basic, basics exempt),
    /// a required commander that must be a legendary creature (CR 903.3/903.5a),
    /// color-identity containment (CR 903.4), 40 starting life (CR 903.7), seating
    /// 2–4. Deck legality is server policy (ADR 0013 §4); the engine only receives
    /// the designated commander in setup and the 40-life `GameSetup`.
    fn commander() -> Self {
        Self {
            // CR 903.7: each player begins with 40 life. The engine is told the
            // starting life through `GameSetup`; 40 is setup data, not a rule the
            // engine knows about (ADR 0013 §4).
            starting_life: COMMANDER_STARTING_LIFE,
            starting_hand_size: rune_engine::DEFAULT_STARTING_HAND_SIZE,
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
                require_commander: false,
                enforce_color_identity: false,
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
    /// database by the caller; `commander` is the seat's designated commander (CR
    /// 903.3), or `None` if it designated none. Validation is server policy only
    /// (ADR 0013 §4): it checks deck size; the per-oracle copy limit (basics exempt
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

/// A card's **color identity** (CR 903.4), computed from **structured** card data
/// only — never from generated display text (issue #372). Three contributors, all
/// read through the engine's typed [`CardData`]:
///
/// 1. the card's color indicator / printed colors (CR 105.2), `CardData::colors`;
/// 2. the colored mana symbols in its mana cost (`CardData::mana_cost` pips); and
/// 3. the colored mana symbols in its **rules**, taken from the ability IR: every
///    [`Effect::AddMana`] is a `{color}` mana symbol printed in a rules ability
///    (e.g. a land's `{T}: Add {G}`), which is exactly what gives a basic Forest
///    its green identity.
///
/// Colorless mana ([`Effect::AddColorlessMana`]) contributes nothing — colorless is
/// not a color (CR 105.1) — so an artifact that taps for `{C}` stays identity-empty
/// and is legal under any commander.
fn color_identity(db: &CardDatabase, card: CardId) -> HashSet<Color> {
    let mut identity = HashSet::new();
    let Some(data) = db.card(card) else {
        return identity;
    };
    // 1. Color indicator / printed colors.
    identity.extend(data.colors.iter().copied());
    // 2. Colored mana-cost pips.
    let cost = parse_mana_cost(&data.mana_cost);
    for (count, color) in [
        (cost.white, Color::White),
        (cost.blue, Color::Blue),
        (cost.black, Color::Black),
        (cost.red, Color::Red),
        (cost.green, Color::Green),
    ] {
        if count > 0 {
            identity.insert(color);
        }
    }
    // 3. Colored mana symbols in the card's rules (its abilities), from the IR.
    for ability in abilities_of(db, card) {
        if let Ability::Activated { effects, .. } | Ability::Triggered { effects, .. } = ability {
            for effect in effects {
                if let Effect::AddMana { color, .. } = effect {
                    identity.insert(color);
                }
            }
        }
    }
    // A spell ability that itself mints colored mana counts too (CR 903.4).
    for effect in &data.spell_effects {
        if let Effect::AddMana { color, .. } = effect {
            identity.insert(*color);
        }
    }
    identity
}

/// Whether `card` is a **legendary creature** — carries both the structured
/// [`Supertype::Legendary`] and the [`CardType::Creature`] type — read through
/// `db` (CR 903.5a, the default commander eligibility). An unknown id is not one;
/// the lobby rejects unknown ids before legality is checked.
fn is_legendary_creature(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|data| {
        data.supertypes.contains(&Supertype::Legendary) && data.has_type(CardType::Creature)
    })
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
    /// A commander format requires a designated commander (CR 903.3) and the deck
    /// designated none.
    MissingCommander,
    /// The designated commander is not among the submitted deck's cards (CR 903.3).
    CommanderNotInDeck {
        /// The designated card the decklist does not contain.
        card: CardId,
    },
    /// The designated commander is not a legendary creature (CR 903.5a).
    CommanderNotLegendaryCreature {
        /// The designated card that is not a legendary creature.
        card: CardId,
    },
    /// A card's color identity is not contained in the commander's (CR 903.4).
    OutOfIdentity {
        /// The first card (in deck order) outside the commander's color identity.
        card: CardId,
    },
}

impl DeckError {
    /// The offending [`CardId`] this rejection is about, if any. Size and
    /// missing-commander rejections name no specific card and return `None`; every
    /// card-specific variant returns the card at fault.
    pub(crate) fn card(&self) -> Option<CardId> {
        match self {
            Self::BelowMinimum { .. } | Self::AboveMaximum { .. } | Self::MissingCommander => None,
            Self::CopyLimit { card, .. }
            | Self::CommanderNotInDeck { card }
            | Self::CommanderNotLegendaryCreature { card }
            | Self::OutOfIdentity { card } => Some(*card),
        }
    }

    /// A stable `snake_case` machine code for this rejection class, mirrored on the
    /// wire in [`LobbyRejection::code`] so a client can branch without parsing the
    /// human-readable reason (issue #395).
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::BelowMinimum { .. } => "below_minimum",
            Self::AboveMaximum { .. } => "above_maximum",
            Self::CopyLimit { .. } => "copy_limit",
            Self::MissingCommander => "missing_commander",
            Self::CommanderNotInDeck { .. } => "commander_not_in_deck",
            Self::CommanderNotLegendaryCreature { .. } => "commander_not_legendary_creature",
            Self::OutOfIdentity { .. } => "out_of_identity",
        }
    }

    /// Render this rejection into its human-readable sentence, naming any offending
    /// card through `card_label`. This is the single source of the wording: the
    /// [`Display`](std::fmt::Display) impl labels a card by its raw [`CardId`] (for
    /// logs), while [`to_rejection`](DeckError::to_rejection) labels it by the card's
    /// display name (for the player). No new sentence templates are added anywhere —
    /// both callers reuse these (issue #395).
    fn render(&self, card_label: impl Fn(CardId) -> String) -> String {
        match self {
            Self::BelowMinimum { have, min } => {
                format!("deck has {have} cards, below the {min}-card minimum")
            }
            Self::AboveMaximum { have, max } => {
                format!("deck has {have} cards, above the {max}-card maximum")
            }
            Self::CopyLimit { card, count, limit } => format!(
                "{} appears {count} times, above the {limit}-copy limit",
                card_label(*card)
            ),
            Self::MissingCommander => "this format requires a designated commander".to_string(),
            Self::CommanderNotInDeck { card } => format!(
                "the designated commander ({}) is not in the deck",
                card_label(*card)
            ),
            Self::CommanderNotLegendaryCreature { card } => format!(
                "the designated commander ({}) is not a legendary creature",
                card_label(*card)
            ),
            Self::OutOfIdentity { card } => format!(
                "{} is outside the commander's color identity",
                card_label(*card)
            ),
        }
    }

    /// Project this rejection into the wire [`LobbyRejection`] delivered to the
    /// rejecting seat only (issue #395), resolving the offending [`CardId`] through
    /// `db` to name it by its display name in the reason and to carry its stable
    /// [`CardIdentity`] (`functional_id`). The reason reuses the same wording the
    /// server logs (see [`render`](DeckError::render)); nothing new is invented. The
    /// named card is always one of the sender's own submitted cards, so no other
    /// seat's hidden state leaks.
    pub(crate) fn to_rejection(&self, db: &CardDatabase) -> rune_protocol::LobbyRejection {
        let reason = self.render(|card| card_display_name(db, card));
        rune_protocol::LobbyRejection {
            code: self.code().to_string(),
            reason,
            card: self
                .card()
                .and_then(|card| db.card(card))
                .map(|data| data.functional_id.as_str().to_string()),
        }
    }
}

/// A card's display name for a player-facing message, read through `db`. Falls back
/// to the raw [`CardId`] label only if the id does not resolve (the lobby rejects
/// unknown ids before deck legality, so this is a defensive default).
fn card_display_name(db: &CardDatabase, card: CardId) -> String {
    db.card(card)
        .map(|data| data.name.clone())
        .unwrap_or_else(|| format!("card {}", card.0))
}

impl std::fmt::Display for DeckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render(|card| format!("card {}", card.0)))
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

    /// The identifier of the commander format (issue #372): 100-card singleton,
    /// color identity, 40 life, seats 2–4.
    const COMMANDER_ID: &'static str = "commander";

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
        // The commander format (issue #372): 100-card singleton with color-identity
        // containment, a required legendary-creature commander, 40 starting life, and
        // 2–4 seats. Deck legality is enforced entirely here (ADR 0013 §4).
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
        assert_eq!(format.validate_deck(&legal_deck(), None, &db()), Ok(()));
    }

    #[test]
    fn a_deck_under_the_minimum_size_is_rejected() {
        let format = Format::starter();
        let small = vec![fixture("forest"); 39];
        assert_eq!(
            format.validate_deck(&small, None, &db()),
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
            Format::starter().validate_deck(&deck, None, &db()),
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
                require_commander: false,
                enforce_color_identity: false,
            },
        };
        assert_eq!(
            strict.validate_deck(&legal_deck(), None, &db()),
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
                require_commander: false,
                enforce_color_identity: false,
            },
        };
        let big = vec![fixture("forest"); 61];
        assert_eq!(
            capped.validate_deck(&big, None, &db()),
            Err(DeckError::AboveMaximum { have: 61, max: 60 }),
        );
    }

    // ----------------------------------------------------------------------
    // Commander format (issue #372): 100-card singleton, color identity, 40 life.
    // ----------------------------------------------------------------------

    /// A legal 100-card mono-green commander deck for the bundled catalog: Jedit
    /// Ojanen (a green legendary creature) as the commander, the catalog's unique
    /// green (and colorless) non-basics, and Forests to fill to 100. Every card is
    /// within Jedit's green color identity, and every non-basic is a singleton — so
    /// this is the acceptance-path deck the rejection tests each perturb one way.
    fn commander_deck() -> Vec<CardId> {
        // Jedit Ojanen (the commander) plus the catalog's other in-identity
        // non-basics: mono-green cards and the colorless Skyscanner (empty identity ⊆
        // green). Each appears exactly once (singleton, CR 903.5b).
        let non_basics = [
            "jedit_ojanen",
            "llanowar_elves",
            "druid_of_the_cowl",
            "giant_spider",
            "colossal_dreadmaw",
            "gigantosaurus",
            "titanic_growth",
            "skyscanner",
        ];
        let mut deck: Vec<CardId> = non_basics.iter().map(|slug| fixture(slug)).collect();
        // Fill to exactly 100 with basic Forests (singleton-exempt, in-identity).
        while deck.len() < 100 {
            deck.push(fixture("forest"));
        }
        assert_eq!(deck.len(), 100);
        deck
    }

    /// The commander (Jedit Ojanen) of [`commander_deck`].
    fn commander() -> CardId {
        fixture("jedit_ojanen")
    }

    #[test]
    fn issue_372_a_legal_commander_deck_is_accepted() {
        // The acceptance path: exactly 100 cards, singleton non-basics, a legendary
        // creature commander, every card within its color identity.
        assert_eq!(
            Format::commander().validate_deck(&commander_deck(), Some(commander()), &db()),
            Ok(())
        );
    }

    #[test]
    fn issue_372_commander_format_starts_at_forty_life_and_seats_two_to_four() {
        let commander = FormatRegistry::with_defaults()
            .get("commander")
            .expect("commander format is registered")
            .clone();
        assert_eq!(commander.starting_life, COMMANDER_STARTING_LIFE);
        assert_eq!(commander.starting_life, 40);
        assert_eq!(commander.seats, 2..=4);
        // The engine `GameSetup` it builds carries the 40-life total.
        let setup = commander.game_setup(vec![PlayerSetup::new(commander_deck())], 1);
        assert_eq!(setup.starting_life, 40);
    }

    #[test]
    fn issue_372_a_missing_commander_is_rejected() {
        assert_eq!(
            Format::commander().validate_deck(&commander_deck(), None, &db()),
            Err(DeckError::MissingCommander)
        );
    }

    #[test]
    fn issue_372_a_non_legendary_creature_commander_is_rejected() {
        // Llanowar Elves is a green creature but not legendary, so it cannot be the
        // commander (CR 903.5a). It is already one of the deck's cards.
        let not_legendary = fixture("llanowar_elves");
        assert_eq!(
            Format::commander().validate_deck(&commander_deck(), Some(not_legendary), &db()),
            Err(DeckError::CommanderNotLegendaryCreature {
                card: not_legendary
            })
        );
    }

    #[test]
    fn issue_372_a_commander_not_in_the_deck_is_rejected() {
        // Designate a legendary creature the deck does not contain. Build a 100-card
        // deck of Forests only (so the designation, not size, is what is wrong).
        let deck = vec![fixture("forest"); 100];
        assert_eq!(
            Format::commander().validate_deck(&deck, Some(commander()), &db()),
            Err(DeckError::CommanderNotInDeck { card: commander() })
        );
    }

    #[test]
    fn issue_372_a_duplicate_non_basic_is_rejected() {
        // Two copies of a non-basic breaks the singleton limit (CR 903.5b). Drop one
        // Forest and add a second Llanowar Elves so the deck is still 100 cards.
        let mut deck = commander_deck();
        let forest_pos = deck
            .iter()
            .rposition(|&c| c == fixture("forest"))
            .expect("deck has forests");
        deck[forest_pos] = fixture("llanowar_elves");
        assert_eq!(deck.len(), 100);
        assert_eq!(
            Format::commander().validate_deck(&deck, Some(commander()), &db()),
            Err(DeckError::CopyLimit {
                card: fixture("llanowar_elves"),
                count: 2,
                limit: 1,
            })
        );
    }

    #[test]
    fn issue_372_an_out_of_identity_card_is_rejected() {
        // Swap a Forest for a blue card (Snapping Drake): its blue color identity is
        // not contained in the commander's green identity (CR 903.4). Deck stays 100.
        let mut deck = commander_deck();
        let forest_pos = deck
            .iter()
            .rposition(|&c| c == fixture("forest"))
            .expect("deck has forests");
        deck[forest_pos] = fixture("snapping_drake");
        assert_eq!(deck.len(), 100);
        assert_eq!(
            Format::commander().validate_deck(&deck, Some(commander()), &db()),
            Err(DeckError::OutOfIdentity {
                card: fixture("snapping_drake"),
            })
        );
    }

    #[test]
    fn issue_372_a_wrong_size_commander_deck_is_rejected() {
        // 99 cards is below the exact-100 requirement (a closed size range).
        let mut deck = commander_deck();
        deck.pop();
        assert_eq!(deck.len(), 99);
        assert_eq!(
            Format::commander().validate_deck(&deck, Some(commander()), &db()),
            Err(DeckError::BelowMinimum { have: 99, min: 100 })
        );
        // 101 cards is above it.
        let mut over = commander_deck();
        over.push(fixture("forest"));
        assert_eq!(
            Format::commander().validate_deck(&over, Some(commander()), &db()),
            Err(DeckError::AboveMaximum {
                have: 101,
                max: 100
            })
        );
    }

    #[test]
    fn issue_372_a_forest_is_green_identity_and_colorless_is_identityless() {
        // Color identity is computed from structured data: a basic Forest is green
        // (its intrinsic `{T}: Add {G}` ability, CR 903.4), while a colorless artifact
        // that taps for {C} has empty identity (colorless is not a color, CR 105.1).
        let database = db();
        let forest = color_identity(&database, fixture("forest"));
        assert!(forest.contains(&Color::Green) && forest.len() == 1);
        let skyscanner = color_identity(&database, fixture("skyscanner"));
        assert!(skyscanner.is_empty());
        // Jedit Ojanen is green (its colored mana-cost pips).
        assert_eq!(
            color_identity(&database, commander()),
            HashSet::from([Color::Green])
        );
    }

    #[test]
    fn issue_372_jedit_ojanen_is_a_legendary_creature() {
        // The commander eligibility predicate reads structured supertype + type.
        assert!(is_legendary_creature(&db(), commander()));
        // A non-legendary creature and a legendary-less card are both ineligible.
        assert!(!is_legendary_creature(&db(), fixture("llanowar_elves")));
        assert!(!is_legendary_creature(&db(), fixture("forest")));
    }

    // ----------------------------------------------------------------------
    // Structured rejection reasons reaching the wire (issue #395).
    // ----------------------------------------------------------------------

    /// The display name the bundled database gives `slug`, used to assert a rejection
    /// reason names the offending card by name rather than a raw interned id.
    fn name_of(slug: &str) -> String {
        db().card(fixture(slug)).expect("bundled card").name.clone()
    }

    #[test]
    fn issue_395_size_rejection_names_no_card_and_carries_a_code() {
        // A below-minimum rejection is not about any one card: the wire reason has the
        // stable class code and the human sentence, and no `card`.
        let rejection = DeckError::BelowMinimum { have: 39, min: 40 }.to_rejection(&db());
        assert_eq!(rejection.code, "below_minimum");
        assert_eq!(rejection.card, None);
        assert_eq!(
            rejection.reason,
            "deck has 39 cards, below the 40-card minimum"
        );

        let over = DeckError::AboveMaximum {
            have: 101,
            max: 100,
        }
        .to_rejection(&db());
        assert_eq!(over.code, "above_maximum");
        assert_eq!(over.card, None);
    }

    #[test]
    fn issue_395_copy_limit_rejection_names_the_offending_card_by_name_and_identity() {
        // The wire reason names the card by its display name (never the raw CardId),
        // and `card` carries its stable functional_id — both drawn from the sender's
        // own submission.
        let rejection = DeckError::CopyLimit {
            card: fixture("onakke_ogre"),
            count: 5,
            limit: 4,
        }
        .to_rejection(&db());
        assert_eq!(rejection.code, "copy_limit");
        assert_eq!(rejection.card.as_deref(), Some("onakke_ogre"));
        assert_eq!(
            rejection.reason,
            format!(
                "{} appears 5 times, above the 4-copy limit",
                name_of("onakke_ogre")
            )
        );
        // Never the internal interned id.
        assert!(!rejection
            .reason
            .contains(&format!("card {}", fixture("onakke_ogre").0)));
    }

    #[test]
    fn issue_395_commander_rejections_carry_class_codes_and_the_named_card() {
        let db = db();
        // Missing commander: a required designation, about no card.
        let missing = DeckError::MissingCommander.to_rejection(&db);
        assert_eq!(missing.code, "missing_commander");
        assert_eq!(missing.card, None);

        // Not a legendary creature: names the illegal designation (Llanowar Elves).
        let not_legendary = DeckError::CommanderNotLegendaryCreature {
            card: fixture("llanowar_elves"),
        }
        .to_rejection(&db);
        assert_eq!(not_legendary.code, "commander_not_legendary_creature");
        assert_eq!(not_legendary.card.as_deref(), Some("llanowar_elves"));
        assert!(not_legendary.reason.contains(&name_of("llanowar_elves")));

        // Not in the deck: names the designated commander (Jedit Ojanen).
        let not_in_deck = DeckError::CommanderNotInDeck {
            card: fixture("jedit_ojanen"),
        }
        .to_rejection(&db);
        assert_eq!(not_in_deck.code, "commander_not_in_deck");
        assert_eq!(not_in_deck.card.as_deref(), Some("jedit_ojanen"));

        // Out of identity: names the offending deck card (Snapping Drake).
        let out = DeckError::OutOfIdentity {
            card: fixture("snapping_drake"),
        }
        .to_rejection(&db);
        assert_eq!(out.code, "out_of_identity");
        assert_eq!(out.card.as_deref(), Some("snapping_drake"));
        assert!(out.reason.contains(&name_of("snapping_drake")));
    }

    #[test]
    fn issue_395_display_for_logs_is_unchanged_and_uses_the_raw_id() {
        // The `Display` impl (used only for server logs) still labels a card by its raw
        // CardId, so the wire naming refactor did not disturb the log wording.
        let display = DeckError::CopyLimit {
            card: fixture("onakke_ogre"),
            count: 5,
            limit: 4,
        }
        .to_string();
        assert_eq!(
            display,
            format!(
                "card {} appears 5 times, above the 4-copy limit",
                fixture("onakke_ogre").0
            )
        );
        assert_eq!(
            DeckError::BelowMinimum { have: 39, min: 40 }.to_string(),
            "deck has 39 cards, below the 40-card minimum"
        );
    }

    #[test]
    fn issue_372_existing_formats_keep_default_life_and_no_commander_rules() {
        // Regression guard: the non-commander formats are unchanged — 20 life, no
        // commander requirement, no color-identity enforcement.
        for id in ["starter-1v1", "standard_2p", "1v1", "standard_ffa", "ffa-4"] {
            let format = FormatRegistry::with_defaults().get(id).unwrap().clone();
            assert_eq!(format.starting_life, rune_engine::DEFAULT_STARTING_LIFE);
            assert!(!format.deck_rules.require_commander);
            assert!(!format.deck_rules.enforce_color_identity);
        }
    }
}
