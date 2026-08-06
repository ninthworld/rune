//! Rules text for the two continuous effects that are **in no CR 613 layer** (issue
//! #741): the combat-damage characteristic override and the as-though-no-defender
//! permission.
//!
//! Split from the main [`tests`](super::tests) module rather than appended to it — that
//! file is already well past the size the coding standards allow, and these tests share a
//! subject with each other rather than with it. The helpers come from there, because the
//! text they compose has to be the text every other card's is compared against.

#![allow(clippy::unwrap_used)]

use super::tests::{bundled, static_text, text_of};

/// Issue #741: the damage-assignment override and the as-though permission each say what
/// they do, and say the part that is easy to leave out.
///
/// "Rather than their power" is the whole content of the first — without it the sentence
/// describes what every creature already does — and "as though" is the whole content of
/// the second: a player has to know the keyword is still there, because their own
/// defender-counting cards can still see it.
#[test]
fn issue_741_the_override_and_the_permission_each_state_themselves() {
    assert_eq!(
        static_text(
            r#"{"scope":"creatures_you_control","keyword":"defender"}"#,
            r#"{"kind":"assigns_combat_damage_by","characteristic":"toughness"}"#
        ),
        "Creatures you control with defender assign combat damage equal to their \
         toughness rather than their power."
    );
    assert_eq!(
        static_text(
            r#"{"scope":"creatures_you_control","keyword":"defender"}"#,
            r#"{"kind":"attacks_as_though_no_defender"}"#
        ),
        "Creatures you control with defender can attack as though they didn't have \
         defender."
    );
}

/// Issue #741: a static ability about the card itself takes the singular, because the one
/// verb that refers back to its subject cannot read right in both numbers.
#[test]
fn issue_741_a_self_scoped_permission_reads_in_the_singular() {
    assert_eq!(
        text_of(&bundled(), "novice_knight"),
        "Defender\nNovice Knight can attack as though it didn't have defender as long as \
         it's enchanted or equipped."
    );
}

/// Issue #741: Arcades states all three of its clauses, the keyword-filtered trigger
/// included.
#[test]
fn issue_741_arcades_states_the_trigger_the_override_and_the_permission() {
    assert_eq!(
        text_of(&bundled(), "arcades_the_strategist"),
        "Flying, vigilance\n\
         Whenever a creature you control with defender enters the battlefield, draw a \
         card.\n\
         Creatures you control with defender assign combat damage equal to their \
         toughness rather than their power.\n\
         Creatures you control with defender can attack as though they didn't have \
         defender."
    );
}
