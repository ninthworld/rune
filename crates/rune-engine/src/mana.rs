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
}

/// A quantity of mana held per color, plus colorless.
///
/// This is a player's mana pool. It stores raw counts only; nothing is derived
/// or cached here.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
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

    /// Total mana of every color and colorless currently in the pool.
    #[must_use]
    pub fn total(&self) -> u16 {
        u16::from(self.white)
            + u16::from(self.blue)
            + u16::from(self.black)
            + u16::from(self.red)
            + u16::from(self.green)
            + u16::from(self.colorless)
    }

    /// Whether `cost` can be paid from this pool: every colored (and colorless)
    /// requirement is covered, and enough mana remains for the generic portion.
    #[must_use]
    pub fn can_pay(&self, cost: &ManaCost) -> bool {
        self.white >= cost.white
            && self.blue >= cost.blue
            && self.black >= cost.black
            && self.red >= cost.red
            && self.green >= cost.green
            && self.colorless >= cost.colorless
            && self.total() - cost.colored_total() >= u16::from(cost.generic)
    }

    /// Pay `cost`, returning the resulting pool, or `None` if it cannot be paid.
    ///
    /// Colored and colorless requirements are paid from their own colors first;
    /// the generic portion is then paid deterministically from colorless, then
    /// white, blue, black, red, green.
    #[must_use]
    pub fn pay(&self, cost: &ManaCost) -> Option<Self> {
        if !self.can_pay(cost) {
            return None;
        }
        let mut pool = self.clone();
        pool.white -= cost.white;
        pool.blue -= cost.blue;
        pool.black -= cost.black;
        pool.red -= cost.red;
        pool.green -= cost.green;
        pool.colorless -= cost.colorless;

        let mut generic = cost.generic;
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
        Some(pool)
    }

    /// The pool as a list of pip strings (e.g. `["{G}", "{G}"]`), colorless last.
    ///
    /// Used to build the protocol's server-computed mana-pool display.
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
