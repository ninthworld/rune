//! The commander designation and its per-designation bookkeeping (CR 903).
//!
//! A commander is a card a player designates before the game (CR 903.3); it
//! begins in the command zone and may be cast from there (CR 903.8). Two facts
//! about a commander outlive any single object it becomes and so cannot hang off
//! a [`PermanentId`](crate::PermanentId) — which is minted fresh on every
//! battlefield entry (`crates/rune-engine/AGENTS.md`):
//!
//! - the **commander tax** (CR 903.8): each cast from the command zone this game
//!   makes the next cost `{2}` more, so the count of prior casts is a running
//!   total that must survive the commander leaving and re-entering the command
//!   zone; and
//! - the pending **return-to-command-zone** decision (CR 903.9a): when the
//!   commander is in a graveyard or exile its owner may move it to the command
//!   zone instead, a choice offered at the next state-based check.
//!
//! Both are keyed to the *designation* (the player and their commander card /
//! instance), never to a battlefield object — a recast commander is a brand new
//! object but the same designation, so its tax keeps climbing. This is the raw
//! stored state the engine forbids deriving; it is carried on
//! [`Player::commander`](crate::Player::commander).
//!
//! Deck legality — singleton, color identity, the 40-life commander *format* — is
//! **not** modeled here; it stays server-side (ADR 0013 §4). The engine only sees
//! which card was designated, as setup data.

use crate::id::{CardId, CardInstanceId};
use crate::mana::ManaCost;

/// The generic mana the commander tax adds per prior cast from the command zone
/// (CR 903.8: "that costs an additional `{2}` for each previous time").
pub const COMMANDER_TAX_PER_CAST: u32 = 2;

/// How much **combat** damage a single commander must have dealt one player over
/// the game for that player to lose (CR 903.10a: "21 or more combat damage from
/// any one commander").
///
/// The tally this compares against is cumulative across combats and keyed to the
/// commander *designation*, so it survives the commander's zone changes and
/// recasts (a fresh [`PermanentId`](crate::PermanentId) every battlefield entry)
/// — see [`GameState::commander_damage`](crate::GameState::commander_damage). The
/// state-based-actions loop ([`crate::sba::run_state_based_actions`]) applies the
/// loss; in a game of three or more the loser is eliminated through the existing
/// CR 800.4a leave-the-game path.
pub const COMMANDER_DAMAGE_LOSS_THRESHOLD: u32 = 21;

/// A player's commander designation and the bookkeeping that outlives every
/// object the commander becomes (CR 903).
///
/// Carried on [`Player::commander`](crate::Player::commander) for the whole game
/// once set at setup. The commander *card* itself lives in whatever zone it
/// currently occupies (the command zone, the stack, the battlefield, a
/// graveyard, or exile); this record is the persistent identity and counters
/// that a bare zone snapshot cannot recover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommanderState {
    /// The designated commander card (CR 903.3).
    pub card: CardId,
    /// The physical instance that is this player's commander. Stable for the
    /// whole game (only a [`PermanentId`](crate::PermanentId) is reborn on
    /// battlefield entry), so it identifies the commander in any zone — which
    /// card in the graveyard is the commander, which card in the command zone to
    /// cast.
    pub instance: CardInstanceId,
    /// How many times this commander has been cast from the command zone this
    /// game (CR 903.8). The tax is [`COMMANDER_TAX_PER_CAST`] generic mana per
    /// prior cast, so a commander cast for the third time pays `{4}` more.
    /// Incremented as the cast is put on the stack; never reset — it is keyed to
    /// the designation, not to any object.
    pub casts: u32,
    /// Whether a CR 903.9a return-to-command-zone decision is currently owed to
    /// this commander's owner because the commander is sitting in a graveyard or
    /// exile and the owner has not yet decided. Set when the commander leaves the
    /// battlefield for a graveyard
    /// ([`GameState::move_permanent_to_graveyard`](crate::GameState)) or for exile
    /// ([`GameState::move_permanent_to_exile`](crate::GameState)) — the two
    /// battlefield-leaves seams both flag it identically — and cleared when the owner
    /// accepts (moving it to the command zone) or declines (leaving it where it went).
    /// Raw stored state: a bare snapshot cannot tell "the commander is in the graveyard
    /// or exile *and a choice is still pending*" from "…and the owner already declined".
    pub return_pending: bool,
}

impl CommanderState {
    /// A fresh designation for `instance` of `card`, with no casts yet and no
    /// return decision pending (its state at setup, in the command zone).
    #[must_use]
    pub fn new(card: CardId, instance: CardInstanceId) -> Self {
        Self {
            card,
            instance,
            casts: 0,
            return_pending: false,
        }
    }

    /// The generic mana this commander's tax adds to a cast right now (CR 903.8):
    /// [`COMMANDER_TAX_PER_CAST`] per prior cast from the command zone.
    #[must_use]
    pub fn tax_generic(&self) -> u32 {
        self.casts.saturating_mul(COMMANDER_TAX_PER_CAST)
    }
}

/// `base` plus the commander tax for a commander that has been cast `casts` times
/// from the command zone (CR 903.8): the generic portion grows by
/// [`COMMANDER_TAX_PER_CAST`] per prior cast, colored requirements untouched.
///
/// A pure cost transform used both to decide payability in
/// [`valid_actions`](crate::valid_actions) and to charge the cast in
/// [`apply_action`](crate::apply_action), so the offered cost and the paid cost
/// can never disagree. Saturating, so a pathological cast count cannot overflow
/// the `u8` generic field.
#[must_use]
pub fn commander_tax_cost(base: &ManaCost, casts: u32) -> ManaCost {
    let extra = casts.saturating_mul(COMMANDER_TAX_PER_CAST);
    let extra = u8::try_from(extra).unwrap_or(u8::MAX);
    let mut taxed = base.clone();
    taxed.generic = taxed.generic.saturating_add(extra);
    taxed
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::id::CardInstanceId;
    use crate::mana::parse_mana_cost;

    #[test]
    fn cr_903_8_tax_adds_two_generic_per_prior_cast() {
        // CR 903.8: {2} more for each previous time cast from the command zone.
        let base = parse_mana_cost("{2}{G}"); // generic 2, one green
        assert_eq!(commander_tax_cost(&base, 0), base, "first cast: no tax");
        assert_eq!(commander_tax_cost(&base, 1).generic, 4, "second cast: +2");
        assert_eq!(commander_tax_cost(&base, 2).generic, 6, "third cast: +4");
        // The colored requirement is never taxed.
        assert_eq!(commander_tax_cost(&base, 3).green, 1);
    }

    #[test]
    fn tax_generic_tracks_casts() {
        let mut c = CommanderState::new(CardId(0), CardInstanceId(1));
        assert_eq!(c.tax_generic(), 0);
        c.casts = 3;
        assert_eq!(c.tax_generic(), 6);
    }
}
