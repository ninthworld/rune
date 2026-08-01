//! Keyword abilities printed on a card (CR 702).

use serde::Deserialize;

/// A keyword ability printed on a card (CR 702). Closed set, deserialized from
/// lowercase names (e.g. `"flying"`, `"first_strike"`).
///
/// This is the printed keyword representation the layer system seeds from; a
/// permanent's *current* keywords are the printed [`super::CardData::keywords`] unioned
/// with any granted by continuous effects at CR 613 layer 6 (see
/// [`characteristics`](crate::characteristics::characteristics)). All twelve variants are
/// enforced: [`Flying`](Keyword::Flying), [`Reach`](Keyword::Reach),
/// [`Vigilance`](Keyword::Vigilance), [`Haste`](Keyword::Haste),
/// [`Defender`](Keyword::Defender), and [`Menace`](Keyword::Menace) at
/// combat-declaration time (keywords I),
/// [`FirstStrike`](Keyword::FirstStrike), [`Trample`](Keyword::Trample),
/// [`Deathtouch`](Keyword::Deathtouch), [`Lifelink`](Keyword::Lifelink), and
/// [`DoubleStrike`](Keyword::DoubleStrike) at combat-damage time (keywords II — see
/// [`crate::combat::combat_damage`]), and [`Hexproof`](Keyword::Hexproof) at
/// targeting time, which is not a combat gate at all.
///
/// Restrictions that are **not** keyword abilities — "can't be blocked", "can't be
/// blocked by black creatures" — live in [`CombatRestriction`](super::CombatRestriction)
/// and are read through the same computed characteristics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keyword {
    /// Flying (CR 702.9): can be blocked only by creatures with flying or reach.
    Flying,
    /// Reach (CR 702.17): can block creatures with flying.
    Reach,
    /// Vigilance (CR 702.20): attacking doesn't cause the creature to tap.
    Vigilance,
    /// Haste (CR 702.10): ignores the summoning-sickness restriction on attacking.
    Haste,
    /// Defender (CR 702.3): can't attack. A pure attack-declaration restriction —
    /// it does not stop the creature blocking, being tapped for a cost, or dealing
    /// combat damage, so it is enforced in exactly one place
    /// ([`attacker_candidates`](crate::combat::attacker_candidates)).
    Defender,
    /// Menace (CR 702.110): can't be blocked except by two or more creatures.
    ///
    /// Unlike flying (CR 702.9c), this is a constraint on the *whole* block
    /// declaration rather than on one blocker/attacker pair: a single legal blocker
    /// is illegal precisely because it is alone. It is therefore checked over the
    /// assembled selection in the declare-blockers legality gate, not by
    /// [`blocker_can_block_attacker`](crate::combat::blocker_can_block_attacker).
    Menace,
    /// First strike (CR 702.7): deals combat damage in a first combat-damage step.
    FirstStrike,
    /// Trample (CR 702.19): a blocked creature assigns excess combat damage to the
    /// player it is attacking.
    Trample,
    /// Deathtouch (CR 702.2): any nonzero damage it deals is lethal.
    Deathtouch,
    /// Lifelink (CR 702.15): damage it deals also gains its controller that much
    /// life.
    Lifelink,
    /// Double strike (CR 702.4): deals combat damage in *both* the first-strike and
    /// the regular combat-damage step.
    DoubleStrike,
    /// Hexproof (CR 702.11): this permanent can't be the target of spells or abilities
    /// its controller's **opponents** control.
    ///
    /// Alone among the keywords here it is not a combat gate: it is a targeting
    /// restriction, so it is enforced in
    /// [`target_is_legal`](crate::resolve::target_is_legal) — the one predicate both
    /// the announcement gate and the CR 608.2b resolution re-check run — rather than
    /// anywhere in combat. It is controller-relative: a hexproof creature's *own*
    /// controller may target it freely, which is what makes a pump spell on one's own
    /// hexproof creature legal.
    Hexproof,
    /// Indestructible (CR 702.12): this permanent is not destroyed by lethal damage or
    /// by an effect that says "destroy".
    ///
    /// Like [`Self::Hexproof`] it is not a combat gate, and unlike hexproof it is not a
    /// targeting one either: it is an exception to *destruction*, so it is enforced at
    /// the two places destruction happens — the CR 704.5g/704.5h state-based actions and
    /// the [`Effect::Destroy`](crate::Effect) verb. It is deliberately **not** an
    /// exception to anything else: a creature with 0 or less toughness is put into its
    /// owner's graveyard by CR 704.5f, which is not destruction, and an indestructible
    /// planeswalker at zero loyalty still leaves under CR 704.5i for the same reason.
    /// Sacrifice, exile, and bounce are likewise untouched.
    Indestructible,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn all_twelve_keyword_variants_deserialize_from_snake_case() {
        // The closed keyword set round-trips from its wire names, including the
        // five combat-damage variants keywords II enforces (CR 702), the two
        // declaration-time restrictions defender and menace, and hexproof, which
        // gates targeting rather than combat.
        let json = r#"[{"schema_version":1,"functional_id":"every_keyword","name":"Every Keyword","types":["creature"],
            "mana_cost":"","power":1,"toughness":1,
            "keywords":["flying","reach","vigilance","haste","defender","menace","first_strike",
                        "trample","deathtouch","lifelink","double_strike","hexproof"]}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        let card = crate::card::tests::card_named(&db, "every_keyword");
        for kw in [
            Keyword::Flying,
            Keyword::Reach,
            Keyword::Vigilance,
            Keyword::Haste,
            Keyword::Defender,
            Keyword::Menace,
            Keyword::FirstStrike,
            Keyword::Trample,
            Keyword::Deathtouch,
            Keyword::Lifelink,
            Keyword::DoubleStrike,
            Keyword::Hexproof,
        ] {
            assert!(card.has_keyword(kw), "expected keyword {kw:?}");
        }
    }
}
