//! Combat restrictions a card carries or is granted (CR 506.3, CR 509.1b).

use serde::Deserialize;

use crate::mana::Color;

/// A rule about how a creature may take part in combat — the vocabulary beyond the
/// keyword abilities (CR 506.3, "restrictions on which creatures can attack or block",
/// and CR 509.1b, "evasion abilities").
///
/// Almost every member narrows what is legal, which is what the name says. One
/// ([`Self::CanBlockAdditional`]) widens it instead, and it belongs here rather than in
/// a second enum because it is the same *kind* of thing everywhere it matters: printed
/// in the same list, granted by the same layer-6 machinery, and consulted by the same
/// declare-blockers gate that judges the restrictions it sits beside.
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
/// A closed, plain-data enum (ADR 0003), deserialized externally tagged: a
/// unit variant is its bare `snake_case` name and a parameterized one wraps its
/// payload, e.g. `["cant_attack", {"cant_be_blocked_by": "black"}]`.
///
/// `Clone` rather than `Copy`: a subtype is an open-ended string
/// ([`Self::CantBeBlockedExceptBy`]), exactly as it is everywhere else the engine
/// names one, so the enum owns one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
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
    /// This creature can't be blocked **except** by creatures of the named subtype
    /// (CR 509.1b) — `Departed Deckhand can't be blocked except by Spirits.` The one
    /// evasion in this vocabulary stated as a permission rather than a prohibition,
    /// which is why it is a separate variant instead of a negation of
    /// [`Self::CantBeBlockedBy`]: a subtype it does not name is forbidden, and the
    /// colour form forbids exactly what it names.
    ///
    /// Enforced in
    /// [`blocker_can_block_attacker`](crate::combat::blocker_can_block_attacker), the
    /// pairwise gate the colour and power forms already live in.
    ///
    /// The blocker's subtypes are read through the **computed**
    /// [`Characteristics`](crate::Characteristics), like the power form and unlike the
    /// colour one: CR 613 layer 4 (type-changing effects) is not implemented yet, so
    /// today the computed subtypes *are* the printed ones — but this is the read path
    /// that becomes correct on its own the day that layer lands, with no call site to
    /// remember.
    CantBeBlockedExceptBy(String),

    /// This creature **may block the named number of additional creatures** each combat
    /// (CR 509.1a) — the one member of this vocabulary that *lifts* a limit instead of
    /// imposing one. A blocker blocks one attacker unless an effect says otherwise, and
    /// this is that effect: a value of 1 is "can block an additional creature", 2 is "up
    /// to two additional creatures", and so on.
    ///
    /// It lives here rather than beside [`Keyword`](super::Keyword) for the same reasons
    /// the rest of this enum does: no card prints it as a keyword, it carries a
    /// parameter, and it is granted and read through the computed
    /// [`Characteristics`](crate::Characteristics) at CR 613 layer 6 exactly as an
    /// imposed restriction is. What it changes is a fact about the whole declaration —
    /// how many attackers *one blocker* is assigned to — so it is enforced over the
    /// assembled selection in the declare-blockers legality gate, beside menace and the
    /// blocker-count ceiling, and never in the pairwise check.
    ///
    /// It is a **permission, not a requirement**: nothing about it forces a creature to
    /// block at all, or to use the extra assignment when it does.
    CanBlockAdditional(u32),

    /// **Every creature able to block this creature does so** (CR 509.1c) — the one
    /// member of this vocabulary that *requires* part of a declaration rather than
    /// permitting or forbidding it.
    ///
    /// A requirement is not a restriction turned around, and the difference is the whole
    /// reason this variant is enforced where it is. A restriction says a declaration is
    /// illegal because of something it *contains*, which the pairwise gate and the
    /// per-count gates beside it can each answer by looking at what was submitted.
    /// CR 509.1c says a declaration is illegal because of something it **omits**: the
    /// declaration chosen must obey the maximum possible number of requirements without
    /// violating any restriction, so judging one means knowing what *other* declarations
    /// could have achieved. That is a search over the whole declaration
    /// ([`max_block_requirements_met`](crate::max_block_requirements_met)), and it is
    /// enforced in exactly one place — the declare-blockers legality gate, beside menace
    /// and the block-count bounds.
    ///
    /// **Restrictions win** (CR 509.1c). A requirement is met only by a declaration that
    /// is legal in the first place, so a creature this restriction cannot legally be
    /// blocked by — because it has flying and the blocker does not, because the blocker
    /// can't block, because a menace floor is unreachable — is simply not required to
    /// block it, and a requirement that cannot be met is not met. Nothing here can make
    /// an illegal declaration legal.
    ///
    /// "Able" is judged per candidate blocker and per pair, exactly as the blocker slot's
    /// candidate list is: a tapped creature is not able, nor is one that can't block, nor
    /// one the attacker's evasion excludes. The requirement names no blocker of its own —
    /// it names an attacker, and the set of creatures it binds is whatever the board
    /// makes able at the moment the declaration is judged.
    MustBeBlockedByAllAble,
}

impl CombatRestriction {
    /// The colour this restriction forbids as a blocker, if it names one.
    ///
    /// A named accessor rather than a `matches!` at each call site, so the two
    /// blocking gates ask the same question of the same variant.
    #[must_use]
    pub fn forbidden_blocker_color(&self) -> Option<Color> {
        match self {
            CombatRestriction::CantBeBlockedBy(color) => Some(*color),
            CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlocked
            | CombatRestriction::CantBeBlockedByMoreThanOne
            | CombatRestriction::CantBeBlockedByPowerOrLess(_)
            | CombatRestriction::CantBeBlockedExceptBy(_)
            | CombatRestriction::CanBlockAdditional(_)
            | CombatRestriction::MustBeBlockedByAllAble => None,
        }
    }

    /// The power at or below which this restriction forbids a blocker, if it names one.
    ///
    /// The numeric counterpart of [`Self::forbidden_blocker_color`], and a named
    /// accessor for the same reason: one question, asked in one place.
    #[must_use]
    pub fn forbidden_blocker_power(&self) -> Option<i32> {
        match self {
            CombatRestriction::CantBeBlockedByPowerOrLess(power) => Some(*power),
            CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlocked
            | CombatRestriction::CantBeBlockedBy(_)
            | CombatRestriction::CantBeBlockedByMoreThanOne
            | CombatRestriction::CantBeBlockedExceptBy(_)
            | CombatRestriction::CanBlockAdditional(_)
            | CombatRestriction::MustBeBlockedByAllAble => None,
        }
    }

    /// The subtype a blocker must have for this restriction to let it through, if this
    /// restriction names one.
    ///
    /// The third named accessor beside [`Self::forbidden_blocker_color`] and
    /// [`Self::forbidden_blocker_power`], and the one whose sense is inverted: what it
    /// returns is what a blocker must **be**, not what it must not be.
    #[must_use]
    pub fn required_blocker_subtype(&self) -> Option<&str> {
        match self {
            CombatRestriction::CantBeBlockedExceptBy(subtype) => Some(subtype),
            CombatRestriction::CanBlockAdditional(_) => None,
            CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlocked
            | CombatRestriction::CantBeBlockedBy(_)
            | CombatRestriction::CantBeBlockedByMoreThanOne
            | CombatRestriction::CantBeBlockedByPowerOrLess(_)
            | CombatRestriction::MustBeBlockedByAllAble => None,
        }
    }

    /// How many creatures **beyond the first** this permission lets a blocker block
    /// (CR 509.1a), if it is that permission.
    ///
    /// The fourth named accessor, for the same reason as the other three: the
    /// declare-blockers gate folds every such permission a creature currently has into
    /// one allowance, and asks this one question of every restriction to do it.
    ///
    /// Takes `&self` rather than `self`: a restriction naming a subtype carries a
    /// `String`, so this vocabulary is `Clone` and no longer `Copy`.
    #[must_use]
    pub fn additional_blocks(&self) -> Option<u32> {
        match self {
            CombatRestriction::CanBlockAdditional(count) => Some(*count),
            CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlocked
            | CombatRestriction::CantBeBlockedBy(_)
            | CombatRestriction::CantBeBlockedByMoreThanOne
            | CombatRestriction::CantBeBlockedByPowerOrLess(_)
            | CombatRestriction::CantBeBlockedExceptBy(_)
            | CombatRestriction::MustBeBlockedByAllAble => None,
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
                            {"cant_be_blocked_by":"black"},"cant_be_blocked_by_more_than_one",
                            {"cant_be_blocked_except_by":"Spirit"}]}]"#;
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
                CombatRestriction::CantBeBlockedExceptBy("Spirit".to_string()),
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
            CombatRestriction::CantBeBlockedExceptBy("Spirit".to_string()),
        ] {
            assert_eq!(other.forbidden_blocker_color(), None);
        }
    }

    #[test]
    fn issue_742_only_the_subtype_restriction_names_a_subtype() {
        assert_eq!(
            CombatRestriction::CantBeBlockedExceptBy("Spirit".to_string())
                .required_blocker_subtype(),
            Some("Spirit")
        );
        for other in [
            CombatRestriction::CantAttack,
            CombatRestriction::CantBlock,
            CombatRestriction::CantBeBlocked,
            CombatRestriction::CantBeBlockedBy(Color::Black),
            CombatRestriction::CantBeBlockedByMoreThanOne,
            CombatRestriction::CantBeBlockedByPowerOrLess(2),
            CombatRestriction::CanBlockAdditional(1),
        ] {
            assert_eq!(other.required_blocker_subtype(), None);
        }
    }

    #[test]
    fn issue_739_the_block_permission_parses_with_its_count_and_names_only_itself() {
        // The one permission in the vocabulary, authored in the same list as the
        // restrictions and read back with the count it names — and nothing else in the
        // enum answers the question it answers.
        let json = r#"[{"schema_version":1,"functional_id":"extra_blocker",
            "name":"Extra Blocker","types":["creature"],"mana_cost":"","power":1,"toughness":1,
            "restrictions":[{"can_block_additional":2},"cant_attack"]}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        let card = crate::card::tests::card_named(&db, "extra_blocker");
        assert_eq!(
            card.restrictions,
            vec![
                CombatRestriction::CanBlockAdditional(2),
                CombatRestriction::CantAttack,
            ]
        );

        assert_eq!(
            CombatRestriction::CanBlockAdditional(2).additional_blocks(),
            Some(2)
        );
        for other in [
            CombatRestriction::CantAttack,
            CombatRestriction::CantBlock,
            CombatRestriction::CantBeBlocked,
            CombatRestriction::CantBeBlockedBy(Color::Black),
            CombatRestriction::CantBeBlockedByMoreThanOne,
            CombatRestriction::CantBeBlockedByPowerOrLess(2),
        ] {
            assert_eq!(other.additional_blocks(), None);
        }
        // And the permission answers neither of the questions the evasion restrictions do.
        assert_eq!(
            CombatRestriction::CanBlockAdditional(1).forbidden_blocker_color(),
            None
        );
        assert_eq!(
            CombatRestriction::CanBlockAdditional(1).forbidden_blocker_power(),
            None
        );
    }

    #[test]
    fn issue_739_the_block_requirement_parses_as_a_bare_name_and_answers_nothing_else() {
        // The requirement is a unit variant, so it is authored exactly as the unit
        // restrictions beside it — and, like them, it answers none of the payload
        // questions the parameterized members answer. What it *does* answer is asked of
        // the whole declaration, not of this enum.
        let json = r#"[{"schema_version":1,"functional_id":"must_be_blocked",
            "name":"Must Be Blocked","types":["creature"],"mana_cost":"","power":1,"toughness":1,
            "restrictions":["must_be_blocked_by_all_able","cant_be_blocked_by_more_than_one"]}]"#;
        let db = crate::card::CardDatabase::from_json(json).unwrap();
        let card = crate::card::tests::card_named(&db, "must_be_blocked");
        assert_eq!(
            card.restrictions,
            vec![
                CombatRestriction::MustBeBlockedByAllAble,
                CombatRestriction::CantBeBlockedByMoreThanOne,
            ]
        );

        let requirement = CombatRestriction::MustBeBlockedByAllAble;
        assert_eq!(requirement.forbidden_blocker_color(), None);
        assert_eq!(requirement.forbidden_blocker_power(), None);
        assert_eq!(requirement.required_blocker_subtype(), None);
        assert_eq!(requirement.additional_blocks(), None);
    }
}
