//! Mana: colors, a per-color pool, and mana costs parsed from `{...}` notation.
//!
//! Pure data with pure operations — no I/O, no randomness. Costs are parsed from
//! the same curly-brace strings the card snapshot already stores in
//! [`crate::CardData::mana_cost`].

use serde::Deserialize;

/// One of the five colors of mana.
///
/// Deserialized from lowercase color names (`"green"`), matching the card
/// snapshot's ability data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    /// White (`{W}`).
    White,
    /// Blue (`{U}`).
    Blue,
    /// Black (`{B}`).
    Black,
    /// Red (`{R}`).
    Red,
    /// Green (`{G}`).
    Green,
}

impl Color {
    /// All five colors (CR 105.1), in the canonical WUBRG order.
    ///
    /// The answer set of every "choose a color" question and the set an effect naming
    /// no particular color ranges over, written once so the two can never disagree
    /// about how many colors there are.
    pub const ALL: [Self; 5] = [Self::White, Self::Blue, Self::Black, Self::Red, Self::Green];

    /// The pip string for this color, e.g. `"{G}"` for [`Color::Green`].
    #[must_use]
    pub fn pip(self) -> &'static str {
        match self {
            Self::White => "{W}",
            Self::Blue => "{U}",
            Self::Black => "{B}",
            Self::Red => "{R}",
            Self::Green => "{G}",
        }
    }

    /// This color as the adjective a rules sentence uses, e.g. `"black"` in "can't be
    /// blocked by black creatures". The prose counterpart of [`Self::pip`], which
    /// writes the same color as a cost symbol.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Blue => "blue",
            Self::Black => "black",
            Self::Red => "red",
            Self::Green => "green",
        }
    }
}

/// A piece of mana that may be spent only on certain things (CR 106.6) — `Add {R}{R}.
/// Spend this mana only to cast Dragon spells.`
///
/// Held apart from the plain per-color counts of [`ManaPool`] because the restriction
/// travels with the mana rather than with the pool: a player may float two restricted
/// red and three ordinary red at once, and only one of those five piles can pay for a
/// Bear. It is still mana in every other respect, and empties with the rest of the pool
/// at the end of the step (CR 500.4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RestrictedMana {
    /// The color of this mana.
    pub color: Color,
    /// How much of it is left.
    pub amount: u8,
    /// What it may be spent on.
    pub restriction: crate::ability::ManaRestriction,
}

/// What a payment is **for** — the question restricted mana has to ask before it can be
/// spent (CR 106.6).
///
/// Ordinary mana never asks, which is why this is a parameter of the payment rather than
/// a field on the cost: a [`ManaCost`] is a shape ("{1}{R}"), and the same shape is paid
/// by a cast, an activation, and an optional effect's cost, only one of which is casting
/// anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpendPurpose<'a> {
    /// Not casting a spell — an activation cost, or an optional effect's cost.
    /// Restricted mana is never spendable on one of these.
    Other,
    /// Casting a spell whose card has these printed subtypes.
    CastingSpell {
        /// The spell's printed subtypes, which is what a `Dragon spells` restriction
        /// reads.
        subtypes: &'a [String],
    },
}

impl crate::ability::ManaRestriction {
    /// Whether mana carrying this restriction may be spent for `purpose`.
    #[must_use]
    pub fn allows(&self, purpose: SpendPurpose<'_>) -> bool {
        match (self, purpose) {
            (
                crate::ability::ManaRestriction::SpellsWithSubtype { subtype },
                SpendPurpose::CastingSpell { subtypes },
            ) => subtypes.iter().any(|s| s == subtype),
            (crate::ability::ManaRestriction::SpellsWithSubtype { .. }, SpendPurpose::Other) => {
                false
            }
        }
    }
}

/// A quantity of mana held per color, plus colorless, plus any restricted mana.
///
/// This is a player's mana pool. It stores raw counts only; nothing is derived
/// or cached here.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct ManaPool {
    /// White mana available.
    pub white: u8,
    /// Blue mana available.
    pub blue: u8,
    /// Black mana available.
    pub black: u8,
    /// Red mana available.
    pub red: u8,
    /// Green mana available.
    pub green: u8,
    /// Colorless mana available.
    pub colorless: u8,
    /// Mana that may be spent only on certain things (CR 106.6) — see
    /// [`RestrictedMana`]. Empty in almost every pool, so a game with no restricted
    /// mana source is byte-for-byte unchanged.
    pub restricted: Vec<RestrictedMana>,
}

impl ManaPool {
    /// Add `amount` mana of `color` to the pool.
    pub fn add(&mut self, color: Color, amount: u8) {
        let slot = match color {
            Color::White => &mut self.white,
            Color::Blue => &mut self.blue,
            Color::Black => &mut self.black,
            Color::Red => &mut self.red,
            Color::Green => &mut self.green,
        };
        *slot = slot.saturating_add(amount);
    }

    /// Add `amount` colorless mana (`{C}`) to the pool — colorless is not one of the
    /// five [`Color`]s (CR 105.1), so it has its own adder rather than a slot in
    /// [`Self::add`].
    pub fn add_colorless(&mut self, amount: u8) {
        self.colorless = self.colorless.saturating_add(amount);
    }

    /// Add `amount` mana of `color` that may be spent only as `restriction` allows
    /// (CR 106.6). Mana of the same color under the same restriction pools together, so
    /// activating one ability twice leaves one entry of two rather than two of one.
    pub fn add_restricted(
        &mut self,
        color: Color,
        amount: u8,
        restriction: crate::ability::ManaRestriction,
    ) {
        match self
            .restricted
            .iter_mut()
            .find(|entry| entry.color == color && entry.restriction == restriction)
        {
            Some(entry) => entry.amount = entry.amount.saturating_add(amount),
            None => self.restricted.push(RestrictedMana {
                color,
                amount,
                restriction,
            }),
        }
    }

    /// Total mana of every color and colorless currently in the pool, **including**
    /// restricted mana — it is mana, and a display that omitted it would be lying about
    /// what a player is holding.
    #[must_use]
    pub fn total(&self) -> u16 {
        u16::from(self.white)
            + u16::from(self.blue)
            + u16::from(self.black)
            + u16::from(self.red)
            + u16::from(self.green)
            + u16::from(self.colorless)
            + self
                .restricted
                .iter()
                .map(|entry| u16::from(entry.amount))
                .sum::<u16>()
    }

    /// Whether `cost` can be paid from this pool with no restricted mana — the shape
    /// every payment that is not a cast asks for.
    #[must_use]
    pub fn can_pay(&self, cost: &ManaCost) -> bool {
        self.pay(cost).is_some()
    }

    /// Whether `cost` can be paid from this pool for `purpose`, which decides whether
    /// the restricted mana in it may be used (CR 106.6).
    #[must_use]
    pub fn can_pay_for(&self, cost: &ManaCost, purpose: SpendPurpose<'_>) -> bool {
        self.pay_for(cost, purpose).is_some()
    }

    /// Pay `cost` with no restricted mana, returning the resulting pool, or `None` if it
    /// cannot be paid. The convenience over [`Self::pay_for`] for every payment that is
    /// not casting a spell.
    #[must_use]
    pub fn pay(&self, cost: &ManaCost) -> Option<Self> {
        self.pay_for(cost, SpendPurpose::Other)
    }

    /// Pay `cost` for `purpose`, returning the resulting pool, or `None` if it cannot be
    /// paid.
    ///
    /// **Restricted mana is spent first**, and that greedy choice is provably optimal
    /// rather than merely convenient: mana that may only be spent here can pay for
    /// nothing else, so using it can never cost the player a payment they would
    /// otherwise have made. Colored and colorless requirements are paid from their own
    /// colors; the generic portion is then paid deterministically from usable restricted
    /// mana, then colorless, then white, blue, black, red, green.
    #[must_use]
    pub fn pay_for(&self, cost: &ManaCost, purpose: SpendPurpose<'_>) -> Option<Self> {
        let (paid, owed) = self.settle(cost, purpose);
        (owed == ManaCost::default()).then_some(paid)
    }

    /// What this cost **still needs** after this pool has paid as much of it as it can.
    ///
    /// The zero cost exactly when [`Self::pay_for`] succeeds, because they are the same
    /// computation — see [`Self::settle`]. That equality is the point of it existing: a
    /// presentation that asked one question and a legality gate that asked the other
    /// could otherwise disagree about what a player still owes, and the player would see
    /// a cost that does not match what the game will accept.
    #[must_use]
    pub fn remaining_cost(&self, cost: &ManaCost, purpose: SpendPurpose<'_>) -> ManaCost {
        self.settle(cost, purpose).1
    }

    /// Pay as much of `cost` as this pool can, returning what is left of the pool and
    /// what is left of the cost.
    ///
    /// **One computation behind two questions.** *Can this be paid* and *what is still
    /// owed* are the same walk over the same pool, so they are one function; asking them
    /// separately is how a "you still need {1}" that the game then refuses gets built.
    ///
    /// **Restricted mana is spent first**, and that greedy choice is provably optimal
    /// rather than merely convenient: mana that may only be spent here can pay for
    /// nothing else, so using it can never cost the player a payment they would
    /// otherwise have made. Colored and colorless requirements are paid from their own
    /// colors; the generic portion is then paid deterministically from usable restricted
    /// mana, then colorless, then white, blue, black, red, green.
    fn settle(&self, cost: &ManaCost, purpose: SpendPurpose<'_>) -> (Self, ManaCost) {
        let mut pool = self.clone();
        let mut owed = ManaCost::default();
        // Each colored requirement takes its own color, restricted first. A requirement
        // the pool cannot meet is recorded rather than abandoned, so the walk goes on and
        // reports everything still owed instead of only the first thing it tripped over.
        for (needed, color) in [
            (cost.white, Color::White),
            (cost.blue, Color::Blue),
            (cost.black, Color::Black),
            (cost.red, Color::Red),
            (cost.green, Color::Green),
        ] {
            let mut want = needed;
            want -= pool.spend_restricted(want, Some(color), purpose);
            let slot = pool.color_slot(color);
            let spent = want.min(*slot);
            *slot -= spent;
            *owed.color_slot(color) = want - spent;
        }
        // `{C}` requires colorless specifically (CR 107.4c); no colored mana, restricted
        // or otherwise, can pay it.
        let colorless = cost.colorless.min(pool.colorless);
        pool.colorless -= colorless;
        owed.colorless = cost.colorless - colorless;

        let mut generic = cost.generic;
        generic -= pool.spend_restricted(generic, None, purpose);
        for slot in [
            &mut pool.colorless,
            &mut pool.white,
            &mut pool.blue,
            &mut pool.black,
            &mut pool.red,
            &mut pool.green,
        ] {
            let spent = generic.min(*slot);
            *slot -= spent;
            generic -= spent;
            if generic == 0 {
                break;
            }
        }
        owed.generic = generic;
        pool.restricted.retain(|entry| entry.amount > 0);
        (pool, owed)
    }

    /// Spend up to `wanted` restricted mana usable for `purpose` — of `color` when one
    /// is named, of any color otherwise — and report how much was spent.
    fn spend_restricted(
        &mut self,
        wanted: u8,
        color: Option<Color>,
        purpose: SpendPurpose<'_>,
    ) -> u8 {
        let mut spent = 0;
        for entry in &mut self.restricted {
            if spent >= wanted {
                break;
            }
            if color.is_some_and(|wanted_color| entry.color != wanted_color) {
                continue;
            }
            if !entry.restriction.allows(purpose) {
                continue;
            }
            let take = entry.amount.min(wanted - spent);
            entry.amount -= take;
            spent += take;
        }
        spent
    }

    /// A mutable handle on the slot holding `color`.
    fn color_slot(&mut self, color: Color) -> &mut u8 {
        match color {
            Color::White => &mut self.white,
            Color::Blue => &mut self.blue,
            Color::Black => &mut self.black,
            Color::Red => &mut self.red,
            Color::Green => &mut self.green,
        }
    }

    /// How much unrestricted mana of `color` this pool holds.
    #[must_use]
    pub fn color_amount(&self, color: Color) -> u8 {
        match color {
            Color::White => self.white,
            Color::Blue => self.blue,
            Color::Black => self.black,
            Color::Red => self.red,
            Color::Green => self.green,
        }
    }

    /// The pool as a list of pip strings (e.g. `["{G}", "{G}"]`), colorless last and
    /// restricted mana last of all.
    ///
    /// Used to build the protocol's server-computed mana-pool display. Restricted mana
    /// is marked so a player can see that two of their five red are not general-purpose;
    /// the marker is display text the client renders and never parses.
    #[must_use]
    pub fn pips(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (count, color) in [
            (self.white, Color::White),
            (self.blue, Color::Blue),
            (self.black, Color::Black),
            (self.red, Color::Red),
            (self.green, Color::Green),
        ] {
            for _ in 0..count {
                out.push(color.pip().to_string());
            }
        }
        for _ in 0..self.colorless {
            out.push("{C}".to_string());
        }
        for entry in &self.restricted {
            for _ in 0..entry.amount {
                out.push(format!("{}*", entry.color.pip()));
            }
        }
        out
    }
}

/// A mana cost broken into its generic and per-color (and colorless) parts.
///
/// Produced by [`parse_mana_cost`] from `{...}` notation. `generic` may be paid
/// with mana of any color; the colored fields must be paid in kind.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ManaCost {
    /// Generic requirement (e.g. the `2` in `{2}{G}`), payable with any mana.
    pub generic: u8,
    /// White pips required.
    pub white: u8,
    /// Blue pips required.
    pub blue: u8,
    /// Black pips required.
    pub black: u8,
    /// Red pips required.
    pub red: u8,
    /// Green pips required.
    pub green: u8,
    /// Colorless pips required (`{C}`), distinct from generic.
    pub colorless: u8,
}

impl ManaCost {
    /// A mutable handle on the slot holding `color`.
    fn color_slot(&mut self, color: Color) -> &mut u8 {
        match color {
            Color::White => &mut self.white,
            Color::Blue => &mut self.blue,
            Color::Black => &mut self.black,
            Color::Red => &mut self.red,
            Color::Green => &mut self.green,
        }
    }

    /// Total of all colored and colorless (non-generic) requirements.
    #[must_use]
    pub fn colored_total(&self) -> u16 {
        u16::from(self.white)
            + u16::from(self.blue)
            + u16::from(self.black)
            + u16::from(self.red)
            + u16::from(self.green)
            + u16::from(self.colorless)
    }
}

/// Parse a mana cost in `{...}` notation into a [`ManaCost`].
///
/// Recognizes numeric generic pips (`{2}`), the five colors (`{W}{U}{B}{R}{G}`),
/// and colorless (`{C}`). An empty string parses to a zero cost (e.g. a land).
/// Unrecognized symbols are ignored so the parser degrades gracefully on richer
/// costs that later cards may introduce.
#[must_use]
pub fn parse_mana_cost(text: &str) -> ManaCost {
    let mut cost = ManaCost::default();
    let mut symbol = String::new();
    let mut in_symbol = false;
    for ch in text.chars() {
        match ch {
            '{' => {
                in_symbol = true;
                symbol.clear();
            }
            '}' => {
                if in_symbol {
                    apply_symbol(&mut cost, &symbol);
                }
                in_symbol = false;
            }
            _ if in_symbol => symbol.push(ch),
            _ => {}
        }
    }
    cost
}

/// Fold one `{...}` symbol's contents into `cost`.
fn apply_symbol(cost: &mut ManaCost, symbol: &str) {
    if let Ok(generic) = symbol.parse::<u8>() {
        cost.generic = cost.generic.saturating_add(generic);
        return;
    }
    match symbol {
        "W" => cost.white = cost.white.saturating_add(1),
        "U" => cost.blue = cost.blue.saturating_add(1),
        "B" => cost.black = cost.black.saturating_add(1),
        "R" => cost.red = cost.red.saturating_add(1),
        "G" => cost.green = cost.green.saturating_add(1),
        "C" => cost.colorless = cost.colorless.saturating_add(1),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_single_colored_pip() {
        let cost = parse_mana_cost("{G}");
        assert_eq!(cost.green, 1);
        assert_eq!(cost.generic, 0);
    }

    #[test]
    fn parse_generic_and_colored() {
        let cost = parse_mana_cost("{2}{G}");
        assert_eq!(cost.generic, 2);
        assert_eq!(cost.green, 1);
    }

    #[test]
    fn empty_cost_parses_to_zero() {
        assert_eq!(parse_mana_cost(""), ManaCost::default());
    }

    #[test]
    fn add_and_can_pay_a_colored_cost() {
        let mut pool = ManaPool::default();
        pool.add(Color::Green, 1);
        assert!(pool.can_pay(&parse_mana_cost("{G}")));
        assert!(!pool.can_pay(&parse_mana_cost("{G}{G}")));
    }

    #[test]
    fn generic_is_paid_by_any_color() {
        let mut pool = ManaPool::default();
        pool.add(Color::Green, 3);
        let cost = parse_mana_cost("{2}{G}");
        assert!(pool.can_pay(&cost));
        let after = pool.pay(&cost).expect("payable");
        assert_eq!(after.green, 0);
        assert_eq!(after.total(), 0);
    }

    #[test]
    fn pay_returns_none_when_unaffordable() {
        let pool = ManaPool::default();
        assert!(pool.pay(&parse_mana_cost("{G}")).is_none());
    }

    #[test]
    fn pips_lists_each_mana() {
        let mut pool = ManaPool::default();
        pool.add(Color::Green, 2);
        assert_eq!(pool.pips(), vec!["{G}".to_string(), "{G}".to_string()]);
    }
}
