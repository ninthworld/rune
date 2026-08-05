//! Combat restrictions a card carries or is granted (CR 506.3, CR 509.1b).

use serde::Deserialize;

use crate::mana::Color;

/// A restriction on how a creature may take part in combat — the vocabulary beyond
/// the keyword abilities (CR 506.3, "restrictions on which creatures can attack or
/// block", and CR 509.1b, "evasion abilities").
///
/// Separate from [`Keyword`](super::Keyword) because these are not keyword abilities:
/// no card prints the word "can't be blocked" as a keyword, several of them carry a
/// parameter ([`Self::CantBeBlockedBy`] names a colour), and a card can be *granted*
/// one without gaining anything a player would name. They are nonetheless read the
/// same way keywords are — through the computed
/// [`Characteristics`](crate::Characteristics), at CR 613 layer 6 — so a restriction
/// an Aura or a spell grants binds exactly as a printed one does, and stops binding
/// the instant the grant ends.
///
/// Each variant is enforced in **exactly one place**, and the doc comment on each says
/// where. Splitting one restriction across two gates is how the two halves drift.
///
/// A closed, `Copy`, plain-data enum (ADR 0003), deserialized externally tagged: a
/// unit variant is its bare `snake_case` name and a parameterized one wraps its
/// payload, e.g. `["cant_attack", {"cant_be_blocked_by": "black"}]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatRestriction {
    /// This creature can't attack (CR 506.3a) — the restriction an Aura like a
    /// pacifism effect carries. Enforced by removing it from
    /// [`attacker_candidates`](crate::combat::attacker_candidates), the one place the
    /// [`Defender`](super::Keyword::Defender) keyword is enforced.
    ///
    /// Distinct from `defender` even though the two forbid the same thing: defender
    /// is a printed keyword a player reads on the type-shaped body of a Wall, and
    /// granting it would put a word on the card that the card does not have.
    CantAttack,
    /// This creature can't block (CR 506.3c). Enforced by removing it from
    /// [`blocker_candidates_for`](crate::combat::blocker_candidates_for).
    CantBlock,
    /// This creature can't be blocked (CR 509.1b) — plain unblockability. Enforced in
    /// [`blocker_can_block_attacker`](crate::combat::blocker_can_block_attacker), the
    /// pairwise gate flying already lives in: no blocker at all may be assigned to it.
    CantBeBlocked,
    /// This creature can't be blocked by creatures of the named colour (CR 509.1b) —
    /// the Mare cycle's evasion. Enforced in
    /// [`blocker_can_block_attacker`](crate::combat::blocker_can_block_attacker).
    ///
    /// The blocker's colour is its **printed** [`CardData::colors`](super::CardData::colors):
    /// CR 613 layer 5 (colour-changing effects) is not implemented, so printed colour
    /// is current colour, exactly as printed types stand in for current types
    /// everywhere else in the engine. When that layer lands this is one of the call
    /// sites that must start reading a computed colour.
    CantBeBlockedBy(Color),
    /// This creature can't be blocked by more than one creature (CR 509.1b) — the
    /// mirror of menace, a ceiling on the block rather than a floor.
    ///
    /// Like menace (CR 702.110b) this is a fact about the **whole** declaration rather
    /// than about any one pair — a second blocker is illegal precisely because it is
    /// not alone — so it is enforced over the assembled selection in the
    /// declare-blockers legality gate, beside menace, and never in the pairwise check.
    CantBeBlockedByMoreThanOne,
    /// This creature can't be blocked by creatures whose power is at most the named
    /// amount (CR 509.1b) — the same shape of evasion [`Self::CantBeBlockedBy`] carries,
    /// naming a number instead of a colour. Enforced in
    /// [`blocker_can_block_attacker`](crate::combat::blocker_can_block_attacker), the
    /// pairwise gate the colour form already lives in.
    ///
    /// The blocker's power is read through the **computed**
    /// [`Characteristics`](crate::Characteristics), not its printed face: a 1/1 pumped
    /// out of range really can block, and one shrunk into range really cannot. That is
    /// the opposite of the colour form, which reads printed colour only because CR 613
    /// layer 5 is not implemented.
    CantBeBlockedByPowerOrLess(i32),
}

impl CombatRestriction {
    /// The colour this restriction forbids as a blocker, if it names one.
    ///
    /// A named accessor rather than a `matches!` at each call site, so the two
    /// blocking gates ask the same question of the same variant.
    #[must_use]
    pub fn forbidden_blocker_color(self) -> Option<Color> {
        match self {
            CombatRestriction::CantBeBlockedBy(color) => Some(color),
            CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlocked
            | CombatRestriction::CantBeBlockedByMoreThanOne
            | CombatRestriction::CantBeBlockedByPowerOrLess(_) => None,
        }
    }

    /// The power at or below which this restriction forbids a blocker, if it names one.
    ///
    /// The numeric counterpart of [`Self::forbidden_blocker_color`], and a named
    /// accessor for the same reason: one question, asked in one place.
    #[must_use]
    pub fn forbidden_blocker_power(self) -> Option<i32> {
        match self {
            CombatRestriction::CantBeBlockedByPowerOrLess(power) => Some(power),
            CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlocked
            | CombatRestriction::CantBeBlockedBy(_)
            | CombatRestriction::CantBeBlockedByMoreThanOne => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn issue_606_restrictions_deserialize_bare_and_parameterized() {
        // The wire form is externally tagged: a unit restriction is its bare name, a
        // parameterized one wraps its payload. Both shapes appear in one list on a
        // real card (Vine Mare prints hexproof beside a colour restriction).
        let json = r#"[{"schema_version":1,"functional_id":"every_restriction",
            "name":"Every Restriction","types":["creature"],"mana_cost":"","power":1,"toughness":1,
            "restrictions":["cant_attack","cant_block","cant_be_blocked",
                            {"cant_be_blocked_by":"black"},"cant_be_blocked_by_more_than_one"]}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        let card = crate::card::tests::card_named(&db, "every_restriction");
        assert_eq!(
            card.restrictions,
            vec![
                CombatRestriction::CantAttack,
                CombatRestriction::CantBlock,
                CombatRestriction::CantBeBlocked,
                CombatRestriction::CantBeBlockedBy(Color::Black),
                CombatRestriction::CantBeBlockedByMoreThanOne,
            ]
        );
    }

    #[test]
    fn issue_606_only_the_colour_restriction_names_a_colour() {
        assert_eq!(
            CombatRestriction::CantBeBlockedBy(Color::White).forbidden_blocker_color(),
            Some(Color::White)
        );
        for other in [
            CombatRestriction::CantAttack,
            CombatRestriction::CantBlock,
            CombatRestriction::CantBeBlocked,
            CombatRestriction::CantBeBlockedByMoreThanOne,
        ] {
            assert_eq!(other.forbidden_blocker_color(), None);
        }
    }
}
