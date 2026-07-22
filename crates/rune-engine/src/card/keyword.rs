//! Keyword abilities printed on a card (CR 702).

use serde::Deserialize;

/// A keyword ability printed on a card (CR 702). Closed set, deserialized from
/// lowercase names (e.g. `"flying"`, `"first_strike"`).
///
/// This is the printed keyword representation the layer system seeds from; a
/// permanent's *current* keywords are the printed [`super::CardData::keywords`] unioned
/// with any granted by continuous effects at CR 613 layer 6 (see
/// [`characteristics`](crate::characteristics::characteristics)). All nine variants are
/// enforced: [`Flying`](Keyword::Flying), [`Reach`](Keyword::Reach),
/// [`Vigilance`](Keyword::Vigilance), and [`Haste`](Keyword::Haste) at
/// combat-declaration time (keywords I), and
/// [`FirstStrike`](Keyword::FirstStrike), [`Trample`](Keyword::Trample),
/// [`Deathtouch`](Keyword::Deathtouch), [`Lifelink`](Keyword::Lifelink), and
/// [`DoubleStrike`](Keyword::DoubleStrike) at combat-damage time (keywords II — see
/// [`crate::combat::combat_damage`]).
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
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn all_nine_keyword_variants_deserialize_from_snake_case() {
        // The closed keyword set round-trips from its wire names, including the
        // five combat-damage variants keywords II enforces (CR 702).
        let json = r#"[{"schema_version":1,"functional_id":"every_keyword","name":"Every Keyword","types":["creature"],
            "mana_cost":"","power":1,"toughness":1,
            "keywords":["flying","reach","vigilance","haste","first_strike",
                        "trample","deathtouch","lifelink","double_strike"]}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        let card = crate::card::tests::card_named(&db, "every_keyword");
        for kw in [
            Keyword::Flying,
            Keyword::Reach,
            Keyword::Vigilance,
            Keyword::Haste,
            Keyword::FirstStrike,
            Keyword::Trample,
            Keyword::Deathtouch,
            Keyword::Lifelink,
            Keyword::DoubleStrike,
        ] {
            assert!(card.has_keyword(kw), "expected keyword {kw:?}");
        }
    }
}
